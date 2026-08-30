#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="$(python3 - <<'PY'
import tomllib
with open('Cargo.toml','rb') as f: print(tomllib.load(f)['package']['version'])
PY
)"
ARCH="${DEB_BUILD_ARCH:-$(dpkg --print-architecture)}"
EPOCH="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || stat -c %Y Cargo.lock)}"
export SOURCE_DATE_EPOCH="$EPOCH"
OUT="target/release-pack"
rm -rf "$OUT"
mkdir -p "$OUT"

DEB="$(find target/deb -maxdepth 1 -type f -name "linuxcare_${VERSION}_*.deb" -print | sort | tail -n1)"
[[ -n "$DEB" && -f "$DEB" ]] || { echo "Missing verified Debian package for ${VERSION}; run ./scripts/build-deb.sh first" >&2; exit 1; }
cp "$DEB" "$OUT/"

for bin in linuxcare linuxcare-helper linuxcare-maintenance; do
  [[ -x "target/release/$bin" ]] || { echo "Missing target/release/$bin; run cargo build --release --workspace" >&2; exit 1; }
done

BIN_DIR="linuxcare-${VERSION}-$(uname -m)-linux-gnu"
TMP_BIN="$(mktemp -d)"
trap 'rm -rf "$TMP_BIN"' EXIT
mkdir -p "$TMP_BIN/$BIN_DIR"
install -m755 target/release/linuxcare "$TMP_BIN/$BIN_DIR/linuxcare"
install -m755 target/release/linuxcare-helper "$TMP_BIN/$BIN_DIR/linuxcare-helper"
install -m755 target/release/linuxcare-maintenance "$TMP_BIN/$BIN_DIR/linuxcare-maintenance"
cp -a data "$TMP_BIN/$BIN_DIR/data"
cp README.md CHANGELOG.md SECURITY.md VERIFICATION.md BETA_TESTING.md RELEASE_CHECKLIST.md RELEASE_NOTES.md VERIFY_RELEASE.md BETA_RELEASE_STATUS.md "$TMP_BIN/$BIN_DIR/"
find "$TMP_BIN/$BIN_DIR" -print0 | xargs -0 touch -h -d "@$EPOCH"
tar --sort=name --mtime="@$EPOCH" --owner=0 --group=0 --numeric-owner -C "$TMP_BIN" -cf - "$BIN_DIR" | gzip -n > "$OUT/${BIN_DIR}.tar.gz"

SOURCE_NAME="linuxcare-${VERSION}-source"
tar --sort=name --mtime="@$EPOCH" --owner=0 --group=0 --numeric-owner \
  --exclude='.git' --exclude='./target' --exclude='./target/**' \
  --transform="s|^\./|${SOURCE_NAME}/|" -C "$ROOT_DIR" -cf - . | gzip -n > "$OUT/${SOURCE_NAME}.tar.gz"

python3 scripts/generate-sbom.py --output "$OUT/SBOM.spdx.json"
cp RELEASE_NOTES.md VERIFY_RELEASE.md BETA_RELEASE_STATUS.md "$OUT/"

{
  echo "LinuxCare ${VERSION} release build"
  echo "Architecture: ${ARCH}"
  echo "SOURCE_DATE_EPOCH: ${EPOCH}"
  echo "Git commit: $(git rev-parse HEAD 2>/dev/null || echo unavailable)"
  echo "rustc: $(rustc --version)"
  echo "cargo: $(cargo --version)"
  echo "Host: $(uname -srmo)"
} > "$OUT/BUILD_INFO.txt"

touch -d "@$EPOCH" "$OUT"/*
python3 scripts/generate-release-manifest.py --directory "$OUT"
(
  cd "$OUT"
  checksum_tmp="$(mktemp "${TMPDIR:-/tmp}/linuxcare-sha256.XXXXXX")"
  trap 'rm -f "$checksum_tmp"' EXIT
  find . -maxdepth 1 -type f ! -name SHA256SUMS.txt -printf '%f\0' \
    | LC_ALL=C sort -z \
    | xargs -0 -r sha256sum > "$checksum_tmp"
  mv "$checksum_tmp" SHA256SUMS.txt
  trap - EXIT
)

echo "Release assets prepared in $OUT"
ls -lh "$OUT"
