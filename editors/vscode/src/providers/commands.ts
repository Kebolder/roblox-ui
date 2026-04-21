import * as vscode from "vscode"

import { reconnectAllWorkspaces } from "../workspaces"
import { ExplorerItem } from "./explorer"
import { Providers } from "."

const EXTENSION_NAME = "roblox-ui"

type ExplorerClipboard = {
	workspacePath: string
	domId: string
	mode: "cut" | "copy"
}

export class CommandsProvider implements vscode.Disposable {
	// biome-ignore lint/suspicious/noExplicitAny:
	private readonly commands: Map<string, (...args: any[]) => any> = new Map()
	private readonly disposables: Array<vscode.Disposable> = new Array()
	private explorerClipboard: ExplorerClipboard | null = null

	constructor(public readonly providers: Providers) {
		this.register("explorer.refresh", reconnectAllWorkspaces)
		this.register("explorer.quickOpen", () => providers.quickOpen.show())

		this.register("explorer.select", async (workspacePath: string, domId: string) => {
			const item = providers.explorerTree.findById(workspacePath, domId)
			if (item) {
				await providers.explorerView.reveal(item, {
					expand: false,
					select: true,
					focus: false,
				})
			}
		})
		this.register(
			"explorer.expand",
			async (workspacePath: string, domId: string, levels?: number | null) => {
				const item = providers.explorerTree.findById(workspacePath, domId)
				if (item) {
					await providers.explorerView.reveal(item, {
						expand: levels ?? true,
						select: false,
						focus: false,
					})
				}
			}
		)

		const revealFileInOS = (item: ExplorerItem) => {
			const uri = item.resourceUri
			if (uri) {
				vscode.commands.executeCommand("revealFileInOS", uri)
			}
		}
		this.register("explorer.revealFileInOS.windows", revealFileInOS)
		this.register("explorer.revealFileInOS.mac", revealFileInOS)
		this.register("explorer.revealFileInOS", revealFileInOS)

		this.register("explorer.openRojoManifest", (item: ExplorerItem) => {
			const filePath = item.domInstance.metadata?.paths?.rojo
			if (filePath) {
				vscode.commands.executeCommand("vscode.open", vscode.Uri.file(filePath))
			}
		})
		this.register("explorer.openWallyManifest", (item: ExplorerItem) => {
			const filePath = item.domInstance.metadata?.paths?.wally
			if (filePath) {
				vscode.commands.executeCommand("vscode.open", vscode.Uri.file(filePath))
			}
		})

		this.register("explorer.insertObject", (item: ExplorerItem) => {
			providers.insertInstance.show(item.workspacePath, item.domInstance, null, false)
		})
		this.register("explorer.insertFolder", (item: ExplorerItem) => {
			providers.insertInstance.show(item.workspacePath, item.domInstance, "Folder", false)
		})
		this.register("explorer.insertService", (item: ExplorerItem) => {
			providers.insertInstance.show(item.workspacePath, item.domInstance, null, true)
		})
		this.register("explorer.instanceCut", (item?: ExplorerItem) => {
			if (!item) {
				return
			}
			this.explorerClipboard = {
				workspacePath: item.workspacePath,
				domId: item.domInstance.id,
				mode: "cut",
			}
			vscode.window.setStatusBarMessage(`Cut "${item.domInstance.name}"`, 1500)
		})
		this.register("explorer.instanceCopy", (item?: ExplorerItem) => {
			if (!item) {
				return
			}
			this.explorerClipboard = {
				workspacePath: item.workspacePath,
				domId: item.domInstance.id,
				mode: "copy",
			}
			vscode.window.setStatusBarMessage(`Copied "${item.domInstance.name}"`, 1500)
		})
		this.register("explorer.instancePaste", async (item?: ExplorerItem) => {
			await this.pasteClipboard(item, false)
		})
		this.register("explorer.instancePasteInto", async (item?: ExplorerItem) => {
			await this.pasteClipboard(item, true)
		})

		this.register("explorer.renameObject", async (item: ExplorerItem) => {
			providers.renameInstance.show(item.workspacePath, item.domInstance)
		})
		this.register("explorer.deleteObject", async (item: ExplorerItem) => {
			await providers.explorerTree.deleteInstance(item.workspacePath, item.domInstance.id)
		})
	}

	dispose() {
		for (const disposable of this.disposables) {
			disposable.dispose()
		}
		this.commands.clear()
	}

	// biome-ignore lint/suspicious/noExplicitAny:
	private register(name: string, command: (...args: any[]) => any) {
		const fullName = `${EXTENSION_NAME}.${name}`
		const disposable = vscode.commands.registerCommand(fullName, command)
		this.disposables.push(disposable)
	}

	// biome-ignore lint/suspicious/noExplicitAny:
	public async run(name: string, ...args: any) {
		const fullName = `${EXTENSION_NAME}.${name}`
		await vscode.commands.executeCommand(fullName, ...args)
	}

	private async pasteClipboard(target?: ExplorerItem, into?: boolean) {
		if (!target) {
			return
		}

		const clipboard = this.explorerClipboard
		if (!clipboard) {
			vscode.window.showWarningMessage("No instance in clipboard. Use Cut or Copy first.")
			return
		}

		if (clipboard.workspacePath !== target.workspacePath) {
			vscode.window.showWarningMessage("Cannot paste across different workspaces.")
			return
		}

		if (clipboard.mode === "copy") {
			vscode.window.showWarningMessage(
				"Copy/Paste is not supported yet. Use Cut/Paste to move instances."
			)
			return
		}

		const newParentId = into === true ? target.domInstance.id : target.parent?.domInstance.id
		if (!newParentId) {
			vscode.window.showWarningMessage("Cannot paste here.")
			return
		}

		if (clipboard.domId === newParentId) {
			vscode.window.showWarningMessage("Cannot move an instance into itself.")
			return
		}

		const moved = await this.providers.explorerTree.moveInstance(
			clipboard.workspacePath,
			clipboard.domId,
			newParentId
		)

		if (!moved) {
			vscode.window.showErrorMessage("Cut/Paste move is not available yet for this project.")
			return
		}

		this.explorerClipboard = null
	}
}
