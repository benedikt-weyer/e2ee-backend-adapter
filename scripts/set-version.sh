#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/set-version.sh <semver>

Update the workspace version and the publishable internal crate dependency version
used for crates.io releases.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "$#" -ne 1 ]]; then
  usage >&2
  exit 1
fi

version="$1"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Version '$version' is not a supported semantic version." >&2
  exit 1
fi

script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(cd -- "$script_dir/.." && pwd)

cd "$repo_root"

python3 - "$version" <<'PY'
from pathlib import Path
import re
import sys

version = sys.argv[1]
path = Path("Cargo.toml")
text = path.read_text()

patterns = [
    (r'(\[workspace\.package\][\s\S]*?\nversion = ")([0-9]+\.[0-9]+\.[0-9]+)(")', 1),
  (r'(e2ee-backend-adapter = \{ version = ")([0-9]+\.[0-9]+\.[0-9]+)(", path = "crates/adapter-core" \})', 1),
]

for pattern, expected in patterns:
    text, count = re.subn(pattern, rf'\g<1>{version}\g<3>', text, count=1)
    if count != expected:
        raise SystemExit(f"Failed to update version pattern: {pattern}")

path.write_text(text)
PY

echo "Updated workspace version to $version"