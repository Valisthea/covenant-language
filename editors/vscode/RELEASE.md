# Covenant VS Code Extension — Release Process

## Prerequisites (one-time)

```bash
cd editors/vscode
npm install
npm install -g @vscode/vsce   # or: npm install --save-dev @vscode/vsce
```

You also need `curl` available in your shell (pre-installed on macOS/Linux/Win11).

---

## Releasing a new version

### Step 1 — Bump versions and tag

```bash
# In the repo root, bump Cargo workspace version (e.g. 0.7.1)
# Edit Cargo.toml: version = "0.7.1"

# Bump extension version to match
cd editors/vscode
# Edit package.json: "version": "0.7.1"

# Add CHANGELOG entry, commit, tag
cd ../..
git add .
git commit -m "chore: release v0.7.1"
git tag v0.7.1
git push origin main --tags
```

This triggers `.github/workflows/release.yml`, which:
- Builds `covenant-lsp` for 5 platforms (linux-x64, linux-arm64, darwin-x64, darwin-arm64, win32-x64)
- Uploads each binary to the GitHub Release + as a GitHub Actions artifact

**Wait for all 5 builds to pass** (~10 min) before proceeding.

---

### Step 2 — Package per platform

```bash
cd editors/vscode
npm run compile

# Package all 5 platforms (downloads binaries from the GitHub Release)
node scripts/package-vsix.js

# Or a single platform:
node scripts/package-vsix.js --platform=win32-x64
```

Output: `covenant-lang-<platform>-<version>.vsix` for each platform.

---

### Step 3 — Publish to Marketplace

```bash
# Get a PAT at https://dev.azure.com/<org>/_usersSettings/tokens
# Required scope: Marketplace → Manage

export VSCE_PAT=<your-token>
node scripts/publish-vsix.js

# Or a single platform:
node scripts/publish-vsix.js --platform=win32-x64
```

---

### Step 4 — Verify

Visit:
```
https://marketplace.visualstudio.com/items?itemName=kairos-lab.covenant-lang
```

Check:
- Version shows the new tag
- All 5 platforms are listed under "Platform-specific extensions"
- Install from a fresh VS Code → open a `.cov` file → squiggle lines appear

---

## Troubleshooting

### "curl: command not found"
Install curl or replace the download step with a Node-native fetch (Node 18+):
```js
// In package-vsix.js, replace run(`curl ...`) with:
const { fetch } = require('node:https');
// or use node-fetch
```

### macOS Gatekeeper warning
The bundled `covenant-lsp` binary is unsigned. Users may see a Gatekeeper
warning the first time the extension activates. They can allow it via:
`System Settings → Privacy & Security → Allow`

To fully resolve: code-sign the binary in CI before bundling (requires an
Apple Developer account with Developer ID certificate).

### Linux glibc compatibility
Binaries built on `ubuntu-latest` link against glibc 2.35+. If users on
older distros report "GLIBC not found", rebuild with the `musl` target:
- Change `x86_64-unknown-linux-gnu` → `x86_64-unknown-linux-musl`
- Add `apt-get install -y musl-tools` before the build step

### Binary not found after install
Check Developer Tools console (`Help → Toggle Developer Tools`) for the
`Covenant: using LSP from ...` log line. If it says "system-path" instead
of "bundled", the binary wasn't packaged into the .vsix. Run `npx vsce ls`
to inspect the archive contents.

---

## Binary naming convention

| Platform    | Release asset name                | bin/ filename         |
|-------------|-----------------------------------|-----------------------|
| linux-x64   | `covenant-lsp-linux-x64`          | `covenant-lsp`        |
| linux-arm64 | `covenant-lsp-linux-arm64`        | `covenant-lsp`        |
| darwin-x64  | `covenant-lsp-darwin-x64`         | `covenant-lsp`        |
| darwin-arm64| `covenant-lsp-darwin-arm64`       | `covenant-lsp`        |
| win32-x64   | `covenant-lsp-win32-x64.exe`      | `covenant-lsp.exe`    |

---

## Platform-specific extension size

Each .vsix contains the TypeScript bundle (~4 KB) + the platform binary (~8–25 MB).
Total: ~10–27 MB per platform. Comparable to rust-analyzer (15–20 MB) and
the Solidity extension (30 MB+).
