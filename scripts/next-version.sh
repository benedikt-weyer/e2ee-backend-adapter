#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/next-version.sh [patch|minor|major]

Print the next semantic version based on the latest semantic git tag.
If no semantic tag exists yet, the current workspace Cargo.toml version is used as the base.
EOF
}

bump_type="${1:-patch}"

case "$bump_type" in
  patch|minor|major) ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    usage >&2
    exit 1
    ;;
esac

script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(cd -- "$script_dir/.." && pwd)

cd "$repo_root"

last_tag=$(git tag --sort=-v:refname | grep -E '^(v)?[0-9]+\.[0-9]+\.[0-9]+$' | head -n 1 || true)

if [[ -n "$last_tag" ]]; then
  base_version="${last_tag#v}"
else
  base_version=$(awk '
    $0 == "[workspace.package]" { in_section=1; next }
    /^\[/ && in_section { exit }
    in_section && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' Cargo.toml)
fi

if [[ ! "$base_version" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "Base version '$base_version' is not a supported semantic version." >&2
  exit 1
fi

major="${BASH_REMATCH[1]}"
minor="${BASH_REMATCH[2]}"
patch="${BASH_REMATCH[3]}"

case "$bump_type" in
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

printf '%s.%s.%s\n' "$major" "$minor" "$patch"