#!/usr/bin/env python3
"""Generate a deterministic machine-readable manifest for LinuxCare release assets."""
from __future__ import annotations
import argparse, hashlib, json, mimetypes, os, pathlib, subprocess, tomllib
from datetime import datetime, timezone


def source_epoch(root: pathlib.Path) -> int:
    raw = os.environ.get("SOURCE_DATE_EPOCH")
    if raw:
        try:
            return max(0, int(raw))
        except ValueError:
            pass
    try:
        return int(subprocess.check_output(
            ["git", "log", "-1", "--format=%ct"], cwd=root, text=True,
            stderr=subprocess.DEVNULL).strip())
    except (OSError, subprocess.CalledProcessError, ValueError):
        return int((root / "Cargo.lock").stat().st_mtime)


def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def purpose(name: str) -> str:
    if name.endswith(".deb"):
        return "Debian package"
    if name.endswith("-linux-gnu.tar.gz"):
        return "Portable binary release archive"
    if name.endswith("-source.tar.gz"):
        return "Deterministic source archive"
    if name == "SBOM.spdx.json":
        return "SPDX 2.3 Cargo dependency SBOM"
    if name.startswith("RELEASE_NOTES"):
        return "Human-readable release notes"
    if name == "BUILD_INFO.txt":
        return "Release build environment summary"
    return "Release artifact"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--directory", default="target/release-pack")
    ap.add_argument("--output", default=None)
    args = ap.parse_args()
    root = pathlib.Path(__file__).resolve().parents[1]
    directory = root / args.directory
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    version = manifest["version"]
    epoch = source_epoch(root)
    generated = datetime.fromtimestamp(epoch, timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    output = pathlib.Path(args.output) if args.output else directory / "RELEASE-MANIFEST.json"
    if not output.is_absolute():
        output = root / output
    artifacts = []
    for path in sorted(directory.iterdir()):
        if not path.is_file() or path.name in {output.name, "SHA256SUMS.txt"}:
            continue
        mime = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
        artifacts.append({
            "name": path.name,
            "purpose": purpose(path.name),
            "size_bytes": path.stat().st_size,
            "sha256": sha256(path),
            "media_type": mime,
        })
    doc = {
        "schema": "net.milmit.LinuxCare.release-manifest.v1",
        "product": "LinuxCare",
        "version": version,
        "channel": "beta" if "beta" in version else "prerelease",
        "generated_at": generated,
        "artifacts": artifacts,
    }
    output.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(output)
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
