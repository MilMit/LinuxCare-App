# Changelog

## 0.7.0-beta.2 — Debian Reliability Beta

- Fixed Debian package policy failures exposed by Lintian in CI.
- Replaced obsolete `policykit-1` dependency metadata with `polkitd` and added the direct `libc6` runtime dependency.
- Enabled release-symbol stripping for LinuxCare binaries.
- Added Debian copyright metadata and generated compressed `changelog.Debian.gz` package metadata.
- Added manual pages for `linuxcare` and `linuxcare-maintenance`.
- Wrapped long Debian control-description lines to satisfy package policy checks.
- Reworked package helper shutdown to use `deb-systemd-invoke` and added systemd `Documentation=` metadata.
- Hardened `validate-package.sh` so the Debian-policy regressions that affected beta.1 fail locally and in CI before publication.
- Preserved the staging-permission hardening that explicitly clears inherited setgid bits and verifies `DEBIAN/` is mode `0755` immediately before `dpkg-deb`.
- Synchronized Cargo, Cargo.lock, Snap, AppStream, and release-note metadata to `0.7.0-beta.2`.

### Release focus

- No new large feature work; this Beta remains under feature freeze.
- Primary goal is package reliability, upgrade/remove safety, reproducible artifacts, and compatibility verification.

## 0.7.0-beta.1 — Beta Preparation Candidate

- Added a one-command Beta gate covering verification, GUI startup, Debian build/validation and staged lifecycle tests.
- Added isolated 1024×600 GTK startup smoke testing under Xvfb and a temporary HOME/D-Bus session.
- Added a real Debian lifecycle smoke that upgrades from a legacy fixture, verifies obsolete extension payload removal, checks the installed CLI version, and removes the package.
- Added staged manual install/upgrade/uninstall testing via `LINUXCARE_DESTDIR` without touching the host system.
- Added a dedicated privilege/security regression suite and wired it into `verify.sh`.
- Replaced GNOME extension command-line `gdbus`/shell spawning with typed `Gio.DBus` and fixed AppInfo launch.
- Added `linuxcare --version` and `--help` for headless package verification.
- Added safe-default migration compatibility for older automation and cleanup-policy JSON files.
- Hardened settings-file creation to mode 0600 and removed unsafe HOME=`/` fallback behavior.
- Reduced emoji-heavy control labels and tightened default window sizing for Beta UI polish.
- Extended Ubuntu 24.04 CI and Ubuntu 26.04 preview coverage with GUI/package/lifecycle smoke tests.

## 0.6.0-alpha.1 — Release Hardening Preview

- Added Ubuntu 24.04 release gate, Rust 1.92 MSRV check, and Ubuntu 26.04 preview compatibility CI.
- Added RustSec, CodeQL, Dependabot, package smoke validation, lintian, ShellCheck, AppStream and desktop-entry validation.
- Added deterministic Debian metadata/timestamps and release tag/version validation.
- Added SPDX 2.3 Cargo SBOM generation and GitHub `actions/attest@v4` provenance/SBOM attestations.
- Removed deprecated GNOME extension-local `version` metadata.
- Added structured GitHub bug reporting guidance with log sanitization reminders.


## 0.5.0-alpha.1 — Deep Network & Desktop Integration Preview

- Added live Deep Network socket ownership grouped by process/PID using `ss`, without persisting endpoint history.
- Added truthful eBPF readiness diagnostics for bpffs, cgroup v2, optional bpftool, and unprivileged-BPF policy.
- Kept per-process network bandwidth intentionally unavailable until a real eBPF/cgroup accounting backend exists.
- Upgraded Storage Intelligence to 3.0 with longer meaningful baselines, daily growth rate, category churn, and confidence levels.
- Added systemd timer Next run / Last trigger visibility plus explicit manual Run Now through the existing hardened user service.
- Added GNOME Integration status for Shell version, extension path, metadata compatibility, and enabled state.
- Added explicit user-level enable/disable controls for LinuxCare Vitals through `gnome-extensions`.
- Added top-bar refresh interval selection (1/2/5/10 seconds).
- Reworked GNOME top-bar labels from emoji-heavy rendering to compact CPU/TMP/RAM/NET/BAT text.
- Top-bar network throughput now prefers the IPv4 default-route interface instead of summing Docker/veth traffic, with a non-loopback fallback.
- Top-bar battery discovery now reads Battery-type power-supply devices instead of assuming only `BAT0`.
- Fixed the extension stylesheet class mismatch so current panel labels actually receive LinuxCare styling.
- Removed the stale MIT license claim from the About dialog because the repository does not currently ship an MIT license file.
- Extended verification diagnostics for ss, bpftool, and gnome-extensions availability.

### Safety / correctness
- Deep Network does not capture packet payloads and does not persist remote endpoint metadata.
- Hidden socket ownership stays unknown rather than being attributed to a guessed PID.
- eBPF readiness does not load or attach any BPF program.
- GNOME integration changes only LinuxCare's own extension enabled state and never restarts the Shell.
- Manual maintenance Run Now does not bypass the unprivileged systemd service boundary.

### Verification note
- Run `cargo fmt` and `./scripts/verify.sh` on the Ubuntu development host before tagging.

## 0.4.0-alpha.1 — Automation & Deep Care Preview

- Replaced preview-only automation with a real user-level `systemd` timer.
- Added `linuxcare-maintenance` headless scheduled runner.
- Added Scan only, Scan + notify, and strictly user-space SAFE Quarantine automation modes.
- Added local Notification Center with desktop delivery and anti-spam rate limiting.
- Added Safe Cleanup 2.0 policy: configurable Undo retention, Quarantine cap, automation budget, and optional expired-item auto-purge.
- Added Storage Intelligence 2.0 category-growth analysis based on Smart Scan history.
- Extended Process Intelligence persistence from one hour to six hours and 30-minute historical vitals.
- Critical process anomalies can now create rate-limited LinuxCare notifications.
- Packaging installs the scheduled-maintenance runner alongside the GUI.


## 0.3.0-alpha.1 — 2026-08-30

### Added
- Maintenance Center with prioritized, explainable recommendations across storage, services, packages, network, filesystems, thermal state, battery and boot.
- Health Score 2.0 domain scores with unknown-domain exclusion from the weighted overall score.
- Service Doctor backend and UI for failed/restarting systemd units plus safer user-session controls.
- Package Doctor 2.0 duplicate APT source detection, cached metadata age, additional versioned kernel inventory and foreign-architecture context.
- Network Doctor 3.0 IPv4/IPv6 routes, MTU/interface error/drop counters and an explicit gateway/DNS active probe.
- Filesystem Doctor for mount capacity, inode pressure, read-only state and current-boot filesystem error context.
- Read-only Btrfs/Snapper/Timeshift awareness.
- Thermal & Power Doctor with hwmon threshold context, CPU frequency/governor, power profile and throttle counters.
- Battery Lab 2.0 bounded private history, capacity-health trend and recent discharge-power context.

### Safety / correctness
- Local Network Doctor refresh never performs the external DNS query; active probing requires an explicit user click.
- Service and filesystem diagnostics keep unavailable scopes/journal access as Unknown/Partially checked rather than inventing a healthy result.
- Snapshot awareness performs no create/delete/restore/prune mutation.
- Package Doctor still does not run apt update or edit repository definitions automatically.
- Maintenance recommendations explain why an action exists and do not execute automatically.

### Verification note
- This source candidate requires `cargo fmt` and `./scripts/verify.sh` on the Ubuntu development host before tagging.


## 0.2.0-alpha.9 — 2026-08-30

### Added
- Safety Quarantine for unprivileged allowlisted cleanup targets with a 30-minute undo window.
- Crash-recoverable quarantine manifest persisted before filesystem detachment.
- Safety & Maintenance Timeline page with active quarantine cards, Undo, two-stage permanent purge, and bounded local Scan/Cleanup/Undo/Purge events.
- Separate accounting for bytes actually freed, bytes protected for Undo, bytes restored, and scan-observed cleanup candidates.
- Automatic expired-quarantine housekeeping while LinuxCare is running and on the next application launch.
- Unit tests covering quarantine/restore round-trip and restore conflict refusal.

### Changed
- User-space cleanup execution now moves eligible directories to quarantine instead of recursively deleting them immediately.
- Smart Cleaner preview explicitly distinguishes undoable bytes from operations that may reclaim space immediately.
- Cleanup completion UI no longer calls quarantine bytes “recovered”; actual freed and protected values are reported separately.
- Sidebar History entry is now **Safety & Timeline** while legacy cleanup history remains visible for backward compatibility.
- Top-level cache scanning skips LinuxCare internal quarantine directories.

### Safety / correctness
- Undo refuses to overwrite a non-empty directory recreated by an application after cleanup.
- Quarantine uses same-filesystem `renameat2(RENAME_NOREPLACE)` semantics; data is not copied through a generic temporary directory.
- Manual purge is permanent and requires a second click within five seconds.
- Privileged APT/Snap/journal and provider cleanup remains non-undoable and is never advertised as reversible.
- Quarantined data still consumes storage until purge; LinuxCare does not count it as freed space.
- Quarantine and timeline manifests are stored under private LinuxCare state directories with restrictive permissions.

### Verification note
- Run `cargo fmt` and `./scripts/verify.sh` on the Ubuntu development host before tagging alpha.9.

## 0.2.0-alpha.8 — 2026-08-30

### Added
- Hardware Doctor page with DMI identity, CPU topology, memory availability, DRM GPU/driver inventory, physical storage inventory, and bounded hwmon temperature discovery.
- Explicit SMART/NVMe health workflow backed by a dedicated `read-storage-health` Polkit action and typed D-Bus helper method.
- Normalized SMART JSON parser for ATA/SATA/SCSI/NVMe health without retaining drive serial numbers or WWNs.
- NVMe critical-warning, endurance-used, media-error, unsafe-shutdown, temperature, power-on-time and I/O-unit visibility when reported by smartctl.
- ATA reallocated/pending/offline-uncorrectable sector and SMART error-log counters when reported.
- Unit tests for NVMe/ATA SMART JSON normalization and serial-data exclusion.

### Changed
- Storage benchmark now uses a 256 MiB temporary file and prefers Linux `O_DIRECT` for both sequential write/read measurement.
- When direct I/O is unavailable, the benchmark falls back to synchronized buffered I/O only with successful `POSIX_FADV_DONTNEED` cache-eviction request and reports that mode explicitly.
- Benchmark refuses tmpfs, ramfs, overlay, squashfs and other non-representative virtual-memory/system filesystems, plus low-free-space targets.
- Benchmark UI labels results as filesystem throughput in MiB/s rather than implying raw SSD/device performance.
- Debian package recommends `smartmontools` for the authenticated storage-health workflow.

### Safety / correctness
- SMART is never queried on application startup; the user must explicitly request it.
- The helper validates a block-device basename against `/sys/block`, constructs `/dev/<name>` itself, uses a fixed smartctl argument set, clears the environment, applies an eight-second timeout, and asks smartctl not to wake standby drives when supported.
- smartctl non-zero status is not automatically treated as command failure because smartctl uses an exit-status bitmask for health findings; valid JSON is parsed and classified instead.
- Raw smartctl JSON is normalized inside the root helper before crossing D-Bus, preventing serial/WWN/raw-output disclosure to the GTK process.
- No SMART self-test, firmware operation, raw write, block-device benchmark, or arbitrary hardware command is exposed.

### Verification note
- Run `cargo fmt` and `./scripts/verify.sh` on the Ubuntu development host before tagging alpha.8.

## 0.2.0-alpha.7 — 2026-08-30

### Added
- Dedicated **Process Intelligence** workflow in the sidebar.
- Per-process CPU sampling from `/proc/<pid>/stat` with PID-reuse protection using process start ticks.
- Per-process RSS memory plus read/write disk-I/O rate sampling from `/proc/<pid>/status` and `/proc/<pid>/io`.
- Historical Vitals for system CPU, RAM, network throughput, and block-device I/O with a 10-minute summary.
- Bounded local telemetry history: one hour maximum, persisted every 15 seconds, with up to 24 process records per persisted sample.
- Baseline-gated anomaly detection for CPU spikes, large memory growth, process disk-I/O spikes, and system CPU/network/disk spikes.
- “Observed for …” context so anomaly cards report when LinuxCare first saw the condition in the retained window.
- Unit tests for baseline gating, process CPU-spike detection, and memory-growth detection.

### Changed
- Sidebar adds Process Intelligence directly after System Vitals.
- Version/AppStream/Snap/Debian metadata synchronized to alpha.7.
- Container and Network Doctor features from alpha.6 remain read-only and unchanged.

### Safety / correctness
- Process telemetry is local-only and persisted with private state permissions.
- LinuxCare does not persist command lines or accessed file paths in Process Intelligence history.
- Per-process network byte attribution is intentionally **not** inferred from `/proc/<pid>/net`; network anomalies remain system-level until an eBPF/cgroup-backed implementation exists.
- Anomaly detection stays in Learning mode until enough time-series samples span at least one minute, preventing first-run spikes from being labelled abnormal.

### Verification note
- Run `cargo fmt` and `./scripts/verify.sh` on the Ubuntu development host before tagging alpha.7.

## 0.2.0-alpha.6 — 2026-08-30

### Added
- Network Doctor 2.0 with read-only NetworkManager connectivity, default-route, DNS, interface, firewall-context and listener analysis.
- Diagnostic notes for missing/multiple default routes, missing DNS, captive portal, limited connectivity and offline state.
- Docker/Podman Analyzer with independent engine detection, bounded CLI queries and storage-category summaries.
- Docker JSON-lines and Podman JSON parsers for image/container/volume usage and reclaimable estimates.
- Unit tests for default-route/NMCLI parsing and Docker/Podman storage parsing.

### Changed
- Sidebar adds Network Doctor and Containers workflows while keeping Security & Firewall as a separate socket/firewall audit.
- AppStream/Snap/Debian/version metadata synchronized to alpha.6.
- Battery runtime validation uses Rust 1.93 Clippy-preferred inclusive-range syntax.

### Safety / correctness
- Network Doctor performs no route, DNS, firewall or interface mutation.
- Container Analyzer exposes no prune/remove/stop action.
- Reclaimable container storage is explicitly an estimate.
- Network-bound listeners remain reachability indicators, not claims of public Internet exposure.

## 0.2.0-alpha.5 — 2026-08-30

### Added
- Package Doctor read-only diagnostics using fixed `apt-get check`, `dpkg --audit`, `apt-mark showhold`, and simulated APT upgrade/autoremove commands.
- Package Doctor diagnostic signals for dependency consistency, dpkg audit state, held packages, autoremove candidates, cached-metadata upgrade candidates, active APT source entries, and cache size.
- Dedicated Battery Lab page with multi-battery discovery through `/sys/class/power_supply`.
- Battery health/wear estimate from matching full/design capacity units, plus cycle count, current/full/design energy, voltage, current, power draw, status, runtime estimate, technology/scope, and charge-threshold visibility when exposed by the kernel.
- Unit tests for APT simulation parsing, deb822 source counting, battery charge-to-energy conversion, capacity-health ratio behavior, and runtime estimation.

### Changed
- Existing Packages page is now Package Doctor; destructive-capable APT maintenance remains visually and architecturally separate from diagnostics.
- Package diagnostic failures are represented as Unknown instead of being misreported as zero candidates.
- System Vitals now reuses the central battery collector, removing the former BAT0/BAT1/BAT2-only battery reader.
- Sidebar adds Battery Lab and renames Packages to Package Doctor.
- Version metadata and release documentation synchronized to alpha.5.

### Safety / correctness
- Package Doctor never runs `apt update`, never probes repository network reachability automatically, and never repairs package state without a separate privileged action.
- Battery health never mixes `energy_*` values with `charge_*` values. Charge-based Wh estimates use available voltage only for display/runtime context.
- Charge thresholds are read-only in alpha.5.

### Verification note
- Alpha.3 is the last native build explicitly reported green by the Ubuntu development host in this conversation. Alpha.5 must pass `cargo fmt` plus `./scripts/verify.sh` before tagging.

## 0.2.0-alpha.4 — 2026-08-30

### Added
- Boot Doctor page with asynchronous `systemd-analyze` timing, blame, critical-chain, and failed-service collection.
- Boot phase breakdown for firmware, loader, kernel, userspace, graphical target, and total startup duration.
- Conservative boot classifications and review guidance; no automatic service disabling is introduced.
- Storage Runway panel inside Storage Analyzer with persistent root-disk history.
- Linear growth trend estimation and 80/90/95% capacity runway forecasts.
- Forecast guardrails requiring at least three samples spanning 12 hours and meaningful positive growth before displaying a fill estimate.
- Unit tests for systemd duration/blame parsing and storage-growth regression behavior.

### Changed
- Sidebar now exposes Boot Doctor as a dedicated diagnostics workflow.
- Storage Analyzer now leads with capacity pressure and trend context before directory analysis.
- Version metadata and release documentation synchronized to alpha.4.

### Verification note
- Alpha.3 was verified successfully with the native Rust/GTK toolchain on the Ubuntu development host. Alpha.4 still requires the same `./scripts/verify.sh` pass after these new modules are applied.

## 0.2.0-alpha.3 — 2026-08-30

### Added
- Conservative 0–100 System Health Score based only on verifiable signals.
- Dashboard Health Intelligence panel with asynchronous refresh.
- Persistent lightweight health snapshots and roughly week-old baseline comparison.
- `What Changed?` reporting for disk usage, kernel, failed services, firewall state, and network-bound listener counts.
- Smart Scan aggregate snapshots and per-category growth/reduction comparisons.
- Local state hardening: atomic writes, `0700` state directory, `0600` snapshot files, bounded history retention.
- Unit tests for healthy/pressured scores and kernel-change detection.

### Changed
- Dashboard constructor now separates Dashboard-only Health Intelligence from the Smart Cleaner view instead of duplicating the panel.
- Smart Scan completion records a comparison baseline and refreshes the shared Dashboard intelligence callback even when the scan starts from Smart Cleaner.
- Security HUD now represents UFW state as active/inactive/unknown instead of treating an unavailable status as disabled.
- Listening-port deduplication now preserves a network-bound bind when the same protocol/port also has a loopback bind.
- `scripts/verify.sh` now runs static/security checks first and reports missing Rust/native GTK build prerequisites explicitly.
- Automation preview version text and package metadata synchronized to alpha.3.

### Verification note
- Static security/metadata/syntax checks pass in the preparation environment. A real Rust/GTK compile remains mandatory before release tagging because that environment cannot install or run the required Rust/GTK toolchain.


## 0.2.0-alpha.2 — 2026-08-30

### Security / correctness
- Removed forced kernel `drop_caches` / misleading "RAM Boost" from the GUI, privileged helper, Polkit policy and GNOME Shell extension.
- Added a dedicated Polkit action for APT autoremove instead of reusing APT-cache authorization.
- Changed `/var/crash` to review-only and added `~/.local/share/coredump` to the exact user cleanup allowlist.
- Added explicit second-click confirmation for process termination and APT autoremove.
- Stopped treating a zero provider byte result as if the estimated candidate size had been actually recovered.
- Reworded externally-bound sockets from `PUBLIC` to `NETWORK` because firewall/router policy determines real reachability.

### Fixed
- Unified GTK and GNOME Shell settings on `$XDG_CONFIG_HOME/linuxcare/config.json`.
- Removed the false `SMART verified` claim.
- Reworked the disk benchmark to use a 128 MiB file and request page-cache invalidation before read measurement.
- Replaced nonfunctional automation switches with explicit preview-only cards.
- Debian package now ships the application icon and GNOME Shell extension like the manual installer.
- Alpha GitHub tags now publish as prereleases and release artifacts receive build provenance attestations.
- Snap metadata is marked `grade: devel` instead of stable.
- Documentation/version metadata synchronized to `0.2.0-alpha.2`.

## 0.2.0-alpha.1 — 2026-08-30

### Added
- Interactive **Storage Analyzer Page** in sidebar with multithreaded directory space breakdown.
- Targeted **Browser & Application Cache Scanner** (`AppCacheScanner`) covering Firefox, Chrome, Brave, Chromium, Spotify, Telegram, Discord, and VS Code.
- System **Crash Reports & Dumps Scanner** (`CrashReportScanner`) covering `/var/crash` and `coredump` stores.
- D-Bus Helper method `AutoremovePackages()` allowing safe removal of orphaned package dependencies.

## 0.1.0-alpha.3 — 2026-08-30

### Added
- Pre-flight helper health check & status banner (`GetVersion` method on D-Bus helper interface).
- Unused Flatpak runtime analysis and safe removal provider via `flatpak uninstall --unused`.
- Journal retention policy selector supporting 7, 30, and 90-day retention policies with explicit review warnings.
- Structured cleanup operation results (`CleanupStatus`, `CleanupOperationResult`) and detailed completion metrics.
- Exportable cleanup report breakdown.
- Enhanced persistent history storing version, operation details, estimated vs recovered storage, and privilege usage.
- Debian packaging script (`scripts/build-deb.sh`) generating `linuxcare_0.1.0-alpha.3_amd64.deb`.
- GitHub Actions CI workflow (`.github/workflows/ci.yml`) for Ubuntu 24.04.

### Fixed
- Fixed Libadwaita color scheme mapping for `Appearance::System` to use `adw::ColorScheme::Default`.

## 0.1.0-alpha.2 — 2026-08-30

### Added
- Typed provider layer for package, Snap, and journal maintenance.
- System D-Bus privileged helper with three allowlisted methods.
- Per-operation Polkit authorization against the caller's unique system-bus name.
- Real APT cache cleanup, disabled Snap revision removal, and journal vacuuming.
- Revalidation of disabled Snap revisions immediately before removal.
- Explicit cleanup selection: SAFE defaults on, REVIEW defaults off.
- Background cleanup event pipeline so authentication and system tools do not block GTK.
- D-Bus activation, hardened systemd unit, install/uninstall scripts, and verification script.

### Security
- No arbitrary command or path API across the privileged boundary.
- Fixed absolute privileged executables with validated/allowlisted arguments.
- Journal retention limited to 7, 30, or 90 days.
- D-Bus send policy narrowed to the LinuxCare helper interface.

### Compatibility
- Rust MSRV raised to 1.92 to match the current `gtk4 0.11.4` crate metadata.
