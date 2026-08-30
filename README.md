# LinuxCare 0.7.0-beta.1 — Beta Preparation Candidate

LinuxCare is a native Rust + GTK4/Libadwaita maintenance and diagnostics suite for Ubuntu/GNOME.

`0.7.0-beta.1` is a stabilization batch rather than a feature expansion. It adds automated GUI/package/lifecycle smoke tests, stricter privilege-boundary regression checks, migration compatibility for older settings files, safer manual install/uninstall staging, and calmer/adaptive UI defaults. The goal is to make failures reproducible before a public Beta, not to add another dashboard.

## New in 0.7.0-beta.1

- one-command `./scripts/beta-gate.sh` for source/security/native/package/lifecycle checks
- 1024×600 GUI startup smoke under an isolated HOME and D-Bus session
- real Debian legacy-upgrade → current-install → remove smoke test on ephemeral CI runners
- staged manual install/upgrade/uninstall lifecycle test with user-state preservation
- stricter security regression suite for Shell spawning, scheduled privilege boundaries, helper interpreters, Polkit declarations, SUID/SGID payloads, and fixed D-Bus activation paths
- older partial `automation.json` and `cleanup-policy.json` now deserialize with safe defaults
- `linuxcare --version` / `--help` support for headless package verification
- GNOME extension power-profile changes now use typed system D-Bus instead of command-line `gdbus` spawning
- GNOME extension app launch no longer falls back to shell command spawning
- explicit 0600 creation for the shared LinuxCare settings file
- reduced emoji-heavy control labels and a smaller 1100×720 default window while preserving the adaptive `AdwFlap` navigation
- CI GUI/package/lifecycle smoke coverage on Ubuntu 24.04 plus non-blocking Ubuntu 26.04 compatibility coverage

## Previous 0.6.0-alpha.1 release-hardening batch

`0.5.0-alpha.1` consolidates the next observability/desktop-integration batch: **Deep Network** socket ownership with truthful eBPF readiness, **Storage Intelligence 3.0** with longer baselines and trend confidence, richer **systemd automation status/manual Run Now**, and a first-class **GNOME Integration** status/control surface. The GUI remains unprivileged and the project still refuses to claim per-process network bandwidth without an actual eBPF accounting backend.

0.6.0-alpha.1 is a release-hardening batch: it does not pretend that more dashboards are the missing piece. It pins the release toolchain, adds MSRV and Ubuntu-generation compatibility gates, audits dependencies, validates Debian/AppStream/Desktop metadata, generates an SPDX SBOM, and attests release artifacts. Runtime safety boundaries from 0.5 remain unchanged.

## New in 0.6.0-alpha.1

- required Ubuntu 24.04 CI and non-blocking Ubuntu 26.04 preview compatibility
- Rust 1.93 quality toolchain plus Rust 1.92 MSRV check
- RustSec + CodeQL security workflows and Dependabot
- deterministic `.deb` metadata and package smoke/lint checks
- SPDX 2.3 Cargo SBOM and GitHub provenance/SBOM attestations
- strict Git tag/version guard before publication
- AppStream/Desktop/ShellCheck validation in release CI
- GNOME extension metadata cleanup for extension-store review

## Previous 0.5.0-alpha.1 feature batch

### Deep Network & eBPF readiness

- Adds a live `ss`-based socket ownership view grouped by process/PID when Linux exposes that metadata to the current user.
- Shows established TCP, UDP, total socket count, visible process ownership, and distinct remote-endpoint counts without persisting endpoint history.
- Detects bpffs mount state, cgroup v2, optional `bpftool`, and the kernel unprivileged-BPF policy.
- **Does not claim per-process upload/download bytes.** LinuxCare will only add that metric when a real eBPF/cgroup accounting backend exists.
- Missing PID ownership stays unknown instead of being guessed.

### Storage Intelligence 3.0

- Uses the oldest meaningful Smart Scan baseline within the recent 30-day window instead of comparing only against an arbitrarily close previous scan.
- Adds daily reclaimable-data growth rate, trend confidence, category churn, and clearer baseline age context.
- Confidence stays Learning/Low/Medium/High based on actual sample count and elapsed history.
- Storage Runway remains the filesystem-capacity forecaster; Storage Intelligence explains cleanup-category growth rather than mixing the two concepts.

### Automation control surface

- Shows systemd-reported **Next run** and **Last trigger** when available.
- Adds an explicit **Run Now** action that starts the already-installed user-level maintenance service.
- Manual Run Now uses the same hardened unprivileged service boundary as scheduled runs.
- Scheduled privileged cleanup remains prohibited.

### GNOME integration polish

- Settings now shows detected GNOME Shell major version, extension install location, compatibility, and enabled state.
- LinuxCare can explicitly enable/disable its own extension through `gnome-extensions`; it does not restart GNOME Shell.
- Top-bar refresh interval is configurable at 1/2/5/10 seconds through the shared LinuxCare config.
- The extension UI moves away from emoji-heavy indicators to compact `CPU / TMP / RAM / NET / BAT` labels for more predictable GNOME rendering.

## Automation & Deep Care foundation (0.4.0-alpha.1)

### Real Automation with systemd user timers

- Generates `~/.config/systemd/user/linuxcare-maintenance.service` and `.timer` only after an explicit **Save & Enable Timer** action.
- Supports Daily/Weekly schedules and three modes: **Scan only**, **Scan + notify**, and **Scan + safe quarantine**.
- The headless runner never invokes Polkit or privileged cleanup. Automated cleanup is restricted to candidates already classified `SAFE` and `UserAllowedDirectory`.
- The user service runs with `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome=read-only`, an idle I/O scheduling class, and narrow write paths.
- Sandboxed/development Store builds intentionally refuse user-systemd automation; this feature targets the full host/.deb installation.

### Notification Center

- Adds local LinuxCare notification history under XDG state storage.
- Uses desktop `notify-send` when available and preserves the local event even if desktop delivery is unavailable.
- Repeated identical automatic alerts are rate-limited to avoid notification spam.
- Critical Process Intelligence anomalies can create rate-limited alerts.

### Storage Intelligence 2.0

- Reads bounded Smart Scan snapshots and explains which cleanup categories grew or shrank between scans.
- Separates total filesystem growth (Storage Runway) from cleanup-category growth (Storage Intelligence).
- Does not invent a cause when there is only one scan baseline.

### Safe Cleanup 2.0

- Undo retention is configurable: 30 minutes, 1 hour (default), 1 day, or 7 days.
- Adds an explicit total Quarantine cap and a separate smaller budget for unattended safe cleanup.
- Expired-copy auto-purge can be disabled.
- **Restore Aside** recovers protected data next to a recreated target instead of overwriting newer application state.

### Deeper Process History

- Extends persisted Process Intelligence history from one hour to six hours.
- Historical Vitals now summarizes the last 30 minutes while anomaly detection continues to use a bounded recent baseline.
- Command lines and file paths remain excluded from persisted process telemetry.

## Consolidated intelligence/system foundation (0.3.0-alpha.1)

### Maintenance Intelligence & Recommended Actions

- Adds a dedicated **Maintenance Center** that checks eight areas together: storage, services, packages, network, filesystems, thermal state, battery and boot.
- Recommendations are prioritized as Critical / High / Medium / Low and always include **Why** plus a suggested next step.
- Each recommendation states whether the proposed next step is potentially reversible and whether administrator privileges may be involved.
- Unknown diagnostics are not silently converted to healthy results.
- Safety Quarantine remains truthfully accounted: protected bytes are not counted as freed space.

### Health Score 2.0

- Dashboard health now exposes domain scores for **Storage, Performance, Reliability, Security, Network, Packages, Thermal and Battery**.
- Overall score is a weighted average of only the domains LinuxCare can actually verify. Unknown domains are excluded from the denominator instead of being treated as failures.
- Battery absence on desktops/VMs and unavailable firewall/sensor data therefore do not artificially lower the score.

### Service Doctor

- Replaces the generic Background Services view with a systemd-focused **Service Doctor**.
- Detects failed units and current auto-restart/activating states separately for system and user scopes.
- Counts enabled and masked service unit files without claiming that enabled services are unnecessary.
- If a systemd scope cannot be enumerated, LinuxCare reports **Partially checked / Unknown**, not Healthy.
- User-session Start/Restart controls remain available; Stop requires an explicit second click. LinuxCare never disables system services automatically.

### Package Doctor 2.0

- Adds exact duplicate APT source detection across classic `.list` and deb822 `.sources` definitions.
- Reports age of the newest cached APT metadata without running `apt update` in the background.
- Lists non-running versioned `linux-image-*` packages conservatively as **additional kernels**, never as automatically safe-to-remove kernels.
- Shows configured foreign dpkg architectures and retains held/autoremove/upgrade diagnostics.

### Network Doctor 3.0

- Separates IPv4 and IPv6 default-route context.
- Reads MTU plus RX/TX error/drop and carrier-change counters from `/sys/class/net`.
- Normal refresh remains local-only and creates no external DNS query.
- **Run Active Probe** is explicit: it pings the local default gateway and resolves `example.com` through the configured resolver to measure DNS resolution latency.
- Network-bound listeners remain described as exposure context, not automatically Internet-public ports.

### Filesystem Doctor & Snapshot Awareness

- Adds persistent mount inventory from `/proc/self/mountinfo` plus `statvfs` capacity and inode pressure.
- Detects unexpected read-only state and current-boot filesystem-related kernel errors when the journal is readable.
- If the kernel journal cannot be read, LinuxCare reports a partial check rather than assuming no errors exist.
- Adds read-only Btrfs/Snapper/Timeshift awareness. No snapshot create/delete/restore/prune action exists in this preview.

### Thermal & Power Doctor

- Adds hwmon temperature classification using exposed max/critical thresholds where available.
- Shows CPU governor, current/max frequency, power profile, and available thermal-throttle counters.
- Historical throttle counters are presented as evidence to investigate, not proof of a current cooling fault.

### Battery Lab 2.0

- Records a private local battery sample at most every 15 minutes for up to 14 days.
- Adds capacity-health trend after a minimum 24-hour span and recent average discharge-power context.
- Trend data remains local under XDG state storage and does not change firmware charge thresholds.

### Hardware & Storage Diagnostics (alpha.8)

- Adds a dedicated **Hardware Doctor** with local DMI system identity, CPU topology, memory, DRM GPU/driver inventory, physical block devices, and readable hwmon temperature sensors.
- SMART/NVMe health is **never read automatically**. The user explicitly starts the check, LinuxCare requests its dedicated Polkit action, and the root helper runs only a fixed `smartctl` JSON query against a validated `/sys/block` device.
- The helper normalizes the SMART result before returning it. Serial numbers, WWNs, raw command output, arbitrary paths, and user-supplied smartctl arguments never cross into the GTK process.
- NVMe health includes critical-warning state, endurance percentage used, media errors, unsafe shutdowns, temperature and power-on time when reported. ATA health includes reallocated, pending, offline-uncorrectable and SMART error-log counters when available.
- Storage health classification does not blindly equate `SMART PASSED` with a perfect drive: critical/media/pending/uncorrectable conditions are Attention; reallocated sectors or very high endurance use remain Review.
- Reworks the Vitals storage benchmark into a **256 MiB sequential filesystem throughput test**. LinuxCare prefers `O_DIRECT`, falls back to synchronized buffered I/O with page-cache eviction only when necessary, reports the mode, and refuses tmpfs/ramfs/overlay/squashfs-style targets that would not represent physical storage.
- Benchmark results are explicitly filesystem throughput, not a raw-device or vendor-spec SSD benchmark.

### Process Intelligence & Historical Vitals (alpha.7)

- Samples per-process CPU, RSS memory, and disk I/O from `/proc` without root privileges.
- Stores a bounded local time-series under `XDG_STATE_HOME/linuxcare/process-intelligence.json` (fallback: `~/.local/state/linuxcare/`).
- Persists only process name, PID/start identity, and resource counters; command lines and file paths are not stored.
- Keeps up to six hours of history, persists at 15-second intervals, and shows a 30-minute historical vitals summary.
- Anomaly detection remains in **Learning** state until a minimum time-series baseline exists, then detects CPU spikes, sustained memory growth, disk-I/O spikes, and system-level network spikes.
- LinuxCare does **not** fake per-process network byte attribution from `/proc/<pid>/net`; that metric remains unavailable until a future eBPF/cgroup-backed implementation.



## New in alpha.9: Safety Undo & Maintenance Timeline

- **Safety Quarantine:** executable user-space cleanup targets are atomically detached into hidden same-filesystem quarantine directories before any permanent deletion.
- **Truthful space accounting:** quarantined bytes are labelled **Protected for Undo**, not recovered/free space. They remain allocated until purge.
- **Configurable undo window:** active protected copies can be retained for 30 minutes, 1 hour (default), 1 day, or 7 days. Expired copies are purged only when the Safe Cleanup policy allows automatic expiry housekeeping.
- **Conflict-safe Undo:** if an application recreates and repopulates the original directory, LinuxCare refuses to overwrite it and preserves quarantine for manual review/purge.
- **Two-stage permanent purge:** manual purge requires a second click within five seconds.
- **Maintenance Timeline:** completed Smart Scans, cleanup runs, Undo operations, and purge operations are stored locally with actual-freed/protected/restored values kept separate.
- **Privilege boundary remains honest:** APT, Snap, journal, Flatpak/provider maintenance is recorded but never presented as rollback-capable.
- **Crash-recovery intent:** quarantine metadata is persisted before directories are detached, so interrupted operations can be rediscovered on the next launch.

## Implemented

- Live system Vitals (CPU, RAM, temperature, network, battery)
- Process Intelligence with per-process CPU/RAM/disk-I/O history and baseline-gated anomaly detection
- Smart Cleaner with explicit risk/executability states
- user cache, thumbnails, Trash, developer cache and user coredump cleanup through a strict user allowlist
- APT cache cleanup through the privileged helper
- APT autoremove through a separate Polkit action and explicit two-click confirmation
- disabled Snap revision cleanup with revision revalidation
- system journal vacuuming with allowlisted retention values
- Flatpak unused-runtime provider
- Storage Analyzer, Storage Runway, and a guarded direct-I/O-first sequential filesystem throughput test
- Security HUD for listening sockets and firewall state
- Network Doctor 3.0 for IPv4/IPv6 routes, DNS, interface counters/MTU, explicit active probe, firewall context, and listener exposure
- Docker/Podman Analyzer for read-only engine detection, storage categories, and reclaimable-space estimates
- GNOME Shell top-bar extension for read-only live metrics and power-profile shortcuts
- Safety Quarantine, Maintenance Timeline, legacy cleanup history, and appearance/top-bar settings
- conservative System Health Score and local What Changed? baselines
- Boot Doctor for systemd boot timing, slow units, failed services, and critical-chain review
- Maintenance Center with explainable prioritized recommendations across eight system areas
- Service Doctor for failed/restarting system and user units with truthful unavailable-scope handling
- Filesystem Doctor with inode/capacity/read-only/error context and read-only snapshot awareness
- Thermal & Power Doctor with hwmon thresholds, CPU frequency/governor, power profile and throttle context
- Storage Runway with local trend sampling and conservative 80/90/95% capacity forecasts
- Package Doctor 2.0 with APT/dpkg consistency checks, duplicate source detection, metadata age, additional-kernel context, multiarch visibility, and explicit maintenance separation
- Battery Lab 2.0 with multi-battery discovery, health/wear estimation, local trend history, discharge-power context, electrical telemetry, runtime estimates, and charge-threshold visibility
- Hardware Doctor with DMI/CPU/GPU/storage/thermal inventory plus explicit authenticated SMART/NVMe health


## New in alpha.6: Network & Container Intelligence

- **Network Doctor 2.0:** combines cached NetworkManager connectivity state, default-route inspection, DNS discovery, interface state, firewall context, and listener counts without changing networking.
- **Truthful reachability language:** a network-bound listener is not labelled Internet-public; LinuxCare keeps router/NAT, VPN, namespace and firewall uncertainty explicit.
- **Route/DNS review:** multiple default routes, missing default route, missing DNS, captive portal, limited connectivity, and offline states generate review notes instead of automatic repair.
- **Docker/Podman Analyzer:** detects each CLI independently and queries engine disk usage on a worker thread with a bounded command timeout.
- **Storage breakdown:** images, containers and volumes are shown with engine-reported total/active counts, used space and reclaimable estimates when available.
- **No prune in alpha.6:** the analyzer is intentionally read-only; there is no image removal, volume deletion, container stop, or `system prune` action.
- **Estimate guardrail:** reclaimable values are presented as estimates because shared layers and active references can make actual freed bytes differ from engine reports.
- **Regression fix:** includes the Rust 1.93 Clippy `manual_range_contains` fix in Battery Lab runtime validation.

## New in alpha.5: Package & Battery Intelligence

- **Package Doctor:** runs fixed, read-only `apt-get check`, `dpkg --audit`, `apt-mark showhold`, and APT simulation commands on a worker thread.
- **Truthful package state:** a failed/unavailable diagnostic is shown as **Unknown**, never silently converted into “0 problems”.
- **Cached-metadata upgrade view:** upgrade candidates are informational and explicitly do not trigger `apt update` or network probing.
- **Safe maintenance separation:** APT cache cleanup and autoremove remain separate Polkit-authorized actions; autoremove keeps its explicit two-click confirmation.
- **Battery Lab:** discovers Battery-type devices from `/sys/class/power_supply` instead of assuming only `BAT0`, and supports multiple batteries/peripheral battery supplies.
- **Battery health guardrail:** full/design health is calculated only from matching energy units or matching charge units; incompatible µWh/µAh sources are never mixed.
- **Battery telemetry:** current/design/full energy, voltage, current, power draw, cycle count, status, runtime estimate, technology/scope, and firmware charge thresholds when the kernel exposes them.
- **Read-only thresholds:** LinuxCare reports firmware charge thresholds but does not modify them in alpha.5.

## New in alpha.4: Boot & Capacity Intelligence

- **Boot Doctor:** reads `systemd-analyze time`, `blame`, and `critical-chain` plus failed system services without requiring privilege.
- **Safe startup guidance:** slow services are review targets only; LinuxCare does not automatically disable units or pretend that a long unit is unnecessary.
- **Boot phase breakdown:** firmware, loader, kernel, userspace, graphical target, total boot status, and the slowest startup contributors.
- **Storage Runway:** stores small root-disk samples under `XDG_STATE_HOME/linuxcare/storage-runway.json` and estimates capacity pressure from a local linear trend.
- **Forecast guardrails:** no forecast is shown until at least three usable samples span 12 hours; near-flat or shrinking usage produces no fake fill date.
- **80/90/95% targets:** estimates are explicitly labelled as trends and accompanied by caveats about downloads, updates, cleanup, snapshots, and partition changes.
- **Algorithm tests:** duration parsing/systemd blame ordering plus storage trend regression and flat-growth refusal tests.

## New in alpha.3: Health Intelligence

- **Conservative Health Score (0–100):** calculated only from signals LinuxCare can verify; unavailable data is shown as unknown and never silently penalized.
- **Verified health signals:** system-disk pressure, MemAvailable-based memory pressure, failed system/user services, UFW state, and network-bound listening sockets.
- **What Changed?:** persistent system snapshots compare disk usage, kernel version, failed services, firewall state, and listener counts over time.
- **Smart Scan history:** every completed scan stores category totals so LinuxCare can report meaningful growth or reduction in caches, trash, APT data, Flatpak runtimes, crash reports, and other scan categories.
- **Private local state:** health/scan snapshots live under `XDG_STATE_HOME/linuxcare` (or `~/.local/state/linuxcare`) with restrictive permissions.
- **Dashboard-only intelligence:** the Health Intelligence panel appears on Dashboard while Smart Cleaner continues recording scan baselines without duplicating the panel.

> Build status: alpha.3 was verified successfully on the Ubuntu development host. Alpha.8 adds Hardware Doctor, authenticated SMART/NVMe health, and the direct-I/O-first benchmark on top of later diagnostic modules; the complete current tree must pass `cargo fmt` and `./scripts/verify.sh` on Ubuntu before alpha.8 is tagged.

## Important alpha.2 safety changes

- LinuxCare no longer exposes a "RAM boost" / forced kernel `drop_caches` operation. Linux reclaims filesystem cache automatically when applications need memory.
- The GNOME Shell extension contains no `pkexec` or shell-based root mutation path.
- `/var/crash` is review-only; user-owned `~/.local/share/coredump` is the only crash-dump directory executable by the user cleanup engine.
- Process termination and APT autoremove require a second explicit click.
- Network-bound sockets are labelled `NETWORK`, not `PUBLIC`; actual reachability depends on firewall/router/network rules.
- The storage benchmark now prefers direct I/O, reports explicit fallback/refusal states, and is labelled filesystem throughput rather than raw SSD performance.
- SMART/NVMe health is available only through the explicit authenticated Hardware Doctor workflow; LinuxCare does not run SMART self-tests or mutate drive settings.
- Automation controls are preview-only until a real scheduler exists.

## Privileged helper API

```text
GetVersion()
CleanAptCache()
AutoremovePackages()
RemoveDisabledSnapRevision(name, revision)
VacuumJournal(keep_days)
ReadStorageHealthJson(device_basename)
```

There is deliberately no generic command runner, shell API, arbitrary path deletion API, or kernel cache-drop method.

## Build prerequisites (Ubuntu)

```bash
sudo apt update
sudo apt install -y \
  build-essential pkg-config libgtk-4-dev libadwaita-1-dev \
  polkitd dbus systemd

# Recommended for Hardware Doctor SMART/NVMe health:
sudo apt install -y smartmontools
```

Install a Rust toolchain compatible with the declared MSRV, then run:

```bash
./scripts/verify.sh
cargo build --release --bins
```

The project currently declares Rust `1.92` and uses `gtk4 0.11.4`, `libadwaita 0.9.2`, and `zbus 5.19`.

## Install system integration

```bash
sudo ./scripts/install-system.sh
```

The Debian package built by `./scripts/build-deb.sh` now includes the application icon and GNOME Shell extension in addition to the GUI, helper, Polkit, D-Bus and systemd files.

The helper is D-Bus activated and is not enabled as a permanent background daemon.

## Distribution status

The `.deb` path is the full-system build for this alpha. `snapcraft.yaml` is marked `grade: devel`; the classic snap is development packaging only because the current full maintenance model relies on host Polkit/D-Bus integration and has not yet been redesigned/approved for store confinement.

## Still intentionally blocked / incomplete

- arbitrary package uninstall
- old-kernel removal
- Docker/Podman prune
- volume deletion
- privileged/system-wide service or startup mutation
- SMART self-tests, firmware/security mutations, sanitize/format operations, or raw block-device writes
- scheduled destructive cleanup
- quarantine/undo engine

These should receive dedicated validation and recovery models instead of being routed through a generic privileged command.

See `SECURITY.md`, `ARCHITECTURE.md`, `CHANGELOG.md`, and `VERIFICATION.md` before extending privileged behavior.
