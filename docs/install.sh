#!/bin/sh
# digse installer — Linux.
#
# Downloads the latest release binary from github.com/openepoch/digse and
# installs it into ~/.local/bin (no sudo required). Re-running updates in place.
#
# One-liner:
#   curl -fsSL https://raw.githubusercontent.com/openepoch/digse/main/docs/install.sh | sh
#
# Install a specific release tag:
#   curl -fsSL https://raw.githubusercontent.com/openepoch/digse/main/docs/install.sh | VERSION=v0.2.0 sh
#
# Override the install directory:
#   curl ... | DIGSE_INSTALL_DIR=/usr/local/bin sh   # (needs write access there)
set -eu

REPO="openepoch/digse"
INSTALL_DIR="${DIGSE_INSTALL_DIR:-$HOME/.local/bin}"

err() { printf 'install: error: %s\n' "$*" >&2; exit 1; }

# --- map the current OS/arch to a release target triple ---------------------
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
    Linux)  os=unknown-linux-gnu ;;
    *) err "unsupported OS '$os' (this installer is for Linux; on Windows use install.ps1)" ;;
esac
case "$arch" in
    x86_64|amd64)  arch=x86_64 ;;
    *) err "unsupported architecture '$arch'" ;;
esac
target="$arch-$os"

# --- resolve the release tag (asset URLs are then constructed directly) ------
if [ -n "${VERSION:-}" ]; then
    tag="$VERSION"
else
    api="https://api.github.com/repos/$REPO/releases/latest"
    json="$(curl -fsSL "$api" || err "could not fetch latest release from $api")"
    tag="$(printf '%s\n' "$json" | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')"
    [ -n "$tag" ] || err "could not parse tag_name from release JSON"
fi
asset_url="https://github.com/$REPO/releases/download/$tag/digse-$target.tar.gz"
printf 'install: latest release is %s for %s\n' "$tag" "$target"

# --- download + extract ------------------------------------------------------
tmpdir="$(mktemp -d 2>/dev/null || mktemp -d -t digse)"
trap 'rm -rf "$tmpdir"' EXIT
tarball="$tmpdir/digse.tar.gz"
curl -fsSL -o "$tarball" "$asset_url" || err "download failed: $asset_url"
tar -xzf "$tarball" -C "$tmpdir"
[ -f "$tmpdir/digse" ] || err "archive did not contain a 'digse' binary"

# --- install (overwrite) -----------------------------------------------------
mkdir -p "$INSTALL_DIR"
prev_version=""
if [ -x "$INSTALL_DIR/digse" ]; then
    prev_version="$("$INSTALL_DIR/digse" --version 2>/dev/null | head -1 || true)"
fi

mv -f "$tmpdir/digse" "$INSTALL_DIR/digse"
chmod +x "$INSTALL_DIR/digse"

new_version="$("$INSTALL_DIR/digse" --version 2>/dev/null | head -1 || true)"
if [ -n "$prev_version" ] && [ "$prev_version" != "$new_version" ]; then
    printf 'install: updated %s -> %s\n' "$prev_version" "$new_version"
elif [ -n "$prev_version" ]; then
    printf 'install: already at %s (reinstalled)\n' "$new_version"
else
    printf 'install: installed %s\n' "$new_version"
fi

# --- PATH hint ---------------------------------------------------------------
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        printf '\ninstall: WARNING: %s is not on your PATH.\n' "$INSTALL_DIR" >&2
        printf '         Add this to your shell profile (~/.bashrc or ~/.zshrc):\n' >&2
        printf '             export PATH="%s:$PATH"\n' "$INSTALL_DIR" >&2
        ;;
esac

printf '\nRun "digse version" to confirm, then "digse start".\n'
