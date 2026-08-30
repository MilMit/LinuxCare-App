#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -ne 0 ]]; then
  echo "Run as root on an ephemeral CI/test host: sudo ./scripts/smoke-deb-install.sh [package.deb]" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
DEB="${1:-$(find target/deb -maxdepth 1 -type f -name 'linuxcare_*.deb' -print | sort | tail -n1)}"
[[ -n "$DEB" && -f "$DEB" ]] || { echo "No LinuxCare .deb found" >&2; exit 1; }

# apt-get must receive an explicit local-file path, not a package-like relative name.
DEB="$(realpath "$DEB")"
echo "Testing Debian package: $DEB"

VERSION="$(python3 - <<'PY'
import tomllib
with open('Cargo.toml','rb') as f: print(tomllib.load(f)['package']['version'])
PY
)"

cleanup() {
  apt-get remove -y linuxcare >/dev/null 2>&1 || true
}
trap cleanup EXIT


# Install a tiny legacy package first to exercise real dpkg upgrade cleanup of obsolete payload files.
LEGACY_ROOT="$(mktemp -d)"
mkdir -p "$LEGACY_ROOT/DEBIAN" \
  "$LEGACY_ROOT/usr/bin" \
  "$LEGACY_ROOT/usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net"
LEGACY_ARCH="$(dpkg --print-architecture)"
cat > "$LEGACY_ROOT/DEBIAN/control" <<CONTROL
Package: linuxcare
Version: 0.6.0~upgrade-test
Section: utils
Priority: optional
Architecture: ${LEGACY_ARCH}
Maintainer: LinuxCare CI <noreply@example.invalid>
Description: LinuxCare legacy lifecycle smoke fixture
CONTROL
printf '#!/bin/sh\necho legacy-linuxcare\n' > "$LEGACY_ROOT/usr/bin/linuxcare"
chmod 755 "$LEGACY_ROOT/usr/bin/linuxcare"
printf '%s\n' '// obsolete upgrade fixture' > "$LEGACY_ROOT/usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/obsolete-beta-file.js"
LEGACY_DEB="$LEGACY_ROOT.deb"
dpkg-deb --root-owner-group --build "$LEGACY_ROOT" "$LEGACY_DEB" >/dev/null
dpkg -i "$LEGACY_DEB" >/dev/null
[[ -e /usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/obsolete-beta-file.js ]] \
  || { echo "Legacy upgrade fixture did not install" >&2; exit 1; }

apt-get install -y "$DEB"
[[ ! -e /usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/obsolete-beta-file.js ]] \
  || { echo "Debian upgrade left obsolete extension payload behind" >&2; exit 1; }
installed="$(linuxcare --version)"
[[ "$installed" == "LinuxCare $VERSION" ]] || { echo "Installed CLI version mismatch: $installed" >&2; exit 1; }

for path in \
  /usr/bin/linuxcare \
  /usr/bin/linuxcare-maintenance \
  /usr/libexec/linuxcare-helper \
  /usr/share/polkit-1/actions/net.milmit.LinuxCare.policy \
  /usr/share/dbus-1/system-services/net.milmit.LinuxCare.Helper.service; do
  [[ -e "$path" ]] || { echo "Installed package missing $path" >&2; exit 1; }
done

if find /usr/bin/linuxcare /usr/bin/linuxcare-maintenance /usr/libexec/linuxcare-helper \
  -type f \( -perm -4000 -o -perm -2000 \) -print -quit | grep -q .; then
  echo "Installed LinuxCare executable has SUID/SGID bits" >&2
  exit 1
fi

apt-get remove -y linuxcare
trap - EXIT
for path in /usr/bin/linuxcare /usr/bin/linuxcare-maintenance /usr/libexec/linuxcare-helper; do
  [[ ! -e "$path" ]] || { echo "Package removal left $path" >&2; exit 1; }
done

echo "LinuxCare real Debian install/remove smoke test passed."
