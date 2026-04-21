#!/usr/bin/env node

import fs from "node:fs/promises";
import path from "node:path";

const ICON_EXTENSIONS = new Set([".png", ".svg"]);
const THEME_NAMES = ["light", "dark"];

async function pathExists(targetPath) {
	try {
		await fs.access(targetPath);
		return true;
	} catch {
		return false;
	}
}

async function collectIconFiles(dirPath) {
	const stack = [dirPath];
	const files = [];

	while (stack.length > 0) {
		const currentDir = stack.pop();
		const entries = await fs.readdir(currentDir, { withFileTypes: true });
		for (const entry of entries) {
			const absPath = path.join(currentDir, entry.name);
			if (entry.isDirectory()) {
				stack.push(absPath);
			} else if (entry.isFile()) {
				const ext = path.extname(entry.name).toLowerCase();
				if (ICON_EXTENSIONS.has(ext)) {
					files.push(absPath);
				}
			}
		}
	}

	return files;
}

function toPosixRelative(baseDir, absFilePath) {
	const rel = path.relative(baseDir, absFilePath);
	return rel.split(path.sep).join("/");
}

function pickPreferredFile(currentRelPath, nextRelPath) {
	const currentExt = path.extname(currentRelPath).toLowerCase();
	const nextExt = path.extname(nextRelPath).toLowerCase();

	// Prefer SVG when both exist for the same class name.
	if (currentExt === ".png" && nextExt === ".svg") {
		return nextRelPath;
	}

	return currentRelPath;
}

async function buildThemeMetadata(themeDir) {
	const iconFilesAbs = await collectIconFiles(themeDir);
	const classIconsMap = new Map();

	for (const absFilePath of iconFilesAbs) {
		const relPath = toPosixRelative(themeDir, absFilePath);
		const className = path.basename(relPath, path.extname(relPath));
		if (className.length === 0) {
			continue;
		}

		const current = classIconsMap.get(className);
		if (!current) {
			classIconsMap.set(className, relPath);
		} else {
			classIconsMap.set(className, pickPreferredFile(current, relPath));
		}
	}

	const sortedClassNames = [...classIconsMap.keys()].sort((a, b) =>
		a.localeCompare(b)
	);
	const classIcons = {};
	for (const className of sortedClassNames) {
		classIcons[className] = classIconsMap.get(className);
	}

	return {
		classCount: sortedClassNames.length,
		classIcons,
	};
}

async function buildPackMetadata(packDir) {
	for (const themeName of THEME_NAMES) {
		const themeDir = path.join(packDir, themeName);
		if (!(await pathExists(themeDir))) {
			continue;
		}

		const metadata = await buildThemeMetadata(themeDir);
		const metadataPath = path.join(themeDir, "metadata.json");
		await fs.writeFile(metadataPath, JSON.stringify(metadata), "utf8");
		console.log(`wrote ${metadataPath} (${metadata.classCount} classes)`);
	}
}

async function main() {
	const iconsRoot = path.resolve(process.argv[2] ?? "icons");
	if (!(await pathExists(iconsRoot))) {
		throw new Error(`icons directory does not exist: ${iconsRoot}`);
	}

	const packs = await fs.readdir(iconsRoot, { withFileTypes: true });
	for (const pack of packs) {
		if (!pack.isDirectory()) {
			continue;
		}
		await buildPackMetadata(path.join(iconsRoot, pack.name));
	}
}

main().catch((error) => {
	console.error(error instanceof Error ? error.message : String(error));
	process.exit(1);
});
