#!/usr/bin/env bash
set -euo pipefail

DESTDIR="${LINUXCARE_DESTDIR:-}"
SKIP_SYSTEMCTL="${LINUXCARE_SKIP_SYSTEMCTL:-0}"

if [[ -z "$DESTDIR" && ${EUID} -ne 0 ]]; then
  echo "Run this installer as root after building: sudo ./scripts/install-system.sh" >&2
  echo "For staged lifecycle tests, set LINUXCARE_DESTDIR to a temporary root." >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

[[ -x target/release/linuxcare ]] || { echo "Missing target/release/linuxcare. Run: cargo build --release --bins" >&2; exit 1; }
[[ -x target/release/linuxcare-helper ]] || { echo "Missing target/release/linuxcare-helper. Run: cargo build --release --bins" >&2; exit 1; }
[[ -x target/release/linuxcare-maintenance ]] || { echo "Missing target/release/linuxcare-maintenance. Run: cargo build --release --bins" >&2; exit 1; }

root_path() {
  printf '%s%s' "$DESTDIR" "$1"
}

install -Dm755 target/release/linuxcare "$(root_path /usr/bin/linuxcare)"
install -Dm755 target/release/linuxcare-maintenance "$(root_path /usr/bin/linuxcare-maintenance)"
install -Dm755 target/release/linuxcare-helper "$(root_path /usr/libexec/linuxcare-helper)"
install -Dm644 data/net.milmit.LinuxCare.desktop "$(root_path /usr/share/applications/net.milmit.LinuxCare.desktop)"
install -Dm644 data/net.milmit.LinuxCare.metainfo.xml "$(root_path /usr/share/metainfo/net.milmit.LinuxCare.metainfo.xml)"
install -Dm644 data/polkit-1/actions/net.milmit.LinuxCare.policy "$(root_path /usr/share/polkit-1/actions/net.milmit.LinuxCare.policy)"
install -Dm644 data/dbus-1/system.d/net.milmit.LinuxCare.Helper.conf "$(root_path /usr/share/dbus-1/system.d/net.milmit.LinuxCare.Helper.conf)"
install -Dm644 data/dbus-1/system-services/net.milmit.LinuxCare.Helper.service "$(root_path /usr/share/dbus-1/system-services/net.milmit.LinuxCare.Helper.service)"
install -Dm644 data/systemd/linuxcare-helper.service "$(root_path /usr/lib/systemd/system/linuxcare-helper.service)"
install -Dm644 data/icons/hicolor/scalable/apps/net.milmit.LinuxCare.svg "$(root_path /usr/share/icons/hicolor/scalable/apps/net.milmit.LinuxCare.svg)"
install -Dm644 data/icons/hicolor/scalable/apps/net.milmit.LinuxCare.svg "$(root_path /usr/share/pixmaps/net.milmit.LinuxCare.svg)"

# Replace only LinuxCare's own extension directory so stale files from manual upgrades do not survive.
EXT_DIR="$(root_path /usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net)"
rm -rf "$EXT_DIR"
mkdir -p "$EXT_DIR"
install -Dm644 data/gnome-shell-extension/metadata.json "$EXT_DIR/metadata.json"
install -Dm644 data/gnome-shell-extension/extension.js "$EXT_DIR/extension.js"
install -Dm644 data/gnome-shell-extension/stylesheet.css "$EXT_DIR/stylesheet.css"

if [[ -z "$DESTDIR" && "$SKIP_SYSTEMCTL" != "1" ]]; then
  systemctl daemon-reload
  gtk-update-icon-cache -q /usr/share/icons/hicolor 2>/dev/null || true
fi

echo "LinuxCare installed. The privileged helper is D-Bus activated and is not enabled as a permanent background service."
