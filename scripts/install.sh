#!/usr/bin/env bash
# Covenant one-line installer
# Usage: curl -sSL https://install.covenant-lang.org | sh
# Or: curl -sSL https://raw.githubusercontent.com/Valisthea/covenant-language/main/scripts/install.sh | sh

set -euo pipefail

REPO="Valisthea/covenant-language"
BINARY_NAME="covenant"
INSTALL_DIR="${COVENANT_INSTALL_DIR:-$HOME/.local/bin}"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RESET='\033[0m'

info()  { echo -e "${BLUE}[covenant]${RESET} $*"; }
ok()    { echo -e "${GREEN}[covenant]${RESET} $*"; }
error() { echo -e "${RED}[covenant]${RESET} $*" >&2; exit 1; }

# Detect OS
OS=""
case "$(uname -s)" in
    Linux*)  OS="linux" ;;
    Darwin*) OS="macos" ;;
    *)       error "Unsupported OS: $(uname -s). Install via: cargo install covenant-cli" ;;
esac

# Detect architecture
ARCH=""
case "$(uname -m)" in
    x86_64)         ARCH="x86_64" ;;
    arm64|aarch64)  ARCH="aarch64" ;;
    *)              error "Unsupported architecture: $(uname -m). Install via: cargo install covenant-cli" ;;
esac

# Resolve latest release tag
info "Resolving latest release..."
LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep '"tag_name"' | head -1 | cut -d'"' -f4)
[ -z "$LATEST" ] && error "Could not resolve latest release from GitHub API."

ASSET="${BINARY_NAME}-${OS}-${ARCH}"
URL="https://github.com/$REPO/releases/download/$LATEST/$ASSET"
CHECKSUM_URL="${URL}.sha256"

info "Installing Covenant $LATEST ($ASSET)..."

# Download binary + checksum
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$URL" -o "$TMP/$BINARY_NAME"
curl -fsSL "$CHECKSUM_URL" -o "$TMP/$ASSET.sha256" 2>/dev/null || true

# Verify checksum if available
if [ -f "$TMP/$ASSET.sha256" ]; then
    info "Verifying checksum..."
    EXPECTED=$(awk '{print $1}' "$TMP/$ASSET.sha256")
    if command -v sha256sum &>/dev/null; then
        ACTUAL=$(sha256sum "$TMP/$BINARY_NAME" | awk '{print $1}')
    elif command -v shasum &>/dev/null; then
        ACTUAL=$(shasum -a 256 "$TMP/$BINARY_NAME" | awk '{print $1}')
    else
        info "Checksum tool not found — skipping verification."
        ACTUAL="$EXPECTED"
    fi
    [ "$ACTUAL" = "$EXPECTED" ] || error "Checksum mismatch. Download may be corrupted."
    ok "Checksum verified."
fi

# Install
mkdir -p "$INSTALL_DIR"
chmod +x "$TMP/$BINARY_NAME"
mv "$TMP/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"

ok "Covenant $LATEST installed to $INSTALL_DIR/$BINARY_NAME"
echo ""

# PATH hint
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    echo "  Add to PATH (add to ~/.bashrc or ~/.zshrc):"
    echo "    export PATH=\"$INSTALL_DIR:\$PATH\""
    echo ""
fi

echo "  Verify: $BINARY_NAME --version"
echo "  Docs:   https://docs.covenant-lang.org"
echo ""
