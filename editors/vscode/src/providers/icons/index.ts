import * as vscode from "vscode"
import * as path from "path"
import * as fs from "fs"

import { Providers } from ".."

import type { IconPack, IconPackData, IconPackIcon, IconPackMetadatas } from "./types"
import { runCommand } from "../../server/child"
export type { IconPack } from "./types"

const CUSTOM_ICON_PACK: IconPack = "RobloxCustom"

const getAllIconPacks = (): Array<IconPack> => {
	return ["None", "Vanilla3"]
}

const getOutBasePath = (extensionPath: string, pack: IconPack): string => {
	return path.join(extensionPath, "out", "icons", pack)
}

const getLocalRepoBasePath = (extensionPath: string, pack: IconPack): string => {
	return path.resolve(extensionPath, "..", "..", "icons", pack)
}

const getEmptyMetadatas = (): IconPackMetadatas => {
	return {
		light: {
			classCount: 0,
			classIcons: {},
		},
		dark: {
			classCount: 0,
			classIcons: {},
		},
	}
}

const tryResolveBasePath = (extensionPath: string, pack: IconPack): string | null => {
	const candidates = [getOutBasePath(extensionPath, pack), getLocalRepoBasePath(extensionPath, pack)]
	for (const basePath of candidates) {
		const metaPathLight = path.join(basePath, "light", "metadata.json")
		const metaPathDark = path.join(basePath, "dark", "metadata.json")
		if (fs.existsSync(metaPathLight) && fs.existsSync(metaPathDark)) {
			return basePath
		}
	}
	return null
}

const readIconPackMetadatas = (basePath: string | null, pack: IconPack): IconPackMetadatas => {
	if (pack === "None") {
		return getEmptyMetadatas()
	}

	if (!basePath) {
		return getEmptyMetadatas()
	}

	const metaPathLight = path.join(basePath, "light", "metadata.json")
	const metaPathDark = path.join(basePath, "dark", "metadata.json")

	try {
		const metaContentsLight = fs.readFileSync(metaPathLight, "utf-8")
		const metaContentsDark = fs.readFileSync(metaPathDark, "utf-8")

		return {
			light: JSON.parse(metaContentsLight),
			dark: JSON.parse(metaContentsDark),
		}
	} catch {
		return getEmptyMetadatas()
	}
}

const createIconPackData = (basePath: string | null, metas: IconPackMetadatas): IconPackData => {
	const icons = new Map<string, IconPackIcon>()
	if (!basePath) {
		return icons
	}

	const allClassNames = new Set<string>()
	for (const className of Object.keys(metas.light.classIcons)) {
		allClassNames.add(className)
	}
	for (const className of Object.keys(metas.dark.classIcons)) {
		allClassNames.add(className)
	}

	for (const className of allClassNames) {
		const relLight = metas.light.classIcons[className]
		const relDark = metas.dark.classIcons[className]
		if (!relLight || !relDark) {
			continue
		}
		const iconPathLight = path.join(basePath, "light", relLight)
		const iconPathDark = path.join(basePath, "dark", relDark)
		icons.set(className, {
			light: vscode.Uri.file(iconPathLight),
			dark: vscode.Uri.file(iconPathDark),
		})
	}

	return icons
}

export class IconsProvider implements vscode.Disposable {
	private readonly metas: Map<IconPack, IconPackMetadatas> = new Map()
	private readonly icons: Map<IconPack, IconPackData> = new Map()

	private customIconsLoading = false
	private customIconsErrored = false

	private readonly _onDidChangeCustomIcons: vscode.EventEmitter<void> = new vscode.EventEmitter()
	public readonly onDidChangeCustomIcons: vscode.Event<void> = this._onDidChangeCustomIcons.event

	private readonly disposables: vscode.Disposable[] = []

	constructor(public readonly providers: Providers) {
		for (const pack of getAllIconPacks()) {
			const basePath = tryResolveBasePath(providers.extensionContext.extensionPath, pack)
			const metas = readIconPackMetadatas(basePath, pack)
			const icons = createIconPackData(basePath, metas)
			this.metas.set(pack, metas)
			this.icons.set(pack, icons)
		}
		const updateCustomIconDir = () => {
			const customIconDir = providers.settings.get("explorer.customIconDir")
			if (customIconDir && customIconDir.trim().length > 0) {
				this.customIconsLoading = true
				this.customIconsErrored = false

				const outputPath = getOutBasePath(
					providers.extensionContext.extensionPath,
					CUSTOM_ICON_PACK
				)
				const commandArgs = [
					"generate-icons",
					"--input",
					customIconDir,
					"--output",
					outputPath,
				]

				runCommand(providers, commandArgs)
					.then(() => {
						const metas = readIconPackMetadatas(
							tryResolveBasePath(
								providers.extensionContext.extensionPath,
								CUSTOM_ICON_PACK
							),
							CUSTOM_ICON_PACK
						)
						const icons = createIconPackData(
							tryResolveBasePath(
								providers.extensionContext.extensionPath,
								CUSTOM_ICON_PACK
							),
							metas
						)
						this.metas.set(CUSTOM_ICON_PACK, metas)
						this.icons.set(CUSTOM_ICON_PACK, icons)

						this.customIconsLoading = false
						this.customIconsErrored = false

						this._onDidChangeCustomIcons.fire()
					})
					.catch((e) => {
						this.customIconsLoading = false
						this.customIconsErrored = true
						vscode.window.showErrorMessage(`Failed to read custom icon pack: ${e}`)
					})
			} else {
				this.metas.delete(CUSTOM_ICON_PACK)
				this.icons.delete(CUSTOM_ICON_PACK)
			}
		}
		this.disposables.push(
			providers.settings.listen("explorer.customIconDir", updateCustomIconDir)
		)
		updateCustomIconDir()
	}

	public getClassIcon(className: string): IconPackIcon | undefined {
		const shouldUseNormalIcons =
			this.customIconsLoading ||
			this.customIconsErrored ||
			!this.providers.settings.get("explorer.customIconDir")
		const configuredPack = this.providers.settings.get("explorer.iconPack")
		const normalPack = this.icons.has(configuredPack) ? configuredPack : "Vanilla3"
		const pack = shouldUseNormalIcons ? normalPack : CUSTOM_ICON_PACK
		const icon = this.icons.get(pack)?.get(className) ?? undefined
		return icon
	}

	dispose() {
		for (const disposable of this.disposables) {
			disposable.dispose()
		}
		this._onDidChangeCustomIcons.dispose()
	}
}
