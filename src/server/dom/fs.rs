use std::{
    fs as stdfs,
    path::{Path, PathBuf},
};

use tokio::fs::{copy, create_dir, read, remove_dir_all, remove_file, rename, write};
use tokio::io;

use crate::util::rojo::{parse_name_and_class_name, parse_name_and_suffix, CLASS_NAME_SUFFIXES};

use super::InstanceMetadataPaths;

enum InstancePathVariant<'a> {
    Dir(&'a Path),
    File(&'a Path),
    None,
}

fn is_init_path(path: &Path) -> bool {
    if let Some((name, _)) = parse_name_and_class_name(path) {
        name == "init"
    } else {
        false
    }
}

fn get_instance_path_variant(paths: &InstanceMetadataPaths) -> InstancePathVariant<'_> {
    let dir_path_opt = paths.folder.as_deref();
    let file_path_opt = paths.file.as_deref().or(paths.file_meta.as_deref());
    if matches!(file_path_opt.map(is_init_path), Some(true)) {
        InstancePathVariant::Dir(dir_path_opt.expect("paths with init file is missing dir"))
    } else if let Some(file_path) = file_path_opt {
        InstancePathVariant::File(file_path)
    } else if let Some(dir_path) = dir_path_opt {
        InstancePathVariant::Dir(dir_path)
    } else {
        InstancePathVariant::None
    }
}

#[derive(Debug, Clone)]
pub struct PathRewrite {
    pub old_path: PathBuf,
    pub new_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RelocateInstanceResult {
    pub resolved_name: String,
    pub new_instance_paths: Vec<PathBuf>,
    pub path_rewrites: Vec<PathRewrite>,
    pub changed_new_parent_paths: Option<Vec<PathBuf>>,
}

fn collect_known_paths(paths: &InstanceMetadataPaths) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Some(path) = paths.folder.as_deref() {
        result.push(path.to_path_buf());
    }
    if let Some(path) = paths.file.as_deref() {
        result.push(path.to_path_buf());
    }
    if let Some(path) = paths.file_meta.as_deref() {
        result.push(path.to_path_buf());
    }
    result
}

fn apply_rewrites(path: &Path, rewrites: &[PathRewrite]) -> PathBuf {
    for rewrite in rewrites {
        if path == rewrite.old_path {
            return rewrite.new_path.clone();
        }
        if path.starts_with(&rewrite.old_path) {
            if let Ok(suffix) = path.strip_prefix(&rewrite.old_path) {
                return rewrite.new_path.join(suffix);
            }
        }
    }
    path.to_path_buf()
}

fn make_available_name<F>(base_name: &str, mut exists: F) -> String
where
    F: FnMut(&str) -> bool,
{
    if !exists(base_name) {
        return base_name.to_owned();
    }

    let copy_name = format!("{base_name} Copy");
    if !exists(&copy_name) {
        return copy_name;
    }

    let mut idx = 2;
    loop {
        let candidate = format!("{base_name} Copy {idx}");
        if !exists(&candidate) {
            return candidate;
        }
        idx += 1;
    }
}

async fn ensure_parent_dir_for_insert(
    parent_paths: &InstanceMetadataPaths,
) -> io::Result<(PathBuf, Option<Vec<PathBuf>>)> {
    match get_instance_path_variant(parent_paths) {
        InstancePathVariant::Dir(dir_path) => Ok((dir_path.to_path_buf(), None)),
        InstancePathVariant::File(file_path) => {
            let (new_parent_dir, new_parent_init) =
                transform_file_to_dir_with_init(file_path).await?;
            Ok((
                new_parent_dir.clone(),
                Some(vec![new_parent_dir, new_parent_init]),
            ))
        }
        InstancePathVariant::None => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "parent has no file or folder path",
        )),
    }
}

fn copy_dir_recursive_sync(source: &Path, destination: &Path) -> io::Result<()> {
    stdfs::create_dir(destination)?;
    for entry in stdfs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_recursive_sync(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            stdfs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

async fn copy_dir_recursive(source: &Path, destination: &Path) -> io::Result<()> {
    let src = source.to_path_buf();
    let dst = destination.to_path_buf();
    tokio::task::spawn_blocking(move || copy_dir_recursive_sync(&src, &dst))
        .await
        .map_err(|e| io::Error::other(format!("copy task failed: {e}")))?
}

fn resolve_file_suffix(paths: &InstanceMetadataPaths) -> io::Result<&str> {
    let path = paths
        .file
        .as_deref()
        .or(paths.file_meta.as_deref())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "instance has no file path"))?;
    parse_name_and_suffix(path)
        .map(|(_, suffix)| suffix)
        .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "failed to parse file suffix"))
}

async fn transform_file_to_dir_with_init(file_path: &Path) -> io::Result<(PathBuf, PathBuf)> {
    let parent_dir = file_path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "No parent dir"))?;

    let file_name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "No file name"))?;

    let (ext, _) = CLASS_NAME_SUFFIXES
        .iter()
        .find(|(ext, _)| file_name.ends_with(ext))
        .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "No matching extension"))?;

    let contents = read(&file_path).await?;
    remove_file(&file_path).await?;

    let new_name = file_name.trim_end_matches(ext);
    let new_dir = parent_dir.join(new_name);
    let new_init = new_dir.join(format!("init{ext}"));

    create_dir(&new_dir).await?;
    write(&new_init, contents).await?;

    Ok((new_dir, new_init))
}

async fn create_instance_in_dir(
    parent_path: &Path,
    class_name: &str,
    name: &str,
) -> io::Result<Vec<PathBuf>> {
    if class_name == "Folder" {
        let child_path = parent_path.join(name);
        create_dir(&child_path).await?;

        Ok(vec![child_path])
    } else {
        let (ext, _) = CLASS_NAME_SUFFIXES
            .iter()
            .find(|(_, ext_class)| class_name == *ext_class)
            .unwrap_or(&(".model.json", "Instance"));

        let file_name = format!("{name}{ext}");
        let child_path = parent_path.join(file_name);
        let child_contents = if *ext == ".model.json" {
            let json = serde_json::json!({
                "ClassName": class_name,
                "Properties": {},
            });
            serde_json::to_string_pretty(&json).unwrap()
        } else {
            String::new()
        };

        write(&child_path, child_contents).await?;
        Ok(vec![child_path, parent_path.to_path_buf()])
    }
}

pub async fn create_instance(
    parent_paths: &InstanceMetadataPaths,
    class_name: &str,
    name: &str,
) -> io::Result<(Vec<PathBuf>, Option<Vec<PathBuf>>)> {
    let mut changed_parent_paths = None;
    let mut new_child_paths = Vec::new();

    match get_instance_path_variant(parent_paths) {
        InstancePathVariant::Dir(dir_path) => {
            new_child_paths = create_instance_in_dir(dir_path, class_name, name).await?;
        }
        InstancePathVariant::File(file_path) => {
            let (new_parent_dir, new_parent_init) =
                transform_file_to_dir_with_init(file_path).await?;
            new_child_paths = create_instance_in_dir(&new_parent_dir, class_name, name).await?;
            changed_parent_paths = Some(vec![new_parent_dir, new_parent_init]);
        }
        InstancePathVariant::None => {}
    }

    Ok((new_child_paths, changed_parent_paths))
}

pub async fn rename_instance(
    instance_paths: &InstanceMetadataPaths,
    current_name: &str,
    name: &str,
) -> io::Result<Vec<PathBuf>> {
    let mut new_paths = Vec::new();

    match get_instance_path_variant(instance_paths) {
        InstancePathVariant::Dir(dir_path) => {
            let new_path = dir_path.with_file_name(name);
            rename(dir_path, &new_path).await?;
            new_paths.push(new_path);
        }
        InstancePathVariant::File(_) => {
            let mut paths_to_change = Vec::new();
            paths_to_change.extend(instance_paths.file.as_deref());
            paths_to_change.extend(instance_paths.file_meta.as_deref());
            for current_path in paths_to_change {
                if let Some((parsed_name, suffix)) = parse_name_and_suffix(current_path) {
                    if parsed_name == current_name {
                        let new_path = current_path.with_file_name(format!("{name}{suffix}"));
                        rename(current_path, &new_path).await?;
                        new_paths.push(new_path);
                    } else {
                        tracing::warn!(
                            "name mismatch while renaming instance from '{}' to '{}'\nat {}",
                            current_name,
                            name,
                            current_path.display()
                        )
                    }
                } else {
                    tracing::warn!(
                        "failed to parse file name and suffix while renaming instance from '{}' to '{}'\nat {}",
                        current_name,
                        name,
                        current_path.display()
                    )
                }
            }
        }
        InstancePathVariant::None => {
            tracing::warn!(
                "no path was found while renaming instance from '{}' to '{}'",
                current_name,
                name
            )
        }
    }

    Ok(new_paths)
}

pub async fn delete_instance(instance_paths: &InstanceMetadataPaths) -> io::Result<()> {
    if let Some(meta_path) = instance_paths.file_meta.as_deref() {
        remove_file(meta_path).await?;
    }
    match get_instance_path_variant(instance_paths) {
        InstancePathVariant::Dir(dir_path) => remove_dir_all(dir_path).await,
        InstancePathVariant::File(file_path) => remove_file(file_path).await,
        InstancePathVariant::None => Ok(()),
    }
}

pub async fn move_instance(
    instance_paths: &InstanceMetadataPaths,
    parent_paths: &InstanceMetadataPaths,
    current_name: &str,
) -> io::Result<RelocateInstanceResult> {
    let (new_parent_dir, changed_new_parent_paths) =
        ensure_parent_dir_for_insert(parent_paths).await?;

    let result = match get_instance_path_variant(instance_paths) {
        InstancePathVariant::Dir(old_dir) => {
            let resolved_name =
                make_available_name(current_name, |name| new_parent_dir.join(name).exists());
            let new_dir = new_parent_dir.join(&resolved_name);
            rename(old_dir, &new_dir).await?;

            let rewrites = vec![PathRewrite {
                old_path: old_dir.to_path_buf(),
                new_path: new_dir.clone(),
            }];

            let new_paths = collect_known_paths(instance_paths)
                .iter()
                .map(|path| apply_rewrites(path, &rewrites))
                .collect::<Vec<_>>();

            RelocateInstanceResult {
                resolved_name,
                new_instance_paths: new_paths,
                path_rewrites: rewrites,
                changed_new_parent_paths,
            }
        }
        InstancePathVariant::File(_main_file) => {
            let suffix = resolve_file_suffix(instance_paths)?;
            let resolved_name = make_available_name(current_name, |name| {
                new_parent_dir.join(format!("{name}{suffix}")).exists()
            });

            let mut rewrites = Vec::new();

            if let Some(path) = instance_paths.file.as_deref() {
                let target = new_parent_dir.join(format!("{resolved_name}{suffix}"));
                rename(path, &target).await?;
                rewrites.push(PathRewrite {
                    old_path: path.to_path_buf(),
                    new_path: target,
                });
            }

            if let Some(path) = instance_paths.file_meta.as_deref() {
                let target = if instance_paths.file.is_some() {
                    new_parent_dir.join(format!("{resolved_name}.meta.json"))
                } else {
                    new_parent_dir.join(format!("{resolved_name}{suffix}"))
                };
                rename(path, &target).await?;
                rewrites.push(PathRewrite {
                    old_path: path.to_path_buf(),
                    new_path: target,
                });
            }

            let mut new_paths = collect_known_paths(instance_paths)
                .iter()
                .map(|path| apply_rewrites(path, &rewrites))
                .collect::<Vec<_>>();
            new_paths.push(new_parent_dir.clone());

            RelocateInstanceResult {
                resolved_name,
                new_instance_paths: new_paths,
                path_rewrites: rewrites,
                changed_new_parent_paths,
            }
        }
        InstancePathVariant::None => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "instance has no file or folder path",
            ))
        }
    };

    Ok(result)
}

pub async fn copy_instance(
    instance_paths: &InstanceMetadataPaths,
    parent_paths: &InstanceMetadataPaths,
    current_name: &str,
) -> io::Result<RelocateInstanceResult> {
    let (new_parent_dir, changed_new_parent_paths) =
        ensure_parent_dir_for_insert(parent_paths).await?;

    let result = match get_instance_path_variant(instance_paths) {
        InstancePathVariant::Dir(old_dir) => {
            let resolved_name =
                make_available_name(current_name, |name| new_parent_dir.join(name).exists());
            let new_dir = new_parent_dir.join(&resolved_name);
            copy_dir_recursive(old_dir, &new_dir).await?;

            let rewrites = vec![PathRewrite {
                old_path: old_dir.to_path_buf(),
                new_path: new_dir.clone(),
            }];

            let new_paths = collect_known_paths(instance_paths)
                .iter()
                .map(|path| apply_rewrites(path, &rewrites))
                .collect::<Vec<_>>();

            RelocateInstanceResult {
                resolved_name,
                new_instance_paths: new_paths,
                path_rewrites: rewrites,
                changed_new_parent_paths,
            }
        }
        InstancePathVariant::File(_) => {
            let suffix = resolve_file_suffix(instance_paths)?;
            let resolved_name = make_available_name(current_name, |name| {
                new_parent_dir.join(format!("{name}{suffix}")).exists()
            });

            let mut rewrites = Vec::new();

            if let Some(path) = instance_paths.file.as_deref() {
                let target = new_parent_dir.join(format!("{resolved_name}{suffix}"));
                copy(path, &target).await?;
                rewrites.push(PathRewrite {
                    old_path: path.to_path_buf(),
                    new_path: target,
                });
            }

            if let Some(path) = instance_paths.file_meta.as_deref() {
                let target = if instance_paths.file.is_some() {
                    new_parent_dir.join(format!("{resolved_name}.meta.json"))
                } else {
                    new_parent_dir.join(format!("{resolved_name}{suffix}"))
                };
                copy(path, &target).await?;
                rewrites.push(PathRewrite {
                    old_path: path.to_path_buf(),
                    new_path: target,
                });
            }

            let mut new_paths = collect_known_paths(instance_paths)
                .iter()
                .map(|path| apply_rewrites(path, &rewrites))
                .collect::<Vec<_>>();
            new_paths.push(new_parent_dir.clone());

            RelocateInstanceResult {
                resolved_name,
                new_instance_paths: new_paths,
                path_rewrites: rewrites,
                changed_new_parent_paths,
            }
        }
        InstancePathVariant::None => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "instance has no file or folder path",
            ))
        }
    };

    Ok(result)
}
