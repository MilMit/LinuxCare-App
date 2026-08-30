#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

fail() { echo "SECURITY REGRESSION: $*" >&2; exit 1; }

# Never reintroduce generic shell/root command execution into the GUI or extension.
if grep -RInE 'execute_command|run_shell|run_as_root|sudo[[:space:]]+rm|sh[[:space:]]+-c|bash[[:space:]]+-c' src data/gnome-shell-extension; then
  fail "generic shell/root execution pattern detected"
fi

# The shell extension must use typed/fixed APIs rather than a command-line shell launcher.
if grep -RIn 'spawn_command_line_async' data/gnome-shell-extension; then
  fail "GNOME extension command-line spawning is forbidden"
fi

# Scheduled maintenance must never gain the privileged provider/helper boundary.
if grep -nE 'UbuntuSystemProvider|PrivilegedAction|PrivilegedProvider|linuxcare-helper|pkexec|sudo' \
  src/automation_engine.rs src/bin/linuxcare-maintenance.rs; then
  fail "scheduled maintenance privilege-boundary regression detected"
fi

# Root helper is deliberately small: generic interpreters and arbitrary executable paths are forbidden.
if grep -nE 'Command::new\("(sh|bash|zsh|python|python3|perl|ruby|env|sudo|pkexec)"\)' src/bin/linuxcare-helper.rs; then
  fail "root helper attempted to launch a generic interpreter/elevation frontend"
fi

# Polkit actions expected by the helper must remain explicitly declared.
for action in \
  net.milmit.LinuxCare.clean-apt-cache \
  net.milmit.LinuxCare.remove-snap-revision \
  net.milmit.LinuxCare.vacuum-journal \
  net.milmit.LinuxCare.autoremove-packages \
  net.milmit.LinuxCare.read-storage-health; do
  grep -Fq "$action" data/polkit-1/actions/net.milmit.LinuxCare.policy || fail "missing Polkit action: $action"
done

# D-Bus service activation must point only to the installed fixed helper path.
grep -Fq '/usr/libexec/linuxcare-helper' data/dbus-1/system-services/net.milmit.LinuxCare.Helper.service \
  || fail "D-Bus activation path is not the fixed helper path"

# Privileged helper service hardening must not silently regress.
for directive in \
  'User=root' \
  'NoNewPrivileges=true' \
  'PrivateTmp=true' \
  'ProtectHome=true' \
  'ProtectSystem=full' \
  'ProtectKernelTunables=true' \
  'ProtectKernelModules=true' \
  'ProtectControlGroups=true' \
  'RestrictSUIDSGID=true' \
  'RestrictAddressFamilies=AF_UNIX' \
  'MemoryDenyWriteExecute=true'; do
  grep -Fxq "$directive" data/systemd/linuxcare-helper.service || fail "missing helper hardening directive: $directive"
done

# Package/installer sources must not request SUID/SGID installation.
if grep -RInE 'install[[:space:]].*-m[[:space:]]*4?7[0-7][0-7][0-7]|chmod[[:space:]]+[2467][0-7]{3}' scripts data; then
  fail "SUID/SGID installation pattern detected"
fi

echo "LinuxCare security regression suite passed."
