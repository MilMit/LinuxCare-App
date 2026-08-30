#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DEB="${1:-}"
if [[ -z "$DEB" ]]; then
  DEB="$(find target/deb -maxdepth 1 -type f -name 'linuxcare_*.deb' -print | sort | tail -n1)"
fi
[[ -n "$DEB" && -f "$DEB" ]] || { echo "No LinuxCare .deb found" >&2; exit 1; }

VERSION="$(python3 - <<'PY'
import tomllib
with open('Cargo.toml','rb') as f: print(tomllib.load(f)['package']['version'])
PY
)"

pkg="$(dpkg-deb -f "$DEB" Package)"
ver="$(dpkg-deb -f "$DEB" Version)"
depends="$(dpkg-deb -f "$DEB" Depends)"
[[ "$pkg" == "linuxcare" ]] || { echo "Unexpected package name: $pkg" >&2; exit 1; }
[[ "$ver" == "$VERSION" ]] || { echo "Package version mismatch: $ver != $VERSION" >&2; exit 1; }
grep -Eq '(^|, )[[:space:]]*libc6([[:space:](,]|$)' <<<"$depends" || {
  echo "Package must declare a direct libc6 dependency" >&2
  exit 1
}
grep -Eq '(^|, )[[:space:]]*polkitd([[:space:](,]|$)' <<<"$depends" || {
  echo "Package must depend on polkitd" >&2
  exit 1
}
if grep -Eq '(^|, )[[:space:]]*policykit-1([[:space:](,]|$)' <<<"$depends"; then
  echo "Package must not depend on obsolete policykit-1" >&2
  exit 1
fi

listing="$(dpkg-deb -c "$DEB")"
for required in \
  './usr/bin/linuxcare' \
  './usr/bin/linuxcare-maintenance' \
  './usr/libexec/linuxcare-helper' \
  './usr/share/applications/net.milmit.LinuxCare.desktop' \
  './usr/share/metainfo/net.milmit.LinuxCare.metainfo.xml' \
  './usr/share/polkit-1/actions/net.milmit.LinuxCare.policy' \
  './usr/share/dbus-1/system.d/net.milmit.LinuxCare.Helper.conf' \
  './usr/share/dbus-1/system-services/net.milmit.LinuxCare.Helper.service' \
  './usr/lib/systemd/system/linuxcare-helper.service' \
  './usr/share/icons/hicolor/scalable/apps/net.milmit.LinuxCare.svg' \
  './usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/metadata.json' \
  './usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/extension.js' \
  './usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/stylesheet.css' \
  './usr/share/doc/linuxcare/changelog.Debian.gz' \
  './usr/share/doc/linuxcare/copyright' \
  './usr/share/man/man1/linuxcare.1.gz' \
  './usr/share/man/man1/linuxcare-maintenance.1.gz'; do
  grep -Fq "$required" <<<"$listing" || { echo "Package missing: $required" >&2; exit 1; }
done

if grep -Eq '^[-d][rwx-]*[sS][rwx-]*[[:space:]]' <<<"$listing"; then
  echo "Unexpected setuid/setgid bit in package payload" >&2
  exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
dpkg-deb -e "$DEB" "$TMP/DEBIAN"
for script in postinst prerm postrm; do
  [[ ! -f "$TMP/DEBIAN/$script" ]] || sh -n "$TMP/DEBIAN/$script"
  if [[ -f "$TMP/DEBIAN/$script" ]] && grep -Eiq '/home/|\.config/linuxcare|\.local/state/linuxcare' "$TMP/DEBIAN/$script"; then
    echo "Maintainer script $script must not delete or mutate per-user LinuxCare state" >&2
    exit 1
  fi
done

if command -v lintian >/dev/null 2>&1; then
  lintian --display-info --fail-on error "$DEB"
else
  echo "NOTE: lintian unavailable; package policy lint skipped" >&2
fi

echo "Debian package smoke validation passed: $DEB"
