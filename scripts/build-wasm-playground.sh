#!/usr/bin/env bash
#
# Build the covenant-wasm-bindings crate and drop the bundle into the
# playground repo. Run from the compiler repo root.
#
# Usage:
#   ./scripts/build-wasm-playground.sh
#   PLAYGROUND_DIR=../covenant-playground ./scripts/build-wasm-playground.sh
#
# Prerequisites:
#   - Rust toolchain (1.75+) with wasm32-unknown-unknown target installed
#   - wasm-pack (any 0.12+ release)

set -euo pipefail

PLAYGROUND_DIR="${PLAYGROUND_DIR:-../covenant-playground}"
CRATE_DIR="crates/covenant-wasm-bindings"

if [[ ! -d "$PLAYGROUND_DIR" ]]; then
    echo "ERROR: playground directory not found at $PLAYGROUND_DIR" >&2
    echo "       set PLAYGROUND_DIR env var if it lives elsewhere" >&2
    exit 1
fi

# wasm-pack resolves --out-dir relative to the crate directory (not cwd),
# so we must absolutize before passing it in. Use a portable
# "cd && pwd" trick that works on macOS bash + Git Bash on Windows.
PLAYGROUND_ABS="$(cd "$PLAYGROUND_DIR" && pwd)"
OUT_DIR="${PLAYGROUND_ABS}/public/covenant-wasm"

if ! command -v wasm-pack >/dev/null 2>&1; then
    echo "ERROR: wasm-pack not found in PATH" >&2
    echo "       install via: cargo install wasm-pack" >&2
    exit 1
fi

echo "==> Building covenant-wasm-bindings"
mkdir -p "$OUT_DIR"

# wasm-pack invokes cargo with target=wasm32-unknown-unknown, runs
# wasm-bindgen-cli to generate the JS glue, and finally wasm-opt for
# size optimization (Oz profile is set in the crate's Cargo.toml).
#
# Flags:
#   --target web         Produce an ES module suitable for `import`.
#   --release            Strip debug info, run wasm-opt -Oz.
#   --no-typescript      We hand-author the .d.ts to keep the surface
#                        identical to what the playground expects.
#   --no-pack            Skip generating package.json, playground
#                        imports the .js + .wasm directly, no npm.
wasm-pack build \
    --target web \
    --release \
    --no-typescript \
    --no-pack \
    --out-dir "$OUT_DIR" \
    --out-name covenant_wasm_bindings \
    "$CRATE_DIR"

# Copy hand-authored .d.ts (next to the script, source of truth).
cp "$CRATE_DIR/covenant_wasm_bindings.d.ts" "$OUT_DIR/"

# Trim files that wasm-pack produces but we don't need.
rm -f "$OUT_DIR/.gitignore"

# Report the bundle size: useful for the size-budget gate in CI.
# Use `wc -c` (POSIX) instead of stat to dodge the BSD/GNU/MSYS
# format-flag fragmentation.
WASM_PATH="$OUT_DIR/covenant_wasm_bindings_bg.wasm"
JS_PATH="$OUT_DIR/covenant_wasm_bindings.js"
WASM_SIZE=$(wc -c < "$WASM_PATH" | tr -d ' ')
JS_SIZE=$(wc -c < "$JS_PATH" | tr -d ' ')
GZIP_SIZE=$(gzip -c "$WASM_PATH" | wc -c | tr -d ' ')

echo
echo "==> Bundle ready in $OUT_DIR :"
echo "    WASM raw  : $WASM_SIZE bytes ($((WASM_SIZE / 1024)) KB)"
echo "    WASM gzip : $GZIP_SIZE bytes ($((GZIP_SIZE / 1024)) KB)"
echo "    JS  glue  : $JS_SIZE bytes ($((JS_SIZE / 1024)) KB)"
echo
ls -la "$OUT_DIR"

# Sprint 22 size budget: fail the build if we blow past it.
if [[ "$WASM_SIZE" -gt 2621440 ]]; then
    echo "ERROR: WASM bundle exceeds 2.5 MB raw budget" >&2
    exit 1
fi
if [[ "$GZIP_SIZE" -gt 1048576 ]]; then
    echo "ERROR: WASM bundle exceeds 1 MB gzipped budget" >&2
    exit 1
fi
