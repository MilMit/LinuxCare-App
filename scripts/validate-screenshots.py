#!/usr/bin/env python3
"""Validate LinuxCare release screenshots without external Python packages."""
from __future__ import annotations
import argparse, json, pathlib, struct, sys

PNG_SIG = b"\x89PNG\r\n\x1a\n"

def png_size(path: pathlib.Path) -> tuple[int, int]:
    data = path.read_bytes()[:24]
    if len(data) < 24 or data[:8] != PNG_SIG or data[12:16] != b"IHDR":
        raise ValueError("not a PNG with a valid IHDR header")
    return struct.unpack(">II", data[16:24])

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("directory", nargs="?", default="assets/screenshots")
    args = ap.parse_args()
    directory = pathlib.Path(args.directory)
    manifest_path = directory / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    min_width = int(manifest.get("minimum_width", 1280))
    errors: list[str] = []
    for item in manifest["required"]:
        path = directory / item["file"]
        if not path.is_file():
            errors.append(f"missing required screenshot: {path}")
            continue
        try:
            width, height = png_size(path)
        except ValueError as exc:
            errors.append(f"{path}: {exc}")
            continue
        if width < min_width:
            errors.append(f"{path}: width {width}px is below required {min_width}px")
        # Exact 16:9 within rounding tolerance of one pixel.
        if abs(width * 9 - height * 16) > 16:
            errors.append(f"{path}: {width}x{height} is not 16:9")
        print(f"OK {path}: {width}x{height}")
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print("LinuxCare release screenshots passed structural validation.")
    print("Manual privacy/content review is still required before publication.")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
