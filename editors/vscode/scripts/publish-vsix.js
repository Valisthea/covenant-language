#!/usr/bin/env node
/**
 * publish-vsix.js — Publish platform-specific .vsix files to the VS Code Marketplace.
 *
 * Usage:
 *   VSCE_PAT=<token> node scripts/publish-vsix.js
 *   VSCE_PAT=<token> node scripts/publish-vsix.js --platform=win32-x64
 *   VSCE_PAT=<token> node scripts/publish-vsix.js --version=0.7.1
 *
 * Get a PAT at: https://dev.azure.com/<org>/_usersSettings/tokens
 * Required scopes: Marketplace → Manage
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

// ── Main ──────────────────────────────────────────────────────────────────

function main() {
  const pat = process.env.VSCE_PAT;
  if (!pat) {
    console.error("Error: VSCE_PAT environment variable is not set.");
    console.error(
      "Get one at https://dev.azure.com/<org>/_usersSettings/tokens"
    );
    console.error("Required scope: Marketplace → Manage");
    process.exit(1);
  }

  const { platforms, version: versionOverride } = parseArgs();

  const pkg = JSON.parse(
    fs.readFileSync(path.join(__dirname, "..", "package.json"), "utf8")
  );
  const version = versionOverride ?? pkg.version;
  const ext = path.resolve(__dirname, "..");

  console.log(`\nCovenant VS Code Extension — publishing v${version}`);
  console.log(`Platforms: ${platforms.join(", ")}\n`);

  const published = [];
  const skipped = [];

  for (const platform of platforms) {
    const vsixName = `covenant-lang-${platform}-${version}.vsix`;
    const vsixPath = path.join(ext, vsixName);

    if (!fs.existsSync(vsixPath)) {
      console.warn(`  ⚠  ${vsixName} not found — skipping`);
      skipped.push(platform);
      continue;
    }

    console.log(`\n══ Publishing ${platform} ══`);
    execSync(
      `npx @vscode/vsce publish --packagePath "${vsixPath}" --pat "${pat}"`,
      { stdio: "inherit", cwd: ext }
    );

    published.push(platform);
    console.log(`  ✓ ${vsixName}`);
  }

  console.log("\n══ Summary ══");
  if (published.length) console.log(`  Published: ${published.join(", ")}`);
  if (skipped.length) console.log(`  Skipped  : ${skipped.join(", ")}`);

  console.log(
    "\nhttps://marketplace.visualstudio.com/items?itemName=kairos-lab.covenant-lang\n"
  );
}

main();
