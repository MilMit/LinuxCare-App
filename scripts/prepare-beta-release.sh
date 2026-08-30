#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REAL_INSTALL=0
REQUIRE_SCREENSHOTS=0
for arg in "$@"; do
  case "$arg" in
    --real-install-smoke) REAL_INSTALL=1 ;;
    --require-screenshots) REQUIRE_SCREENSHOTS=1 ;;
    *) echo "Unknown option: $arg" >&2; exit 2 ;;
  esac
done

./scripts/beta-gate.sh
cargo build --release --workspace
./scripts/build-deb.sh
./scripts/validate-package.sh
if (( REAL_INSTALL )); then
  sudo ./scripts/smoke-deb-install.sh
else
  echo "NOTE: real root-level .deb smoke was not requested. Use --real-install-smoke on a disposable host/VM before public binary publication." >&2
fi
if (( REQUIRE_SCREENSHOTS )); then
  python3 scripts/validate-screenshots.py assets/screenshots
else
  echo "NOTE: AppStream screenshot publication gate skipped. Use --require-screenshots when real release captures are present." >&2
fi
./scripts/package-release-assets.sh

echo "LinuxCare Beta release pack completed."
