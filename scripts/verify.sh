#!/usr/bin/env bash
set -euo pipefail

fail=0

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    fail=1
  fi
}

echo "== LinuxCare static/security verification =="

# Security regression checks: generic shell/root execution APIs must never appear.
if grep -RInE 'execute_command|run_shell|run_as_root|sudo[[:space:]]+rm|sh[[:space:]]+-c|bash[[:space:]]+-c|pkexec' src data/gnome-shell-extension; then
  echo "Forbidden execution pattern detected" >&2
  exit 1
fi

# Scheduled maintenance must remain strictly unprivileged.
if grep -nE 'UbuntuSystemProvider|PrivilegedAction|PrivilegedProvider|linuxcare-helper' src/automation_engine.rs src/bin/linuxcare-maintenance.rs; then
  echo "Scheduled maintenance privilege-boundary regression detected" >&2
  exit 1
fi

require_command python3
require_command bash

if [ "$fail" -ne 0 ]; then
  exit 2
fi

python3 - <<'PY'
import xml.etree.ElementTree as ET
for path in [
    'data/net.milmit.LinuxCare.metainfo.xml',
    'data/polkit-1/actions/net.milmit.LinuxCare.policy',
    'data/dbus-1/system.d/net.milmit.LinuxCare.Helper.conf',
]:
    ET.parse(path)
print('XML metadata/policies parse successfully')
PY

bash -n scripts/*.sh
./scripts/security-regression.sh

if command -v shellcheck >/dev/null 2>&1; then
  shellcheck scripts/*.sh
  echo "ShellCheck passed"
else
  echo "NOTE: shellcheck is not installed; shell lint skipped" >&2
fi

if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate data/net.milmit.LinuxCare.desktop
  echo "Desktop entry validation passed"
else
  echo "NOTE: desktop-file-validate is not installed; desktop metadata lint skipped" >&2
fi

if command -v appstreamcli >/dev/null 2>&1; then
  appstreamcli validate --no-net data/net.milmit.LinuxCare.metainfo.xml
  echo "AppStream metadata validation passed"
else
  echo "NOTE: appstreamcli is not installed; AppStream policy lint skipped" >&2
fi

./scripts/check-release.sh

python3 - <<'PY'
import json
with open('data/gnome-shell-extension/metadata.json', encoding='utf-8') as f:
    json.load(f)
print('GNOME extension metadata parses successfully')
PY

if command -v node >/dev/null 2>&1; then
  node --check data/gnome-shell-extension/extension.js
  echo "GNOME extension JavaScript syntax check passed"
else
  echo "NOTE: node is not installed; skipping JavaScript parser check" >&2
fi

python3 - <<'PY'
import tomllib
with open('Cargo.toml','rb') as f:
    version = tomllib.load(f)['package']['version']
checks = {
    'Cargo.lock': open('Cargo.lock', encoding='utf-8').read(),
    'snapcraft.yaml': open('snapcraft.yaml', encoding='utf-8').read(),
    'data/net.milmit.LinuxCare.metainfo.xml': open('data/net.milmit.LinuxCare.metainfo.xml', encoding='utf-8').read(),
}
for path, text in checks.items():
    if version not in text:
        raise SystemExit(f'version mismatch: {path} does not contain {version}')
print(f'version metadata consistent: {version}')
PY

echo "== LinuxCare native build preflight =="
require_command cargo
require_command rustc
require_command pkg-config

if command -v pkg-config >/dev/null 2>&1; then
  if ! pkg-config --exists gtk4; then
    echo "Missing native development package: gtk4 (Ubuntu: libgtk-4-dev)" >&2
    fail=1
  fi
  if ! pkg-config --exists libadwaita-1; then
    echo "Missing native development package: libadwaita-1 (Ubuntu: libadwaita-1-dev)" >&2
    fail=1
  fi
fi

if [ "$fail" -ne 0 ]; then
  cat >&2 <<'MSG'

Build preflight failed. On Ubuntu, install the native dependencies with:
  sudo apt update
  sudo apt install -y build-essential pkg-config libgtk-4-dev libadwaita-1-dev libgraphene-1.0-dev

Then install/use a Rust toolchain compatible with Cargo.toml (MSRV 1.92) and rerun:
  ./scripts/verify.sh
MSG
  exit 2
fi

rustc --version
cargo --version
pkg-config --modversion gtk4
pkg-config --modversion libadwaita-1

if [[ -x /usr/sbin/smartctl || -x /usr/bin/smartctl ]]; then
  echo "Optional SMART backend detected: smartctl"
else
  echo "NOTE: smartmontools is not installed; Hardware Doctor inventory will work, but SMART/NVMe health requires it" >&2
fi

if command -v notify-send >/dev/null 2>&1; then
  echo "Optional desktop notification backend detected: notify-send"
else
  echo "NOTE: notify-send is not installed; Notification Center history will work, but desktop alerts require libnotify-bin" >&2
fi

if command -v ss >/dev/null 2>&1; then
  echo "Deep Network socket backend detected: ss"
else
  echo "NOTE: ss is unavailable; Deep Network socket/process ownership view requires iproute2" >&2
fi

if command -v bpftool >/dev/null 2>&1; then
  echo "Optional eBPF capability tool detected: bpftool"
else
  echo "NOTE: bpftool is not installed; LinuxCare will report kernel eBPF readiness without loading programs" >&2
fi

if command -v gnome-extensions >/dev/null 2>&1; then
  echo "GNOME extension management CLI detected: gnome-extensions"
else
  echo "NOTE: gnome-extensions is unavailable; LinuxCare can still share top-bar settings but cannot toggle the extension from Settings" >&2
fi

echo "== Rust format / lint / test / build =="
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --workspace --all-targets

echo "LinuxCare verification completed successfully."
