#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAPES_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$TAPES_DIR/.." && pwd)"
cd "$REPO_ROOT"

names=(demo-create demo-cd demo-ls demo-rm demo-pr demo-merge-pr)

if ! command -v rsworktree >/dev/null 2>&1; then
  echo "rsworktree is not on PATH. Install it or run `cargo install --path .` from the repo." >&2
  exit 1
fi

for name in "${names[@]}"; do
  rsworktree rm "$name" --force >/dev/null 2>&1 || true
  git branch -D "$name" >/dev/null 2>&1 || true
  rm -rf ".rsworktree/$name" >/dev/null 2>&1 || true
done

git worktree prune >/dev/null 2>&1 || true
rm -rf "$REPO_ROOT/.bin"
