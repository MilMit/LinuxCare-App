#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "== LinuxCare Beta Gate: source / security / native =="
./scripts/verify.sh

echo "== LinuxCare Beta Gate: GUI startup =="
./scripts/smoke-gui.sh

echo "== LinuxCare Beta Gate: Debian package =="
./scripts/build-deb.sh
./scripts/validate-package.sh

echo "== LinuxCare Beta Gate: staged lifecycle =="
./scripts/test-install-lifecycle.sh

if [[ "${LINUXCARE_RUN_REAL_DEB_SMOKE:-0}" == "1" ]]; then
  if [[ ${EUID} -ne 0 ]]; then
    echo "LINUXCARE_RUN_REAL_DEB_SMOKE=1 requires root (or run the smoke script with sudo)." >&2
    exit 2
  fi
  echo "== LinuxCare Beta Gate: real Debian upgrade/install/remove =="
  ./scripts/smoke-deb-install.sh
else
  echo "NOTE: real .deb install/remove smoke skipped. On an ephemeral test host run:"
  echo "  sudo ./scripts/smoke-deb-install.sh"
fi

echo "LinuxCare beta gate completed successfully."
