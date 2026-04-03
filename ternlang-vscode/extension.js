// ternlang VS Code extension — LSP client
// Connects to the ternlang-lsp language server for .tern files.
//
// Setup:
//   1. Build ternlang-lsp: cd ternlang-root && cargo build --release
//   2. Install extension (vsce package, or copy to ~/.vscode/extensions)
//   3. The extension auto-starts ternlang-lsp when you open a .tern file.

const vscode = require('vscode');
const path   = require('path');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;

function activate(context) {
    // Path to the compiled ternlang-lsp binary.
    // Adjust if your ternlang-root is in a different location.
    const lspBin = path.join(
        context.extensionPath,
        '..', 'ternlang-root', 'target', 'release', 'ternlang-lsp'
    );

    const serverOptions = {
        command: lspBin,
        transport: TransportKind.stdio
    };

    const clientOptions = {
        documentSelector: [{ scheme: 'file', language: 'ternlang' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.tern')
        }
    };

    client = new LanguageClient(
        'ternlang-lsp',
        'Ternlang Language Server',
        serverOptions,
        clientOptions
    );

    client.start();
    console.log('ternlang-lsp started');
}

function deactivate() {
    if (client) return client.stop();
}

module.exports = { activate, deactivate };
