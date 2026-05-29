#!/usr/bin/env bash
# Download the official LiteRT-LM CLiteRTLM xcframeworks into ../vendor/.
#
# These are the Apple (iOS device + simulator, macOS) C-API binaries that
# build.rs links on Apple targets. They're gitignored (~165 MB extracted),
# so a clean checkout must run this once before building for iOS / macOS.
# Android is unaffected — it uses the bazel-built libengine_shared.so.
#
# Pinned to the LiteRT-LM release whose C API litert-lm-rs binds against.
# Checksums are Google's own published values (from the upstream
# Package.swift binaryTarget entries) — a mismatch means the release asset
# changed and we should NOT silently use it.
#
# Usage:  ./scripts/fetch_xcframeworks.sh        # idempotent; skips if present
#         ./scripts/fetch_xcframeworks.sh --force # re-download even if present
set -euo pipefail

VERSION="v0.12.0"
BASE="https://github.com/google-ai-edge/LiteRT-LM/releases/download/${VERSION}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR="$(cd "$SCRIPT_DIR/.." && pwd)/vendor"

FORCE=0
[[ "${1:-}" == "--force" ]] && FORCE=1

# zip | sha256 | extracted-dir-name
IOS_ZIP="CLiteRTLM.xcframework.zip"
IOS_SHA="3c2a11ecc8511d1e74efa7ca308dc7130c95223325c33212337ffb0563b79cde"
IOS_DIR="CLiteRTLM.xcframework"

MAC_ZIP="CLiteRTLM_mac.xcframework.zip"
MAC_SHA="a8238da94b31ce0383e0fd52a0a729b9c18a1055170a995f0aa32056bd9822e5"
MAC_DIR="CLiteRTLM_mac.xcframework"

mkdir -p "$VENDOR"

fetch() {
    local zip="$1" sha="$2" dir="$3"
    local dest="$VENDOR/$dir"
    local marker="$dest/.litertlm_version"

    if [[ "$FORCE" -eq 0 && -d "$dest" && "$(cat "$marker" 2>/dev/null || true)" == "$VERSION" ]]; then
        echo "  ✓ $dir already present ($VERSION)"
        return
    fi

    local tmp
    tmp="$(mktemp -t clitertlm.XXXXXX).zip"
    echo "  ↓ $zip"
    curl -fsSL -o "$tmp" "$BASE/$zip"

    local got
    got="$(shasum -a 256 "$tmp" | awk '{print $1}')"
    if [[ "$got" != "$sha" ]]; then
        echo "  ✗ checksum mismatch for $zip" >&2
        echo "      expected: $sha" >&2
        echo "      got:      $got" >&2
        rm -f "$tmp"
        exit 1
    fi

    rm -rf "$dest"
    # The zip holds the .xcframework at its top level, so extract into vendor/.
    unzip -q "$tmp" -d "$VENDOR"
    rm -f "$tmp"
    # Strip macOS zip cruft (the __MACOSX sidecar dir + .DS_Store files).
    rm -rf "$VENDOR/__MACOSX"
    find "$dest" -name ".DS_Store" -delete 2>/dev/null || true
    echo "$VERSION" > "$marker"
    echo "  ✓ $dir extracted + checksum verified"
}

echo "Fetching LiteRT-LM xcframeworks ($VERSION) → $VENDOR"
fetch "$IOS_ZIP" "$IOS_SHA" "$IOS_DIR"
fetch "$MAC_ZIP" "$MAC_SHA" "$MAC_DIR"
echo "Done. build.rs will link these on Apple targets."
