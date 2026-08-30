#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TAG="${1:-}"
VERSION="$(python3 - <<'PY'
import tomllib
with open('Cargo.toml','rb') as f:
    print(tomllib.load(f)['package']['version'])
PY
)"

if [[ -n "$TAG" ]]; then
  EXPECTED="v${VERSION}"
  if [[ "$TAG" != "$EXPECTED" ]]; then
    echo "Release tag mismatch: expected ${EXPECTED}, got ${TAG}" >&2
    exit 1
  fi
fi

python3 - "$VERSION" <<'PY'
from pathlib import Path
import json, sys, tomllib, xml.etree.ElementTree as ET
version=sys.argv[1]

checks = [
    Path('Cargo.lock'),
    Path('snapcraft.yaml'),
    Path('data/net.milmit.LinuxCare.metainfo.xml'),
]
for path in checks:
    text=path.read_text(encoding='utf-8')
    if version not in text:
        raise SystemExit(f'version mismatch: {path} does not contain {version}')

root=ET.parse('data/net.milmit.LinuxCare.metainfo.xml').getroot()
release_versions=[node.attrib.get('version') for node in root.findall('./releases/release')]
if not release_versions or release_versions[0] != version:
    raise SystemExit('AppStream newest release does not match Cargo.toml version')

notes=Path('RELEASE_NOTES.md').read_text(encoding='utf-8')
if version not in notes:
    raise SystemExit('RELEASE_NOTES.md does not mention the Cargo.toml version')

metadata=json.loads(Path('data/gnome-shell-extension/metadata.json').read_text())
if 'version' in metadata:
    raise SystemExit('GNOME extension metadata must not ship deprecated version key')
if not metadata.get('shell-version'):
    raise SystemExit('GNOME extension metadata has no shell-version entries')
print(f'release metadata is internally consistent for {version}')
PY

if [[ -n "${GITHUB_SHA:-}" ]]; then
  printf 'Git commit: %s\n' "$GITHUB_SHA"
fi
