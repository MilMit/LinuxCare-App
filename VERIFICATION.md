# LinuxCare 0.7.0-beta.1 verification status

Date: 2026-08-30

## Verified base

`0.2.0-alpha.3` completed the full native verification gate on the Ubuntu development host:

- Rust formatting
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-targets`
- `cargo build --workspace --all-targets`
- XML/JSON metadata parsing
- shell syntax checks
- GNOME extension JavaScript syntax check
- privileged-shell regression grep

The reported result was: `LinuxCare verification completed successfully.`

## Post-alpha.3 additions requiring native re-verification

The current candidate includes all post-alpha.3 work through the 0.7.0-beta.1 Beta-preparation batch. The artifact-preparation environment does not contain the Rust/GTK native toolchain, so these deltas are not claimed compile-verified here.

Run on the Ubuntu development host:

```bash
cargo fmt
./scripts/verify.sh
```

The script must pass format, check, Clippy with warnings denied, tests, and all-target build.


## Automated Beta-preparation gate

After native dependencies are installed, run:

```bash
cargo fmt
./scripts/beta-gate.sh 2>&1 | tee beta-gate-0.7.0-beta.1.log
```

The Beta gate now includes:

- `verify.sh` with `cargo check`, Clippy `-D warnings`, tests and all-target build
- dedicated privilege/security regression checks
- GTK startup smoke on a 1024×600 virtual display using an isolated HOME
- deterministic Debian build and package validation
- staged manual install → upgrade → uninstall with stale-extension cleanup and user-state preservation

On an ephemeral VM/CI runner, additionally run the real package smoke:

```bash
sudo ./scripts/smoke-deb-install.sh
```

That test installs a legacy fixture, upgrades to the current `.deb`, verifies obsolete package-owned files are removed, checks `linuxcare --version`, and finally removes the package.

## Manual smoke tests after a green build

1. Open **Boot Doctor** and verify timing, slow-unit, failed-service, and critical-chain rendering without UI blocking or mutation.
2. Open **Storage Analyzer** and confirm Storage Runway starts in Learning until its forecast guardrails are satisfied.
3. Open **Package Doctor** and verify apt/dpkg checks render; temporarily unavailable commands must show Unknown rather than zero.
4. Confirm Package Doctor does not run `apt update` and the diagnostics work even when the privileged helper is absent.
5. Confirm APT cache cleanup disables cleanly when the helper is unavailable and autoremove still requires two explicit clicks when candidates exist.
6. Open **Battery Lab** on a laptop and verify capacity/status plus whichever health, cycle, energy, voltage/current/power and threshold attributes the kernel actually provides.
7. On a desktop/VM with no Battery-type supply, confirm Battery Lab says no battery detected rather than fabricating values.
8. Confirm System Vitals battery display still updates using the shared collector.
9. Re-run Smart Cleaner and Health Intelligence regression smoke tests from alpha.3.
10. Open **Network Doctor** with normal Wi-Fi/Ethernet and confirm connectivity, route, DNS and interfaces render without mutation.
11. If available, test a captive portal/VPN or multiple default routes and verify LinuxCare reports review context rather than claiming a fault.
12. Open **Containers** with Docker/Podman absent, installed-but-unreachable, and reachable where possible; each state must render distinctly.
13. Confirm the Containers page has no prune/remove/stop action and reported reclaimable space is labelled as an estimate.
14. Re-test privileged helper/Polkit paths before packaging.
15. Open **Process Intelligence** and confirm the first minute remains in Learning mode rather than fabricating anomalies.
16. Leave LinuxCare running long enough to collect multiple samples; confirm CPU/RAM/network/disk historical metrics update and the state file remains bounded/private.
17. Generate a safe temporary CPU or disk-I/O load and confirm an anomaly appears only after a baseline exists.
18. Confirm process rows show CPU, RSS RAM and disk I/O but do not claim per-process network throughput.
19. Confirm `~/.local/state/linuxcare/process-intelligence.json` (or XDG equivalent) contains no command-line arguments or file paths.
20. Open **Hardware Doctor** and verify DMI/CPU/memory/GPU/storage/thermal inventory renders without authentication.
21. With `smartmontools` absent, click SMART/NVMe Health and verify the missing-tool state is explicit rather than a fabricated health result.
22. With `smartmontools` installed, authenticate once and test SMART on at least one real device. Confirm serial/WWN data is not rendered or present in the normalized D-Bus result.
23. If an NVMe drive is available, verify percentage-used/critical-warning/media-errors/unsafe-shutdown fields render only when reported.
24. If an ATA/SATA drive is available, verify reallocated/pending/offline-uncorrectable counters render only when reported.
25. Run the storage benchmark on ext4/btrfs/xfs where available and verify the UI reports `Direct I/O` or an explicit `Buffered fallback`, with MiB/s units and filesystem name.
26. Verify the benchmark refuses tmpfs/overlay and refuses a target with insufficient free space instead of displaying a speed.
27. Re-run the privileged helper/Polkit regression checks and confirm no arbitrary hardware command/path API was introduced.

Do not tag `v0.7.0-beta.1` until the full script and these smoke tests pass.

## Alpha.9 Safety Quarantine smoke tests

1. Run Smart Scan and select a small, disposable user cache. Confirm cleanup. Verify the UI reports it as **protected for Undo**, not as freed disk space.
2. Open **Safety & Timeline** and confirm the quarantine transaction appears with a remaining undo window.
3. Click Undo before the application recreates the cache. Verify the previous cache content is restored and an Undo timeline event appears.
4. Repeat cleanup, then create a new file inside the recreated target directory. Verify Undo is blocked and the new file is not overwritten.
5. Use Purge Now: the first click must only arm the action; the second click within five seconds permanently purges. Verify a Purge event records actual freed bytes.
6. Confirm APT/Snap/journal/provider actions do not create an Undo button and are described as non-reversible in the timeline.
7. Confirm `~/.cache/.linuxcare-quarantine-*` entries are not surfaced as normal top-level application-cache candidates.
8. Close LinuxCare with a quarantine record, wait beyond the undo window, relaunch, and confirm expired data is purged and logged.


## 0.4.0-alpha.1 consolidated intelligence/system batch

This candidate batches the next diagnostics milestone rather than releasing one feature per alpha. It adds Maintenance Intelligence and explainable Recommended Actions, Health Score 2.0 domains, Service Doctor, Package Doctor 2.0, Network Doctor 3.0, Filesystem Doctor with snapshot awareness, Thermal & Power diagnostics, and Battery Lab 2.0 history/trends.

The artifact-preparation environment can run static metadata/security checks but does not have the native Rust/GTK toolchain. Native verification therefore remains a hard release gate on the Ubuntu development host.

Run once and keep the complete log:

```bash
cargo fmt
./scripts/verify.sh 2>&1 | tee verify-0.7.0-beta.1.log
```

If the gate fails, report the complete error block from that single run so all compiler/Clippy issues can be fixed together.

### Consolidated manual smoke test

1. **Dashboard / Health Score 2.0:** verify Storage, Performance, Reliability, Security, Network, Packages, Thermal and Battery domain cards render. A missing battery/sensor/tool must show Unknown and must not silently lower the weighted denominator.
2. **Maintenance Center:** run Analyze System. Verify recommendations include a priority, why, suggested next step, privilege context and reversibility context. No recommendation may execute an action by itself.
3. **Service Doctor:** verify system and user scopes are distinguished. An unavailable scope must render Unknown/Partially checked. User-service Start/Restart/Stop controls must not imply system-service mutation; Stop requires explicit confirmation.
4. **Package Doctor 2.0:** verify `apt-get check`, `dpkg --audit`, held/autoremove/upgrade candidates, source count, exact duplicate-source detection, APT metadata age, foreign architectures and additional kernel images. Confirm no `apt update`, source edit or kernel removal is performed.
5. **Network Doctor 3.0:** normal refresh must remain local-only. Verify IPv4/IPv6 default routes, DNS, NetworkManager devices, MTU and interface error/drop counters. Only the explicit Active Probe button may ping the local gateway and resolve `example.com`.
6. **Filesystem Doctor:** verify persistent mounts, capacity/inode pressure and read-only state. If kernel journal access is denied, the filesystem-error check must be Unknown rather than Healthy. Confirm no fsck is launched.
7. **Btrfs/Snapshot awareness:** on a Btrfs/Snapper/Timeshift system, verify detection/counts are read-only. Confirm no snapshot create/delete/rollback command exists.
8. **Thermal & Power:** verify exposed hwmon temperatures, CPU governor/frequency, power profile and throttle counters. Missing sensors must render Unknown. LinuxCare must not change governor or power profile from this page.
9. **Battery Lab 2.0:** verify the existing live battery fields plus bounded 14-day local trend history. Health delta must remain Learning until at least 24 hours of compatible samples exist.
10. **Regression:** repeat alpha.9 Safety Quarantine Undo/Purge tests and alpha.8 SMART/benchmark checks; the consolidated batch must not weaken either privilege boundary.

Do not tag `v0.7.0-beta.1` until the native gate is green and the relevant smoke tests have been exercised on real Ubuntu/GNOME hardware.


## 0.4.0-alpha.1 Automation & Deep Care smoke test

After the native gate is green:

1. Open **Automation** and enable a timer. Verify `systemctl --user status linuxcare-maintenance.timer` reports active/enabled and inspect both generated unit files under `~/.config/systemd/user/`.
2. Start one run manually with `systemctl --user start linuxcare-maintenance.service`. Confirm **Safety & Timeline** gets a Scheduled Smart Scan event.
3. In **Scan + safe quarantine** mode, use disposable SAFE cache data. Confirm no privileged action is invoked and protected bytes do not exceed the configured automation budget.
4. Set a very small Quarantine cap and confirm additional quarantine is rejected instead of silently exceeding the cap.
5. Recreate/populate a cleaned cache target, verify normal Undo is blocked, then use **Restore Aside** and confirm new data remains untouched while the older copy is recovered to a sibling path.
6. Change Undo retention and auto-purge policy, restart LinuxCare, and verify expired data is kept when auto-purge is disabled.
7. Use **Test Alert** and verify Notification Center logs the event; if `notify-send` is installed, verify desktop delivery too.
8. Generate a critical Process Intelligence anomaly after baseline learning and verify repeat alerts are rate-limited rather than emitted every 3 seconds.
9. Run Smart Scan twice with a controlled cache-growth change and verify **Storage Intelligence 2.0** identifies the changed category without claiming it explains unrelated total filesystem growth.
10. Leave Process Intelligence running and confirm the local telemetry file remains private/bounded and retains at most six hours of samples.

Do not tag `v0.7.0-beta.1` until this native gate and the relevant automation/quarantine smoke tests pass.


## 0.5.0-alpha.1 Deep Network / GNOME smoke test

After the native gate is green:

```bash
ss -H -t -u -n -a -p | head
command -v bpftool || true
cat /proc/sys/kernel/unprivileged_bpf_disabled 2>/dev/null || true
gnome-shell --version || true
gnome-extensions info linuxcare-vitals@milmit.net || true
systemctl --user show linuxcare-maintenance.timer \
  -p NextElapseUSecRealtime -p LastTriggerUSec --no-pager || true
```

Manual UI checks:

1. Network Doctor must show Deep Network socket counts without claiming per-process bandwidth.
2. Refreshing Network Doctor must not create persistent endpoint-history files.
3. Settings > GNOME Integration must report installed/enabled/compatibility truthfully and only toggle LinuxCare's UUID.
4. Changing the top-bar refresh interval must update `config.json` and be picked up by the extension monitor.
5. Automation Run Now must start `linuxcare-maintenance.service`, not a direct shell command.
6. Storage Intelligence 3.0 must stay Learning/Low confidence with insufficient elapsed history.

Do not tag `v0.7.0-beta.1` until `./scripts/verify.sh` passes with `-D warnings` and these smoke checks have been exercised on Ubuntu/GNOME.

## 0.7.0-beta.1 release gate

Release CI additionally runs `scripts/check-release.sh`, `scripts/validate-package.sh`, AppStream/Desktop/ShellCheck validators, RustSec and CodeQL, then emits SHA256, SPDX SBOM and artifact attestations. Ubuntu 26.04 compatibility is informative while the hosted runner remains preview; Ubuntu 24.04 is the blocking package gate.

For the consolidated pre-release checklist, see `RELEASE_CHECKLIST.md`.
