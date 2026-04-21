use std::{
    io::{Cursor, Read},
};

use anyhow::{Context, Result};
use bytes::Bytes;

/**
    Extracts a single file from the given zip file.

    The given file path must be a full path to the file, including directories.
*/
pub fn extract_file_from_zip<Z, S>(zip_bytes: Z, file_path: S) -> Result<Bytes>
where
    Z: AsRef<[u8]>,
    S: AsRef<str>,
{
    let zip_bytes = zip_bytes.as_ref();
    let file_name = file_path.as_ref();

    let mut reader = Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(&mut reader).context("failed to read zip archive")?;

    let file_idx = (0..archive.len())
        .find(|i| {
            if let Ok(file) = archive.by_index(*i) {
                let file_name_str = file.enclosed_name().and_then(|p| {
                    p.file_name()
                        .and_then(|f| f.to_str())
                        .map(|s| s.to_string())
                });
                file.is_file() && matches!(file_name_str, Some(n) if n == file_name)
            } else {
                false
            }
        })
        .with_context(|| format!("failed to find file '{file_name}' in zip"))?;

    let mut file = archive.by_index(file_idx).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    Ok(Bytes::from(buffer))
}

