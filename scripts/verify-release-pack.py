#!/usr/bin/env python3
from __future__ import annotations
import argparse, hashlib, json, pathlib, sys


def digest(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('directory', nargs='?', default='target/release-pack')
    args = ap.parse_args()
    d = pathlib.Path(args.directory)
    manifest = json.loads((d / 'RELEASE-MANIFEST.json').read_text(encoding='utf-8'))
    errors = []
    for item in manifest['artifacts']:
        p = d / item['name']
        if not p.is_file():
            errors.append(f"missing: {item['name']}")
            continue
        if p.stat().st_size != item['size_bytes']:
            errors.append(f"size mismatch: {item['name']}")
        got = digest(p)
        if got != item['sha256']:
            errors.append(f"sha256 mismatch: {item['name']}")
    checksum_file = d / 'SHA256SUMS.txt'
    if checksum_file.is_file():
        for line in checksum_file.read_text(encoding='utf-8').splitlines():
            if not line.strip():
                continue
            expected, name = line.split(None, 1)
            name = name.lstrip('* ')
            p = d / name
            if not p.is_file():
                errors.append(f"checksum list references missing file: {name}")
            elif digest(p) != expected:
                errors.append(f"SHA256SUMS mismatch: {name}")
    if errors:
        for e in errors:
            print(f"ERROR: {e}", file=sys.stderr)
        return 1
    print(f"LinuxCare release pack verified: {len(manifest['artifacts'])} manifest artifacts")
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
