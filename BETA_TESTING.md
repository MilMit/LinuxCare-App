# LinuxCare Beta Testing Matrix

## Automated

```bash
cargo fmt
./scripts/beta-gate.sh
```

On an ephemeral Ubuntu VM/runner:

```bash
sudo ./scripts/smoke-deb-install.sh
```

## UX / adaptive GTK

Test at 1024×600, 1366×768 and a normal desktop resolution.

- Sidebar folds on narrow windows and can be reopened/closed without trapping focus.
- Every page remains vertically scrollable; critical actions are reachable at 1024×600.
- Keyboard Tab/Shift+Tab reaches header controls and page actions with visible focus.
- No action is communicated by color alone; risk text/badges remain explicit.
- With GNOME animations disabled, Scanner/health animations stop or remain usable without motion.
- Repeat at 150% and 200% text scaling and check clipping/ellipsizing.

## Upgrade / state

- Start from an older config containing only a subset of automation/cleanup-policy fields; LinuxCare must load safe defaults rather than reject it.
- Upgrade a package with stale extension files; stale package-owned files must disappear.
- Package removal must remove system integration while preserving `~/.config/linuxcare` and `~/.local/state/linuxcare`.
- If Automation was enabled before uninstall, the generated user timer is allowed to remain as user state, but its service contains `ConditionPathIsExecutable` so the missing runner cannot execute. Reinstall should restore operability when the runner returns.

## Destructive/safety flows

- Safety Quarantine, Undo, conflict-blocked Undo, Restore Aside and Purge.
- APT Autoremove two-step confirmation and Polkit prompt.
- SMART/NVMe explicit authorization on a real physical drive.
- Scheduled automation never performs privileged cleanup.

## GNOME integration

- Enable/disable LinuxCare Vitals only through LinuxCare/`gnome-extensions`.
- Power Profile switches through D-Bus without a Shell command launcher.
- Top-bar configuration survives atomic config rewrites and respects refresh interval.

## Exit criteria

A Beta is not ready merely because it compiles. The automated gate must be green, the real-package smoke must pass, and the manual safety/accessibility matrix must have no unresolved high-severity issue.
