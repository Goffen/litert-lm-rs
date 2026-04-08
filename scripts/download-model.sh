#!/usr/bin/env bash
set -euo pipefail

# Downloads a LiteRT-LM model from HuggingFace to the local cache.
#
# Usage:
#   ./scripts/download-model.sh [REPO_ID] [FILENAME]
#
# Defaults to gemma-4-E2B-it-litert-lm if no arguments given.
#
# Environment:
#   LITERT_LM_CACHE_DIR  Override cache directory (default: ~/.litert-lm/models)
#   HF_TOKEN             HuggingFace auth token for gated models

REPO_ID="${1:-litert-community/gemma-4-E2B-it-litert-lm}"
FILENAME="${2:-gemma-4-E2B-it.litertlm}"
CACHE_DIR="${LITERT_LM_CACHE_DIR:-$HOME/.litert-lm/models}"

# Convert repo_id to directory name: org/repo -> org--repo
DIR_NAME="${REPO_ID/\//$'--'}"
DEST_DIR="$CACHE_DIR/$DIR_NAME"
DEST_PATH="$DEST_DIR/$FILENAME"

if [ -f "$DEST_PATH" ]; then
    echo "$DEST_PATH"
    exit 0
fi

URL="https://huggingface.co/$REPO_ID/resolve/main/$FILENAME"

mkdir -p "$DEST_DIR"

AUTH_ARGS=()
TOKEN="${HF_TOKEN:-${HUGGING_FACE_HUB_TOKEN:-}}"
if [ -n "$TOKEN" ]; then
    AUTH_ARGS=(-H "Authorization: Bearer $TOKEN")
fi

echo >&2 "Downloading $URL"
echo >&2 "  -> $DEST_PATH"

TMP_PATH="$DEST_PATH.download"
trap 'rm -f "$TMP_PATH"' EXIT

curl -fSL --progress-bar ${AUTH_ARGS[@]+"${AUTH_ARGS[@]}"} -o "$TMP_PATH" "$URL"
mv "$TMP_PATH" "$DEST_PATH"
trap - EXIT

echo >&2 "Done."
echo "$DEST_PATH"
