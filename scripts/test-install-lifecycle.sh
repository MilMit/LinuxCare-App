#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

for bin in linuxcare linuxcare-helper linuxcare-maintenance; do
  [[ -x "target/release/$bin" ]] || { echo "Missing target/release/$bin; build release binaries first" >&2; exit 1; }
done

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
STAGE="$TMP/root"
mkdir -p "$STAGE"

export LINUXCARE_DESTDIR="$STAGE"
export LINUXCARE_SKIP_SYSTEMCTL=1

./scripts/install-system.sh

for path in \
  /usr/bin/linuxcare \
  /usr/bin/linuxcare-maintenance \
  /usr/libexec/linuxcare-helper \
  /usr/share/applications/net.milmit.LinuxCare.desktop \
  /usr/share/metainfo/net.milmit.LinuxCare.metainfo.xml \
  /usr/share/polkit-1/actions/net.milmit.LinuxCare.policy \
  /usr/share/dbus-1/system-services/net.milmit.LinuxCare.Helper.service \
  /usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/extension.js; do
  [[ -e "$STAGE$path" ]] || { echo "Staged install missing $path" >&2; exit 1; }
done

# Simulate a manual upgrade from an older extension containing a stale file.
touch "$STAGE/usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/obsolete-beta-file.js"
./scripts/install-system.sh
[[ ! -e "$STAGE/usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net/obsolete-beta-file.js" ]] \
  || { echo "Manual upgrade left stale extension payload behind" >&2; exit 1; }

# Ensure privileged payload never gains setuid/setgid bits.
if find "$STAGE" -type f \( -perm -4000 -o -perm -2000 \) -print -quit | grep -q .; then
  echo "Staged install contains SUID/SGID payload" >&2
  exit 1
fi

# User data is intentionally outside the staged system root and must survive uninstall.
USER_HOME="$TMP/user-home"
mkdir -p "$USER_HOME/.config/linuxcare" "$USER_HOME/.local/state/linuxcare"
printf '%s\n' '{"beta_test":true}' > "$USER_HOME/.config/linuxcare/config.json"
printf '%s\n' 'keep-me' > "$USER_HOME/.local/state/linuxcare/lifecycle-marker"

./scripts/uninstall-system.sh
[[ ! -e "$STAGE/usr/bin/linuxcare" ]] || { echo "Uninstall left main binary" >&2; exit 1; }
[[ ! -e "$STAGE/usr/libexec/linuxcare-helper" ]] || { echo "Uninstall left helper" >&2; exit 1; }
[[ ! -e "$STAGE/usr/share/gnome-shell/extensions/linuxcare-vitals@milmit.net" ]] || { echo "Uninstall left extension directory" >&2; exit 1; }
[[ -f "$USER_HOME/.config/linuxcare/config.json" ]] || { echo "Uninstall unexpectedly removed user config" >&2; exit 1; }
[[ -f "$USER_HOME/.local/state/linuxcare/lifecycle-marker" ]] || { echo "Uninstall unexpectedly removed user state" >&2; exit 1; }

# Idempotent second uninstall should remain safe.
./scripts/uninstall-system.sh

echo "LinuxCare staged install/upgrade/uninstall lifecycle test passed."
