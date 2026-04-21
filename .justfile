EXT := if os() == "windows" { ".exe" } else { "" }
CWD := invocation_directory()
VSCODE := invocation_directory() / "editors" / "vscode"
BIN_NAME := "roblox-ui"

# Default hidden recipe for listing other recipes + cwd
[no-cd]
[no-exit-message]
[private]
default:
	#!/usr/bin/env bash
	set -euo pipefail
	printf "Current directory:\n    {{CWD}}\n"
	just --list

# Builds the executable
[no-exit-message]
build *ARGS:
	#!/usr/bin/env bash
	set -euo pipefail
	cargo build --bin {{BIN_NAME}} {{ARGS}}

# Legacy command retained for compatibility.
# Downloaded icon packs are removed; use local Vanilla3 icons instead.
[no-exit-message]
generate-icons DEBUG="false":
	#!/usr/bin/env bash
	set -euo pipefail
	echo "generate-icons is no longer supported for built-in pack downloads."
	echo "Use local icons at '{{CWD}}/icons/Vanilla3/{light,dark}' and run:"
	echo "  node {{CWD}}/tools/build-icon-metadata.mjs {{CWD}}/icons"
	exit 1

# Builds metadata.json files from local icon files under each pack's light/dark directories.
[no-exit-message]
build-icon-metadata ICONS_DIR="{{CWD}}/icons":
	#!/usr/bin/env bash
	set -euo pipefail
	node {{CWD}}/tools/build-icon-metadata.mjs "{{ICONS_DIR}}"

# Ensures icon packs are present in ./icons.
# ICONS_MODE="local" validates local files only.
# ICONS_MODE="download" is intentionally unsupported in Vanilla3-only mode.
[no-exit-message]
ensure-icons ICONS_MODE="local" DEBUG="false":
	#!/usr/bin/env bash
	set -euo pipefail
	if [[ "{{ICONS_MODE}}" == "download" ]]; then
		echo "ICONS_MODE=download is not supported with Vanilla3-only icon packs."
		echo "Provide local icons under '{{CWD}}/icons/Vanilla3/{light,dark}'."
		exit 1
	fi
	if [ ! -d "{{CWD}}/icons/Vanilla3/light" ] || [ ! -d "{{CWD}}/icons/Vanilla3/dark" ]; then
		echo "Missing local icon directories under '{{CWD}}/icons'."
		echo "Run: just ensure-icons download {{DEBUG}}"
		exit 1
	fi
	if [ ! -f "{{CWD}}/icons/Vanilla3/light/metadata.json" ] || [ ! -f "{{CWD}}/icons/Vanilla3/dark/metadata.json" ]; then
		echo "Missing icon metadata.json files under '{{CWD}}/icons'."
		echo "Run: just ensure-icons download {{DEBUG}}"
		exit 1
	fi
	echo "Using local icons from '{{CWD}}/icons'."

# Generates reflection and class metadata files in a "data" directory in cwd
[no-exit-message]
generate-metadata DEBUG="false":
	#!/usr/bin/env bash
	set -euo pipefail
	mkdir -p {{CWD}}/data/
	if [[ "{{DEBUG}}" == "true" ]]; then
		cargo run -- generate-classes --output {{CWD}}/data/classes.json
		cargo run -- generate-reflection --output {{CWD}}/data/reflection.json
	else
		cargo run --release -- generate-classes --output {{CWD}}/data/classes.json
		cargo run --release -- generate-reflection --output {{CWD}}/data/reflection.json
	fi

# Packs the executable into the VSCode extension build directory
[no-exit-message]
[private]
vscode-pack TARGET_DIR DEBUG="false":
	#!/usr/bin/env bash
	set -euo pipefail
	#
	rm -rf "{{VSCODE}}/out"
	rm -rf "{{VSCODE}}/bin"
	rm -rf "{{VSCODE}}/CHANGELOG.md"
	rm -rf "{{VSCODE}}/LICENSE.txt"
	mkdir -p "{{VSCODE}}/bin"
	mkdir -p "{{VSCODE}}/out"
	mkdir -p "{{VSCODE}}/out/icons"
	mkdir -p "{{VSCODE}}/out/data"
	#
	if [[ "{{DEBUG}}" == "true" ]]; then
		mkdir -p {{VSCODE}}/out/debug/
		cp {{TARGET_DIR}}/debug/{{BIN_NAME}}{{EXT}} {{VSCODE}}/out/debug/
	else
		mkdir -p {{VSCODE}}/out/release/
		cp {{TARGET_DIR}}/release/{{BIN_NAME}}{{EXT}} {{VSCODE}}/out/release/
	fi
	#
	if [ "{{os_family()}}" = "windows" ]; then
		cp -R {{CWD}}/icons/ {{VSCODE}}/out/
		cp -R {{CWD}}/data/ {{VSCODE}}/out/
	else
		cp -R {{CWD}}/icons/ {{VSCODE}}/out/icons/
		cp -R {{CWD}}/data/ {{VSCODE}}/out/data/
	fi
	#
	cp CHANGELOG.md {{VSCODE}}/CHANGELOG.md
	cp LICENSE.txt {{VSCODE}}/LICENSE.txt

# Builds the VSCode extension - must be used after vscode-pack
[no-exit-message]
[private]
vscode-build:
	#!/usr/bin/env bash
	set -euo pipefail
	cd "{{VSCODE}}/"
	npm install
	npx @vscode/vsce package --out "{{VSCODE}}/bin/"

# Builds the VSCode extension locally without auto-installing it.
[no-exit-message]
vscode-install DEBUG="false" ICONS_MODE="local":
	#!/usr/bin/env bash
	set -euo pipefail
	#
	echo "[1/4] Building executable..."
	if [[ "{{DEBUG}}" == "true" ]]; then
		just build
	else
		just build --release
	fi
	echo "[2/4] Generating files..."
	just ensure-icons {{ICONS_MODE}} {{DEBUG}} > /dev/null
	just generate-metadata {{DEBUG}} > /dev/null
	echo "[3/4] Packing executable..."
	just vscode-pack "target" {{DEBUG}} > /dev/null
	echo "[4/4] Building extension..."
	just vscode-build > /dev/null
	#
	EXTENSION=$(find "{{VSCODE}}/bin/" -name "*.vsix" | head -n 1)
	if [[ -z "$EXTENSION" ]]; then
		echo "No .vsix file found in '{{VSCODE}}/bin/'."
		exit 1
	fi
	echo "Built VSIX at: $EXTENSION"
	echo "Install manually when needed:"
	echo "  code --install-extension \"$EXTENSION\" --force"

# Builds and publishes the VSCode extension to the marketplace
[no-exit-message]
vscode-publish TARGET_TRIPLE VSCODE_TARGET ICONS_MODE="local":
	#!/usr/bin/env bash
	set -euo pipefail
	#
	echo "🚧 [1/5] Building executable..."
	just build --release --target {{TARGET_TRIPLE}}
	echo "🤖 [2/5] Generating files..."
	just ensure-icons {{ICONS_MODE}}
	just generate-metadata
	echo "📦 [3/5] Packing executable..."
	just vscode-pack "target/{{TARGET_TRIPLE}}"
	echo "🧰 [4/5] Building extension..."
	just vscode-build
	echo "🚀 [5/5] Publishing extension..."
	#
	cd "{{VSCODE}}/"
	npx @vscode/vsce publish --target {{VSCODE_TARGET}}
	#
	echo "✅ Published extension successfully!"

# Zips up executable and built extensions into single zip file
[no-exit-message]
zip-release TARGET_TRIPLE:
	#!/usr/bin/env bash
	set -euo pipefail
	#
	rm -rf staging
	rm -rf release.zip
	mkdir -p staging
	#
	cp "target/{{TARGET_TRIPLE}}/release/{{BIN_NAME}}{{EXT}}" staging/
	cp "$(find "{{VSCODE}}/bin/" -name "*.vsix")" staging/extension.vsix
	cp -R "icons" staging/
	cp -R "data" staging/
	#
	cd staging
	if [ "{{os_family()}}" = "windows" ]; then
		7z a ../release.zip *
	else
		chmod +x {{BIN_NAME}}
		zip ../release.zip *
	fi
	cd "{{CWD}}"
	#
	rm -rf staging

# Used in GitHub workflow to move per-matrix release zips
[no-exit-message]
[private]
unpack-releases RELEASES_DIR:
	#!/usr/bin/env bash
	set -euo pipefail
	#
	if [ ! -d "{{RELEASES_DIR}}" ]; then
		echo "Releases directory is missing"
		exit 1
	fi
	#
	cd "{{RELEASES_DIR}}"
	echo ""
	echo "Releases dir:"
	ls -lhrt
	echo ""
	echo "Searching for zipped releases..."
	#
	for DIR in * ; do
		if [ -d "$DIR" ]; then
			cd "$DIR"
			for FILE in * ; do
				if [ ! -d "$FILE" ]; then
					if [ "$FILE" = "release.zip" ]; then
						echo "Found zipped release '$DIR'"
						mv "$FILE" "../$DIR.zip"
						rm -rf "../$DIR/"
					fi
				fi
			done
			cd ..
		fi
	done
	#
	echo ""
	echo "Releases dir:"
	ls -lhrt

