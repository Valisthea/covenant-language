import * as fs from "fs";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";
import { getBundledBinaryPath, detectPlatform } from "./platform";

let client: LanguageClient | undefined;

function resolveFromPath(name: string): string | undefined {
  const { execSync } = require("child_process");
  try {
    const cmd =
      process.platform === "win32" ? `where.exe ${name}` : `which ${name}`;
    const result = execSync(cmd, { encoding: "utf8" }).trim().split("\n")[0];
    if (result && fs.existsSync(result.trim())) {
      return result.trim();
    }
  } catch {
    // not found in PATH
  }
  return undefined;
}

export async function activate(
  context: vscode.ExtensionContext
): Promise<void> {
  const config = vscode.workspace.getConfiguration("covenant");

  // ── Commands ────────────────────────────────────────────────────────────
  context.subscriptions.push(
    vscode.commands.registerCommand("covenant.runGasBenchmark", () => {
      const terminal = vscode.window.createTerminal({
        name: "Covenant Bench",
        env: { FORCE_COLOR: "1" },
      });
      terminal.show();
      terminal.sendText("covenant bench --print");
    })
  );

  if (!config.get<boolean>("lsp.enabled", true)) {
    return;
  }

  // Resolution order:
  //   1. Bundled binary (bin/covenant-lsp[.exe] inside the extension install dir)
  //   2. Explicit user config (covenant.lsp.path, if set to something non-default)
  //   3. System PATH lookup for the configured/default name

  let serverPath: string | undefined;
  let source = "";

  // 1 — Bundled binary (zero-config for Marketplace installs)
  const bundled = getBundledBinaryPath(context.extensionPath);
  if (bundled) {
    serverPath = bundled;
    source = "bundled";
  }

  // 2/3 — User config or PATH fallback (developer / manual install)
  if (!serverPath) {
    const configuredPath = config.get<string>("lsp.path", "covenant-lsp");
    // If the user pointed at an absolute path that exists, trust it directly.
    if (configuredPath !== "covenant-lsp" && fs.existsSync(configuredPath)) {
      serverPath = configuredPath;
      source = "user-config";
    } else {
      // Try PATH resolution (works for both default "covenant-lsp" and a bare name).
      const fromPath = resolveFromPath(configuredPath);
      if (fromPath) {
        serverPath = fromPath;
        source = "system-path";
      }
    }
  }

  if (!serverPath) {
    const platform = detectPlatform() ?? "unknown";
    const install = "Install covenant-lsp";
    const ignore = "Ignore";
    const choice = await vscode.window.showWarningMessage(
      `Covenant: language server not found for platform ${platform}. ` +
        `Diagnostics and hover will be unavailable.`,
      install,
      ignore
    );
    if (choice === install) {
      vscode.env.openExternal(
        vscode.Uri.parse("https://github.com/Valisthea/covenant-language#installation")
      );
    }
    return;
  }

  console.log(`Covenant: using LSP from ${source}: ${serverPath}`);

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: [],
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "covenant" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.cov"),
    },
    outputChannelName: "Covenant Language Server",
  };

  client = new LanguageClient(
    "covenant-lsp",
    "Covenant Language Server",
    serverOptions,
    clientOptions
  );

  try {
    await client.start();
    context.subscriptions.push({
      dispose: () => client?.stop(),
    });
  } catch (err) {
    vscode.window.showErrorMessage(
      `Covenant: failed to start language server: ${err}`
    );
  }
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
  }
}
