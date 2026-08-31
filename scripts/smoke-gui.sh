#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

[[ -x target/debug/linuxcare ]] || {
  echo "Missing target/debug/linuxcare; run cargo build first" >&2
  exit 1
}

for cmd in xvfb-run setsid dbus-run-session gsettings; do
  command -v "$cmd" >/dev/null 2>&1 || {
    echo "Missing GUI smoke dependency: $cmd" >&2
    exit 2
  }
done

# Libadwaita's startup settings backend requires a real GSettings schema source.
# Minimal CI images do not necessarily install the desktop schemas alongside
# libadwaita itself, so fail here with a precise dependency error rather than a
# later g_settings_schema_source_lookup(source == NULL) critical.
if ! gsettings list-schemas | grep -Fxq 'org.gnome.desktop.interface'; then
  echo "Missing runtime GSettings schema: org.gnome.desktop.interface" >&2
  echo "Install gsettings-desktop-schemas before running the GUI smoke test." >&2
  exit 2
fi

TMP="$(mktemp -d)"
pid=""
cleanup() {
  # LinuxCare can start bounded diagnostic subprocesses during startup (for
  # example podman/systemd probes). The application may exit before one of
  # those descendants has finished, so always reap the isolated process group
  # before removing its temporary HOME.
  if [[ -n "${pid:-}" ]]; then
    kill -TERM -- "-$pid" 2>/dev/null || true
    sleep 0.1
    kill -KILL -- "-$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi

  # Temporary rootless-container storage can contain overlay work directories
  # that are not immediately removable on every runner. Cleanup is best-effort
  # and must not turn an otherwise successful GUI startup assertion into a
  # false-negative CI result.
  if ! rm -rf "$TMP" 2>/dev/null; then
    echo "GUI smoke: warning: temporary test directory could not be fully removed: $TMP" >&2
  fi
}
trap cleanup EXIT
mkdir -p "$TMP/home/.config" "$TMP/home/.local/state"
chmod 700 "$TMP/home" "$TMP/home/.config" "$TMP/home/.local" "$TMP/home/.local/state" 2>/dev/null || true

echo "GUI smoke: starting isolated 1024x600 X11 + D-Bus session..."

# Run a private session bus as well as Xvfb. A libadwaita desktop application is
# expected to run in a user session, and this keeps the smoke test independent
# of whatever session services happen to exist on a hosted runner.
set +e
setsid env \
  HOME="$TMP/home" \
  XDG_CONFIG_HOME="$TMP/home/.config" \
  XDG_STATE_HOME="$TMP/home/.local/state" \
  GDK_BACKEND=x11 \
  GSETTINGS_BACKEND=memory \
  G_DEBUG=fatal-criticals \
  LINUXCARE_GUI_SMOKE=1 \
  dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1024x600x24 -nolisten tcp' \
  ./target/debug/linuxcare \
  >"$TMP/gui.out" 2>"$TMP/gui.err" &
pid=$!
set -e

status=0
timed_out=0
for _ in $(seq 1 120); do
  if ! kill -0 "$pid" 2>/dev/null; then
    set +e
    wait "$pid"
    status=$?
    set -e
    break
  fi
  sleep 0.1
done

if kill -0 "$pid" 2>/dev/null; then
  timed_out=1
  status=124
  echo "GUI smoke: startup did not exit within 12s; terminating process group..." >&2
  kill -TERM -- "-$pid" 2>/dev/null || true
  sleep 1
  kill -KILL -- "-$pid" 2>/dev/null || true
  set +e
  wait "$pid" 2>/dev/null
  set -e
fi

if [[ "$timed_out" -eq 1 || "$status" -ne 0 ]]; then
  echo "LinuxCare GUI startup smoke failed with status $status" >&2
  echo "--- stdout ---" >&2
  cat "$TMP/gui.out" >&2 || true
  echo "--- stderr ---" >&2
  cat "$TMP/gui.err" >&2 || true
  exit 1
fi

if grep -Eiq '(^|[^a-z])(panic|segmentation fault|gtk-critical|glib-critical|libadwaita-critical)' "$TMP/gui.err"; then
  echo "LinuxCare GUI emitted a fatal/critical diagnostic during startup" >&2
  cat "$TMP/gui.err" >&2
  exit 1
fi

# Reap any diagnostic descendants that outlived the main application before
# the EXIT trap attempts to remove its temporary HOME.
kill -TERM -- "-$pid" 2>/dev/null || true
sleep 0.1
kill -KILL -- "-$pid" 2>/dev/null || true

echo "LinuxCare GUI startup smoke passed at a 1024x600 virtual display."
