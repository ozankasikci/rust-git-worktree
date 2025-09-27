#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: release-homebrew.sh <version> [tap-path]

Prepare a release for the rsworktree Homebrew tap by updating the formula
URL, version, and checksum. The tap path defaults to $HOMEBREW_TAP_PATH or
../homebrew-tap relative to this repository.
EOF
}

if [[ $# -lt 1 ]]; then
  usage >&2
  exit 1
fi

require() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command '$1' not found" >&2
    exit 1
  fi
}

require git
require curl
require perl

checksum_cmd=""
if command -v shasum >/dev/null 2>&1; then
  checksum_cmd="shasum -a 256"
elif command -v sha256sum >/dev/null 2>&1; then
  checksum_cmd="sha256sum"
else
  echo "error: neither shasum nor sha256sum found" >&2
  exit 1
fi

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
VERSION=$1
TAP_PATH=${2:-${HOMEBREW_TAP_PATH:-"$REPO_ROOT/../homebrew-tap"}}
FORMULA_PATH="$TAP_PATH/Formula/rsworktree.rb"

infer_repo_url() {
  case "$1" in
    git@github.com:*)
      local path=${1#git@github.com:}
      path=${path%.git}
      echo "https://github.com/$path"
      ;;
    https://github.com/*)
      local path=${1#https://github.com/}
      path=${path%.git}
      echo "https://github.com/$path"
      ;;
    *)
      echo ""
      ;;
  esac
}

if [[ ! -d "$TAP_PATH" ]]; then
  echo "error: tap path '$TAP_PATH' not found" >&2
  exit 1
fi

if [[ ! -f "$FORMULA_PATH" ]]; then
  echo "error: formula '$FORMULA_PATH' not found" >&2
  exit 1
fi

if [[ -n $(git -C "$TAP_PATH" status --porcelain) ]]; then
  echo "error: tap repository has uncommitted changes. Commit or stash them first." >&2
  exit 1
fi

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: version '$VERSION' is not valid semver (expected X.Y.Z)" >&2
  exit 1
fi

origin_url=$(git -C "$REPO_ROOT" remote get-url origin 2>/dev/null || true)
default_repo_url=$(infer_repo_url "$origin_url")
if [[ -z "$default_repo_url" ]]; then
  default_repo_url="https://github.com/ozankasikci/rust-git-worktree"
fi

TARBALL_URL=${HOMEBREW_TARBALL_URL:-"${default_repo_url}/archive/refs/tags/v${VERSION}.tar.gz"}

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT
tarball="$tmpdir/rsworktree-${VERSION}.tar.gz"

echo "Downloading release tarball to compute checksum..."
if ! curl -sSL --fail "$TARBALL_URL" -o "$tarball"; then
  echo "error: failed to download $TARBALL_URL" >&2
  echo "       make sure tag v$VERSION is published or set HOMEBREW_TARBALL_URL" >&2
  exit 1
fi

SHA256=$($checksum_cmd "$tarball" | awk '{print $1}')

echo "Updating formula at $FORMULA_PATH"

perl -0pi -e 's/^  url "[^"]+"/  url "'"$TARBALL_URL"'"/' "$FORMULA_PATH"
perl -0pi -e 's/^  sha256 "[^"]+"/  sha256 "'"$SHA256"'"/' "$FORMULA_PATH"
perl -0pi -e 's/^  version "[^"]+"/  version "'"$VERSION"'"/' "$FORMULA_PATH" 2>/dev/null || true

# Remove stale bottle block so new bottles can be generated after the update.
perl -0pi -e 's/^bottle do\n.*?^end\n\n//ms' "$FORMULA_PATH" || true

git -C "$TAP_PATH" add "$FORMULA_PATH"

tap_remote=$(git -C "$TAP_PATH" remote get-url origin 2>/dev/null || true)
tap_name=""
if [[ -n "$tap_remote" ]]; then
  case "$tap_remote" in
    git@github.com:*) tap_name=${tap_remote#git@github.com:}; tap_name=${tap_name%.git} ;;
    https://github.com/*) tap_name=${tap_remote#https://github.com/}; tap_name=${tap_name%.git} ;;
    *) tap_name="" ;;
  esac
fi

echo "Formula updated. Next steps:"
echo "  1. cd $TAP_PATH"
echo "  2. git commit -m \"rsworktree $VERSION\""
echo "  3. (optional) brew install --build-from-source Formula/rsworktree.rb"
if [[ -n "$tap_name" ]]; then
  echo "  4. (optional) brew audit --tap $tap_name rsworktree --strict"
else
  echo "  4. (optional) brew audit --strict Formula/rsworktree.rb"
fi
echo "  5. git push / open PR"
