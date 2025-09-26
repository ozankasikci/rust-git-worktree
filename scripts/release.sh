#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO_ROOT"

require() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command '$1' not found" >&2
    exit 1
  fi
}

require git
require cargo
require gh
require perl
require python3

if [[ -n $(git status --porcelain) ]]; then
  echo "error: working tree not clean. Commit or stash changes before releasing." >&2
  exit 1
fi

current_branch=$(git symbolic-ref --short HEAD 2>/dev/null || git rev-parse --abbrev-ref HEAD)
if [[ "$current_branch" != "main" && "$current_branch" != "master" ]]; then
  echo "warning: releasing from branch '$current_branch'. Press Enter to continue or Ctrl-C to abort."
  read -r
fi

current_version=$(grep '^version = "' Cargo.toml | head -n1 | cut -d '"' -f2)
if [[ -z "$current_version" ]]; then
  echo "error: unable to determine current version" >&2
  exit 1
fi

echo "Current version: $current_version"
echo "Select version bump type:"
select bump in "patch" "minor" "major" "custom"; do
  case "$bump" in
    patch|minor|major|custom)
      break
      ;;
    *) echo "Please choose 1-4." ;;
  esac
done

calc_semver() {
  local ver=$1 part=$2
  IFS='.' read -r major minor patch <<< "$ver"
  case "$part" in
    patch)
      patch=$((patch + 1))
      ;;
    minor)
      minor=$((minor + 1))
      patch=0
      ;;
    major)
      major=$((major + 1))
      minor=0
      patch=0
      ;;
  esac
  printf '%s.%s.%s' "$major" "$minor" "$patch"
}

case "$bump" in
  patch|minor|major)
    new_version=$(calc_semver "$current_version" "$bump")
    ;;
  custom)
    read -rp "Enter new version: " new_version
    ;;
esac

if [[ -z "${new_version:-}" ]]; then
  echo "error: new version is empty" >&2
  exit 1
fi

if [[ ! "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: version '$new_version' is not valid semver (expected format X.Y.Z)" >&2
  exit 1
fi

if [[ "$new_version" == "$current_version" ]]; then
  echo "error: new version matches current version" >&2
  exit 1
fi

if git rev-parse "v${new_version}" >/dev/null 2>&1; then
  echo "error: tag v${new_version} already exists" >&2
  exit 1
fi

echo "Releasing version $new_version"
read -rp "Continue? [y/N] " confirm
if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
  echo "Aborted."
  exit 0
fi

perl -pi -e 's/^version = \".*\"$/version = \"'"$new_version"'\"/' Cargo.toml
perl -0pi -e 's/name = \"rsworktree\"\nversion = \"[^\"]+\"/name = \"rsworktree\"\nversion = \"'"$new_version"'\"/' Cargo.lock

today=$(date +%Y-%m-%d)
change_status=$(python3 - "$new_version" "$today" <<'PY'
import sys
from pathlib import Path
version = sys.argv[1]
today = sys.argv[2]
path = Path("CHANGELOG.md")
text = path.read_text()
if f"## [{version}]" in text:
    print("exists")
    sys.exit(0)
section = f"## [{version}] - {today}\n\n### Added\n- _TBD_\n\n"
marker = "\n## ["
idx = text.find(marker)
if idx == -1:
    updated = text.rstrip() + "\n\n" + section
else:
    updated = text[:idx] + "\n" + section + text[idx+1:]
path.write_text(updated)
print("inserted")
PY
)

if [[ "$change_status" == *"inserted"* ]]; then
  echo "A placeholder entry for $new_version was added to CHANGELOG.md."
  echo "Please edit CHANGELOG.md to describe the release."
  ${EDITOR:-vi} CHANGELOG.md || true
  read -rp "Press Enter to continue once CHANGELOG.md is updated..."
fi

if grep -q "_TBD_" CHANGELOG.md; then
  echo "error: changelog still contains placeholder '_TBD_'." >&2
  exit 1
fi

cargo fmt
cargo test

git add Cargo.toml Cargo.lock CHANGELOG.md

git commit -m "Release $new_version"
git tag -a "v$new_version" -m "Release $new_version"

git push origin "$current_branch"
git push origin "v$new_version"

cargo publish --locked

notes_file=$(mktemp)
python3 - "$new_version" "$notes_file" <<'PY'
import sys
import re
from pathlib import Path
version = sys.argv[1]
notes_path = Path(sys.argv[2])
text = Path("CHANGELOG.md").read_text()
pattern = re.compile(rf"^## \[{re.escape(version)}\][^\n]*\n", re.MULTILINE)
match = pattern.search(text)
if not match:
    sys.exit("No changelog entry found for release")
start = match.start()
rest = text[start:]
following = re.search(r"^## \[", rest[match.end()-start:], re.MULTILINE)
if following:
    body = rest[:match.end()-start + following.start()]
else:
    body = rest
notes_path.write_text(body.strip() + "\n")
PY

if ! grep -Fqs "## [$new_version" "$notes_file"; then
  echo "error: failed to extract release notes" >&2
  rm -f "$notes_file"
  exit 1
fi

gh release create "v$new_version" --title "v$new_version" --notes-file "$notes_file"
rm -f "$notes_file"

echo "Release v$new_version created and pushed."
echo "GitHub Actions release workflow will publish macOS binaries automatically."
