#!/usr/bin/env bash
set -euo pipefail

DESTDIR="${LINUXCARE_DESTDIR:-}"
SKIP_SYSTEMCTL="${LINUXCARE_SKIP_SYSTEMCTL:-0}"

if [[ -z "$DESTDIR" && ${EUID} -ne 0 ]]; then
  echo "Run as root: sudo ./scripts/uninstall-system.sh" >&2
  echo "For staged lifecycle tests, set LINUXCARE_DESTDIR to a temporary root." >&2
  exit 1
fi

root_path() {
  printf '%s%s' "$DESTDIR" "$1"
}

if [[ -z "$DESTDIR" && "$SKIP_SYSTEMCTL" != "1" ]]; then
  systemctl stop linuxcare-helper.service 2>/dev/null || true
fi

rm -f \
  "$(root_path /usr/bin/linuxcare)" \
  "$(root_path /usr/bin/linuxcare-maintenance)" \
  "$(root_path /usr/libexec/linuxcare-helper)" \
  "$(root_path /usr/share/applications/net.milmit.LinuxCare.desktop)" \
  "$(root_path /usr/share/metainfo/net.milmit.LinuxCare.metainfo.xml)" \
  "$(root_path /usr/share/polkit-1/actions/net.milmit.LinuxCare.policy)" \
  "$(root_path /usr/share/dbus-1/system.d/net.milmit.LinuxCare.Helper.conf)" \
  "$(root_path /usr/share/dbus-1/system-services/net.milmit.LinuxCare.Helper.service)" \
  "$(root_path /usr/lib/systemd/system/linuxcare-helper.service)" \
  "$(root_path /usr/share/icons/hicolor/scalable/apps/net.milmit.LinuxCare.svg)" \
  "$(root_path /usr/share/pixmaps/net.milmit.LinuxCare.svg)"
rm -rf "$(root_path /usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net)"

if [[ -z "$DESTDIR" && "$SKIP_SYSTEMCTL" != "1" ]]; then
  gtk-update-icon-cache -q /usr/share/icons/hicolor 2>/dev/null || true
  systemctl daemon-reload
fi

echo "LinuxCare system integration removed. User configuration/history was preserved."
