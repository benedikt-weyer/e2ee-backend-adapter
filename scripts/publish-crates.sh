#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/publish-crates.sh [cargo publish args...]

Runs tests before publishing the crates in dependency order to crates.io.
Pass through extra cargo publish arguments such as --dry-run.

Authentication:
- cargo publish uses CARGO_REGISTRY_TOKEN by default
- if CARGO_REGISTRY_TOKEN is unset and CRATES_IO_TOKEN is set, this script exports it
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(cd -- "$script_dir/.." && pwd)

cd "$repo_root"

if [[ -z "${CARGO_REGISTRY_TOKEN:-}" && -n "${CRATES_IO_TOKEN:-}" ]]; then
  export CARGO_REGISTRY_TOKEN="$CRATES_IO_TOKEN"
fi

cargo test

publish_args=()
if [[ "$#" -gt 0 ]]; then
  publish_args=("$@")
fi

packages=(
  e2ee-backend-adapter
  e2ee-backend-adapter-cli
)

for package in "${packages[@]}"; do
  cargo publish -p "$package" "${publish_args[@]}"
done