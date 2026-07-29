#!/usr/bin/env node
/**
 * publish-vsix.js: publish the platform bundles to both extension marketplaces.
 *
 * This used to publish to the VS Code Marketplace only, and reaching Open VSX
 * meant remembering a second command. v0.9.6 went out to Microsoft and never
 * reached Open VSX, so Cursor users, who install from Open VSX, sat two
 * versions behind for weeks with nothing reporting it. Publishing is one
 * operation now, and a marketplace that ends up behind is an error rather
 * than a thing you notice later.
 *
 * Usage:
 *   VSCE_PAT=<token> OVSX_PAT=<token> node scripts/publish-vsix.js
 *   VSCE_PAT=<token> node scripts/publish-vsix.js --only=vscode
 *   OVSX_PAT=<token> node scripts/publish-vsix.js --only=openvsx
 *   ... --platform=win32-x64        (repeatable)
 *   ... --version=0.9.7
 *
 * VSCE_PAT: https://dev.azure.com/<org>/_usersSettings/tokens, scope
 *           Marketplace > Manage.
 * OVSX_PAT: https://open-vsx.org/user-settings/tokens.
 */

"use strict";

const fs = require("fs");
const path = require("path");
const https = require("https");
const { execSync } = require("child_process");

const PLATFORMS = [
  "linux-x64",
  "linux-arm64",
  "darwin-x64",
  "darwin-arm64",
  "win32-x64",
];

const PUBLISHER = "kairos-lab";
const EXTENSION = "covenant-lang";

// ── CLI args ──────────────────────────────────────────────────────────────

function parseArgs() {
  const platforms = [];
  let version = null;
  let only = null;
  let check = false;

  for (const arg of process.argv.slice(2)) {
    if (arg === "--check") {
      check = true;
    } else if (arg.startsWith("--platform=")) {
      platforms.push(arg.slice("--platform=".length));
    } else if (arg.startsWith("--version=")) {
      version = arg.slice("--version=".length);
    } else if (arg.startsWith("--only=")) {
      only = arg.slice("--only=".length);
    }
  }

  if (only && !["vscode", "openvsx"].includes(only)) {
    console.error(`Unknown --only=${only}; expected vscode or openvsx.`);
    process.exit(1);
  }

  return {
    platforms: platforms.length ? platforms : PLATFORMS,
    version,
    only,
    check,
  };
}

// ── Marketplace state ─────────────────────────────────────────────────────

function get(url, options, body) {
  return new Promise((resolve) => {
    const request = https.request(url, options, (response) => {
      let data = "";
      response.on("data", (chunk) => (data += chunk));
      response.on("end", () => {
        try {
          resolve(JSON.parse(data));
        } catch {
          resolve(null);
        }
      });
    });
    request.on("error", () => resolve(null));
    if (body) request.write(body);
    request.end();
  });
}

async function openVsxVersion() {
  const json = await get(
    `https://open-vsx.org/api/${PUBLISHER}/${EXTENSION}`,
    { method: "GET" }
  );
  return json?.version ?? null;
}

async function vsCodeVersion() {
  const body = JSON.stringify({
    filters: [
      { criteria: [{ filterType: 7, value: `${PUBLISHER}.${EXTENSION}` }] },
    ],
    flags: 914,
  });
  const json = await get(
    "https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery",
    {
      method: "POST",
      headers: {
        Accept: "application/json;api-version=7.2-preview.1",
        "Content-Type": "application/json",
        "Content-Length": Buffer.byteLength(body),
      },
    },
    body
  );
  return json?.results?.[0]?.extensions?.[0]?.versions?.[0]?.version ?? null;
}

// ── Publishing ────────────────────────────────────────────────────────────

function publishBundles(label, command, bundles) {
  const published = [];
  console.log(`\n══ ${label} ══`);
  for (const { platform, vsixPath, vsixName } of bundles) {
    console.log(`\n  ${platform}`);
    execSync(command(vsixPath), {
      stdio: "inherit",
      cwd: path.resolve(__dirname, ".."),
    });
    published.push(platform);
    console.log(`  ok ${vsixName}`);
  }
  return published;
}

async function main() {
  const { platforms, version: override, only, check } = parseArgs();

  const ext = path.resolve(__dirname, "..");
  const pkg = JSON.parse(
    fs.readFileSync(path.join(ext, "package.json"), "utf8")
  );
  const version = override ?? pkg.version;

  // --check answers "is what is published what this repo says it is" without
  // needing a token. Nothing asked that question for two releases.
  if (check) {
    const [live, ovsx] = await Promise.all([vsCodeVersion(), openVsxVersion()]);
    console.log(`\nThis repo builds ${version}`);
    console.log(`  VS Code Marketplace   ${live ?? "unreachable"}`);
    console.log(`  Open VSX              ${ovsx ?? "unreachable"}`);
    const drifted = [live, ovsx].filter((v) => v !== null && v !== version);
    if (drifted.length) {
      console.log(
        `\n${drifted.length === 2 ? "Both marketplaces are" : "One marketplace is"} behind.`
      );
      process.exit(1);
    }
    console.log("\nBoth marketplaces serve this version.");
    return;
  }

  const wantVsCode = only !== "openvsx";
  const wantOpenVsx = only !== "vscode";

  const vscePat = process.env.VSCE_PAT;
  const ovsxPat = process.env.OVSX_PAT;

  const missing = [];
  if (wantVsCode && !vscePat) missing.push("VSCE_PAT (VS Code Marketplace)");
  if (wantOpenVsx && !ovsxPat) missing.push("OVSX_PAT (Open VSX)");
  if (missing.length) {
    console.error(`\nMissing token: ${missing.join(", ")}.`);
    console.error(
      "Publish one marketplace at a time with --only=vscode or --only=openvsx\n" +
        "if that is what you intend, so the omission is deliberate."
    );
    process.exit(1);
  }

  // Resolve every bundle up front. A missing platform is a hard stop: half a
  // release is how the marketplaces drifted apart in the first place.
  const bundles = [];
  const absent = [];
  for (const platform of platforms) {
    const vsixName = `covenant-lang-${platform}-${version}.vsix`;
    const vsixPath = path.join(ext, vsixName);
    if (fs.existsSync(vsixPath)) bundles.push({ platform, vsixPath, vsixName });
    else absent.push(vsixName);
  }
  if (absent.length) {
    console.error(`\nNot built: ${absent.join(", ")}`);
    console.error("Run `node scripts/package-vsix.js` first.");
    process.exit(1);
  }

  console.log(`\nCovenant VS Code extension, publishing v${version}`);
  console.log(`Platforms: ${platforms.join(", ")}`);
  console.log(
    `Targets  : ${[wantVsCode && "VS Code Marketplace", wantOpenVsx && "Open VSX"]
      .filter(Boolean)
      .join(", ")}`
  );

  if (wantVsCode) {
    publishBundles(
      "VS Code Marketplace",
      (p) => `npx @vscode/vsce publish --packagePath "${p}" --pat "${vscePat}"`,
      bundles
    );
  }

  if (wantOpenVsx) {
    publishBundles(
      "Open VSX",
      (p) => `npx ovsx publish "${p}" --pat "${ovsxPat}"`,
      bundles
    );
  }

  // Read both marketplaces back rather than trusting the exit codes. The
  // registries index asynchronously, so a version still behind here is worth
  // re-checking before it is worth panicking about.
  console.log("\n══ Verifying ══");
  const [live, ovsx] = await Promise.all([vsCodeVersion(), openVsxVersion()]);
  const rows = [
    ["VS Code Marketplace", live, wantVsCode],
    ["Open VSX", ovsx, wantOpenVsx],
  ];
  let behind = false;
  for (const [name, reported, targeted] of rows) {
    const state =
      reported === version
        ? "up to date"
        : reported === null
          ? "unreachable"
          : `behind at ${reported}`;
    console.log(`  ${name.padEnd(21)} ${state}`);
    if (targeted && reported !== version && reported !== null) behind = true;
  }

  if (behind) {
    console.error(
      "\nA marketplace this run targeted is still behind. Registries can lag a\n" +
        "few minutes; re-run this script to re-check before republishing."
    );
    process.exit(1);
  }
  console.log("\nBoth marketplaces serve the same version.");
}

main();
