import * as fs from "fs";
import * as path from "path";

export type VscodePlatform =
  | "linux-x64"
  | "linux-arm64"
  | "darwin-x64"
  | "darwin-arm64"
  | "win32-x64";

export function detectPlatform(): VscodePlatform | null {
  const p = process.platform;
  const a = process.arch;
  if (p === "linux" && a === "x64") return "linux-x64";
  if (p === "linux" && a === "arm64") return "linux-arm64";
  if (p === "darwin" && a === "x64") return "darwin-x64";
  if (p === "darwin" && a === "arm64") return "darwin-arm64";
  if (p === "win32" && a === "x64") return "win32-x64";
  return null;
}

/** Returns the path to the bundled binary inside the installed extension, or null. */
export function getBundledBinaryPath(extensionPath: string): string | null {
  const binaryName =
    process.platform === "win32" ? "covenant-lsp.exe" : "covenant-lsp";
  const candidate = path.join(extensionPath, "bin", binaryName);
  return fs.existsSync(candidate) ? candidate : null;
}
