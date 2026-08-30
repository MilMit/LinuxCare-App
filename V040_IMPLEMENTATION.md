# LinuxCare 0.4.0-alpha.1 — Automation & Deep Care Preview

This consolidated batch adds real user-level systemd automation, a local notification center, Storage Intelligence 2.0, six-hour process telemetry retention, and Safe Cleanup 2.0 policy controls.

## Safety boundaries

- Scheduled jobs are user-level systemd units; no permanent root daemon is introduced.
- Background cleanup never runs Polkit, APT, Snap, Journal or other privileged cleanup.
- Automated cleanup is limited to candidates already classified `SAFE` and `UserAllowedDirectory`, and moves them to Safety Quarantine rather than permanently deleting them.
- Quarantine retention, total cap and automation budget are explicit user-owned policies.
- Notification history is local/private and rate-limited.
- Process command lines and file paths are still not persisted by Process Intelligence.
- The six-hour Process Intelligence history is collected only while LinuxCare is running; this release does not add a permanent telemetry daemon.

## New state/config

- `~/.config/linuxcare/automation.json`
- `~/.config/linuxcare/cleanup-policy.json`
- `~/.config/systemd/user/linuxcare-maintenance.{service,timer}` when enabled
- `~/.local/state/linuxcare/notifications.json`

## Verification expectation

Run `cargo fmt` then `./scripts/verify.sh`. A functional smoke test should also enable the timer, inspect it with `systemctl --user status linuxcare-maintenance.timer`, run `systemctl --user start linuxcare-maintenance.service`, and inspect Safety & Timeline plus Notification Center.
