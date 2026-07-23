#!/usr/bin/env node
/**
 * package-vsix.js — Download covenant-lsp binaries from GitHub Releases and
 * package one platform-specific .vsix per target.
 *
 * Usage:
 *   node scripts/package-vsix.js                     # all 5 platforms
 *   node scripts/package-vsix.js --platform=win32-x64
 *   node scripts/package-vsix.js --version=0.7.1     # override version
 *
 * Requires: curl (all platforms), @vscode/vsce (npm install -g @vscode/vsce)
 */

"use strict";

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

const PLATFORMS = [
  "linux-x64",
  "linux-arm64",
  "darwin-x64",
  "darwin-arm64",
  "win32-x64",
];

const REPO = "Valisthea/covenant-language";

// ── CLI args ──────────────────────────────────────────────────────────────

function parseArgs() {
  const args = process.argv.slice(2);
  const platforms = [];
  let version = null;

  for (const arg of args) {
    if (arg.startsWith("--platform=")) {
      platforms.push(arg.slice("--platform=".length));
    } else if (arg.startsWith("--version=")) {
      version = arg.slice("--version=".length);
    }
  }

  return { platforms: platforms.length ? platforms : PLATFORMS, version };
}

// ── Helpers ───────────────────────────────────────────────────────────────

function run(cmd, opts = {}) {
  console.log(`  $ ${cmd}`);
  execSync(cmd, { stdio: "inherit", ...opts });
}

function downloadBinary(platform, version) {
  const ext = platform === "win32-x64" ? ".exe" : "";
  const assetName = `covenant-lsp-${platform}${ext}`;
  const binDir = path.join(__dirname, "..", "bin");
  const dest = path.join(binDir, `covenant-lsp${ext}`);

  if (!fs.existsSync(binDir)) fs.mkdirSync(binDir, { recursive: true });

  // Remove any existing binary from a previous platform
  for (const f of fs.readdirSync(binDir)) {
    fs.rmSync(path.join(binDir, f));
  }

  // Use gh CLI — handles auth for both public and private repos
  console.log(`  Downloading ${assetName} from v${version}`);
  run(
    `gh release download v${version} --repo ${REPO} --pattern "${assetName}" --dir "${binDir}" --clobber`
  );

  // Rename asset to the expected bin/ name
  const downloaded = path.join(binDir, assetName);
  if (downloaded !== dest && fs.existsSync(downloaded)) {
    fs.renameSync(downloaded, dest);
  }

  if (platform !== "win32-x64") {
    fs.chmodSync(dest, 0o755);
  }

  console.log(`  Saved → ${path.relative(process.cwd(), dest)}`);
  return dest;
}

// ── Main ──────────────────────────────────────────────────────────────────

function main() {
  const { platforms, version: versionOverride } = parseArgs();

  const pkg = JSON.parse(
    fs.readFileSync(path.join(__dirname, "..", "package.json"), "utf8")
  );
  const version = versionOverride ?? pkg.version;

  console.log(`\nCovenant VS Code Extension — packaging v${version}`);
  console.log(`Platforms: ${platforms.join(", ")}\n`);

  const ext = path.resolve(__dirname, "..");
  const outputs = [];

  for (const platform of platforms) {
    console.log(`\n══ ${platform} ══`);

    downloadBinary(platform, version);

    const outFile = path.join(ext, `covenant-lang-${platform}-${version}.vsix`);
    run(`npx @vscode/vsce package --target ${platform} --out "${outFile}"`, {
      cwd: ext,
    });

    outputs.push(outFile);
    console.log(`  ✓ ${path.basename(outFile)}`);
  }

  // Clean up bin/ so it doesn't linger after packaging
  const binDir = path.join(__dirname, "..", "bin");
  if (fs.existsSync(binDir)) fs.rmSync(binDir, { recursive: true });

  console.log("\n══ Done ══\n");
  outputs.forEach((f) => console.log(`  ${path.basename(f)}`));
}

main();
