#!/usr/bin/env bash
set -euo pipefail

umask 022

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="$(python3 - <<'PYVER'
import tomllib
with open('Cargo.toml', 'rb') as f:
    print(tomllib.load(f)['package']['version'])
PYVER
)"
ARCH="${DEB_BUILD_ARCH:-$(dpkg --print-architecture)}"
SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || date +%s)}"
export SOURCE_DATE_EPOCH
PKG_NAME="linuxcare_${VERSION}_${ARCH}"
BUILD_DIR="${ROOT_DIR}/target/deb/${PKG_NAME}"
CHANGELOG_DATE="$(date -u -d "@${SOURCE_DATE_EPOCH}" -R)"

echo "Building release binaries..."
cargo build --release --workspace

echo "Preparing package structure at ${BUILD_DIR}..."
rm -rf "${BUILD_DIR}"
mkdir -p "${BUILD_DIR}/DEBIAN"
mkdir -p "${BUILD_DIR}/usr/bin"
mkdir -p "${BUILD_DIR}/usr/libexec"
mkdir -p "${BUILD_DIR}/usr/share/applications"
mkdir -p "${BUILD_DIR}/usr/share/metainfo"
mkdir -p "${BUILD_DIR}/usr/share/polkit-1/actions"
mkdir -p "${BUILD_DIR}/usr/share/dbus-1/system.d"
mkdir -p "${BUILD_DIR}/usr/share/dbus-1/system-services"
mkdir -p "${BUILD_DIR}/usr/lib/systemd/system"
mkdir -p "${BUILD_DIR}/usr/share/icons/hicolor/scalable/apps"
mkdir -p "${BUILD_DIR}/usr/share/pixmaps"
mkdir -p "${BUILD_DIR}/usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net"
mkdir -p "${BUILD_DIR}/usr/share/doc/linuxcare"
mkdir -p "${BUILD_DIR}/usr/share/man/man1"

# Parent directories may carry the setgid bit (for example 2775).
# Debian control/package directories must not inherit those permissions.
find "${BUILD_DIR}" -type d -exec chmod 00755 {} +

echo "Copying binaries and system data files..."
install -Dm755 target/release/linuxcare "${BUILD_DIR}/usr/bin/linuxcare"
install -Dm755 target/release/linuxcare-maintenance "${BUILD_DIR}/usr/bin/linuxcare-maintenance"
install -Dm755 target/release/linuxcare-helper "${BUILD_DIR}/usr/libexec/linuxcare-helper"
install -Dm644 data/net.milmit.LinuxCare.desktop "${BUILD_DIR}/usr/share/applications/net.milmit.LinuxCare.desktop"
install -Dm644 data/net.milmit.LinuxCare.metainfo.xml "${BUILD_DIR}/usr/share/metainfo/net.milmit.LinuxCare.metainfo.xml"
install -Dm644 data/polkit-1/actions/net.milmit.LinuxCare.policy "${BUILD_DIR}/usr/share/polkit-1/actions/net.milmit.LinuxCare.policy"
install -Dm644 data/dbus-1/system.d/net.milmit.LinuxCare.Helper.conf "${BUILD_DIR}/usr/share/dbus-1/system.d/net.milmit.LinuxCare.Helper.conf"
install -Dm644 data/dbus-1/system-services/net.milmit.LinuxCare.Helper.service "${BUILD_DIR}/usr/share/dbus-1/system-services/net.milmit.LinuxCare.Helper.service"
install -Dm644 data/systemd/linuxcare-helper.service "${BUILD_DIR}/usr/lib/systemd/system/linuxcare-helper.service"
install -Dm644 data/icons/hicolor/scalable/apps/net.milmit.LinuxCare.svg "${BUILD_DIR}/usr/share/icons/hicolor/scalable/apps/net.milmit.LinuxCare.svg"
install -Dm644 data/icons/hicolor/scalable/apps/net.milmit.LinuxCare.svg "${BUILD_DIR}/usr/share/pixmaps/net.milmit.LinuxCare.svg"
install -Dm644 data/gnome-shell-extension/metadata.json "${BUILD_DIR}/usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/metadata.json"
install -Dm644 data/gnome-shell-extension/extension.js "${BUILD_DIR}/usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/extension.js"
install -Dm644 data/gnome-shell-extension/stylesheet.css "${BUILD_DIR}/usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/stylesheet.css"
install -Dm644 data/debian/copyright "${BUILD_DIR}/usr/share/doc/linuxcare/copyright"
install -Dm644 data/man/linuxcare.1 "${BUILD_DIR}/usr/share/man/man1/linuxcare.1"
install -Dm644 data/man/linuxcare-maintenance.1 "${BUILD_DIR}/usr/share/man/man1/linuxcare-maintenance.1"
gzip -n -9 "${BUILD_DIR}/usr/share/man/man1/linuxcare.1"
gzip -n -9 "${BUILD_DIR}/usr/share/man/man1/linuxcare-maintenance.1"

cat <<EOF_CHANGELOG > "${BUILD_DIR}/usr/share/doc/linuxcare/changelog.Debian"
linuxcare (${VERSION}) unstable; urgency=medium

  * Beta package reliability and Debian policy hardening.

 -- MilMit <info@milmit.net>  ${CHANGELOG_DATE}
EOF_CHANGELOG
gzip -n -9 "${BUILD_DIR}/usr/share/doc/linuxcare/changelog.Debian"

cat <<EOF_CONTROL > "${BUILD_DIR}/DEBIAN/control"
Package: linuxcare
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Depends: libc6, libgtk-4-1, libadwaita-1-0, dbus, polkitd, systemd
Recommends: smartmontools, libnotify-bin, iproute2
Suggests: gnome-shell, bpftool
Maintainer: MilMit <info@milmit.net>
Homepage: https://milmit.net
Description: Safe Linux maintenance and storage-management suite
 LinuxCare is a native GTK4/Libadwaita application for Ubuntu and GNOME.
 It provides safe cleanup, package diagnostics, battery health, boot and
 storage intelligence, process anomaly diagnostics, Deep Network socket
 ownership and eBPF readiness, GNOME integration, authenticated read-only
 SMART/NVMe health, hardware inventory, and security auditing with strict
 privilege separation.
EOF_CONTROL

cat <<'EOF_POSTINST' > "${BUILD_DIR}/DEBIAN/postinst"
#!/bin/sh
set -e
if [ "$1" = "configure" ]; then
    if command -v systemctl >/dev/null 2>&1; then
        systemctl daemon-reload || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q /usr/share/icons/hicolor 2>/dev/null || true
    fi
fi
exit 0
EOF_POSTINST
chmod 755 "${BUILD_DIR}/DEBIAN/postinst"

cat <<'EOF_PRERM' > "${BUILD_DIR}/DEBIAN/prerm"
#!/bin/sh
set -e
if [ "$1" = "remove" ] || [ "$1" = "deconfigure" ]; then
    if command -v deb-systemd-invoke >/dev/null 2>&1; then
        deb-systemd-invoke stop linuxcare-helper.service >/dev/null 2>&1 || true
    fi
fi
exit 0
EOF_PRERM
chmod 755 "${BUILD_DIR}/DEBIAN/prerm"

cat <<'EOF_POSTRM' > "${BUILD_DIR}/DEBIAN/postrm"
#!/bin/sh
set -e
if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
    if command -v systemctl >/dev/null 2>&1; then
        systemctl daemon-reload || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q /usr/share/icons/hicolor 2>/dev/null || true
    fi
fi
exit 0
EOF_POSTRM
chmod 755 "${BUILD_DIR}/DEBIAN/postrm"

echo "Generating deterministic package metadata..."
(
  cd "${BUILD_DIR}"
  find usr -type f -print0 | sort -z | xargs -0 md5sum > DEBIAN/md5sums
)
chmod 644 "${BUILD_DIR}/DEBIAN/md5sums"
find "${BUILD_DIR}" -print0 | xargs -0 touch -h -d "@${SOURCE_DATE_EPOCH}"

# Normalize final staging permissions immediately before dpkg-deb.
# Some parent directories may carry setgid and propagate it to DEBIAN/.
find "${BUILD_DIR}" -type d -exec chmod 00755 {} +
find "${BUILD_DIR}" -type f -exec chmod u-s,g-s {} +
chmod 00755 "${BUILD_DIR}/DEBIAN"

echo "Building Debian package..."
DEBIAN_MODE="$(stat -c '%a' "${BUILD_DIR}/DEBIAN")"
echo "DEBIAN directory mode before dpkg-deb: ${DEBIAN_MODE}"
if [ "${DEBIAN_MODE}" != "755" ]; then
    echo "ERROR: DEBIAN directory must be mode 755, got ${DEBIAN_MODE}" >&2
    exit 1
fi

dpkg-deb --root-owner-group --build "${BUILD_DIR}" "${ROOT_DIR}/target/deb/${PKG_NAME}.deb"

echo "Package generated successfully at target/deb/${PKG_NAME}.deb"
