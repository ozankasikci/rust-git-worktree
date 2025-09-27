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

python3 - "$FORMULA_PATH" "$VERSION" "$SHA256" "$TARBALL_URL" <<'PY'
import hashlib
import sys
import urllib.request
import pathlib

formula_path = pathlib.Path(sys.argv[1])
new_version = sys.argv[2]
source_sha = sys.argv[3]
source_url = sys.argv[4]

text = formula_path.read_text().splitlines()

version_line_idx = None
old_version = None
for idx, line in enumerate(text):
    if line.strip().startswith("version "):
        version_line_idx = idx
        start = line.find('"') + 1
        end = line.rfind('"')
        old_version = line[start:end]
        text[idx] = line[:start] + new_version + line[end:]
        break

if old_version is None:
    raise SystemExit("could not locate version line in formula")

def download_sha(url):
    with urllib.request.urlopen(url) as resp:
        data = resp.read()
    return hashlib.sha256(data).hexdigest()

sha_cache = {source_url: source_sha}

def update_pair(i, url_line):
    url_start = url_line.find('"') + 1
    url_end = url_line.rfind('"')
    current_url = url_line[url_start:url_end]
    updated_url = current_url.replace(old_version, new_version)
    if updated_url != current_url:
        url_line = url_line[:url_start] + updated_url + url_line[url_end:]
    else:
        updated_url = current_url
    # find sha line following
    j = i + 1
    while j < len(text) and "sha256" not in text[j]:
        j += 1
    if j == len(text):
        raise SystemExit("expected sha256 line after url line")
    if updated_url not in sha_cache:
        sha_cache[updated_url] = download_sha(updated_url)
    new_sha = sha_cache[updated_url]
    sha_line = text[j]
    sha_start = sha_line.find('"') + 1
    sha_end = sha_line.rfind('"')
    text[j] = sha_line[:sha_start] + new_sha + sha_line[sha_end:]
    text[i] = url_line

for idx, line in enumerate(text):
    stripped = line.strip()
    if stripped.startswith("url "):
        update_pair(idx, line)

formula_path.write_text("\n".join(text) + "\n")
PY

# Remove stale bottle block to force regeneration
python3 - "$FORMULA_PATH" <<'PY'
from pathlib import Path
import re
path = Path(__import__('sys').argv[1])
text = path.read_text()
updated = re.sub(r"^bottle do\n.*?^end\n\n", "", text, flags=re.MULTILINE | re.DOTALL)
if updated != text:
    path.write_text(updated)
PY

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
