#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAPES_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$TAPES_DIR/.." && pwd)"

if ! command -v vhs >/dev/null 2>&1; then
  echo "vhs is not installed. Install it from https://github.com/charmbracelet/vhs" >&2
  exit 1
fi

# Clean up existing demos
"$SCRIPT_DIR/reset_tapes.sh"

cd "$REPO_ROOT"

mkdir -p "$TAPES_DIR/gifs"

for tape in create cd ls rm; do
  vhs < "$TAPES_DIR/${tape}.tape"
done
