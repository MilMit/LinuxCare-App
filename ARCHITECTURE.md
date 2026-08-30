# LinuxCare Architecture — 0.7.0-beta.1

## Boundary

```text
GTK4 / Libadwaita UI (normal user)
        |
        +----> read-only diagnostics / scanners
        |
        +----> exact user allowlist cleanup
        |
        +----> typed providers
                    |
                    v
              system D-Bus
                    |
                    v
         net.milmit.LinuxCare.Helper
              (root, small API)
                    |
                    v
                 Polkit
           /        |        \
      apt-get      snap    journalctl / smartctl
```

The GUI is never elevated.

## Typed cleanup model

`CleanupCandidate` separates risk from executability. Execution is one of:

- `UserAllowedDirectory`
- `PrivilegedProvider`
- `FlatpakUnused`
- `AnalysisOnly`

Privileged actions are typed; no string command exists in the domain model.

## Helper methods

```text
GetVersion() -> version
CleanAptCache() -> recovered_bytes
AutoremovePackages() -> recovered_bytes (zero when no reliable byte total exists)
RemoveDisabledSnapRevision(name, revision) -> recovered_bytes
VacuumJournal(keep_days) -> recovered_bytes
ReadStorageHealthJson(device_basename) -> normalized_health_json
```

There is no `DropCaches`, `ExecuteCommand`, `RunShell`, generic package remover, or root path API.

## Revalidation and truthfulness

- Snap revision state is re-queried immediately before mutation.
- journal retention only accepts 7/30/90 days.
- APT autoremove receives no package list from the GUI; APT computes its own current candidate set.
- `/var/crash` is analysis-only; user coredumps use an exact user-space allowlist.
- a provider that cannot measure reclaimed bytes returns zero; estimates are kept separate from measured results.
- SMART device names are validated against `/sys/block`; raw smartctl output is normalized inside the helper before D-Bus return.
- the storage benchmark prefers aligned `O_DIRECT` I/O; its fallback requires synchronized writes and a successful page-cache eviction request before read timing.

## Settings

GTK and the GNOME Shell extension share:

```text
$XDG_CONFIG_HOME/linuxcare/config.json
```

(or `~/.config/linuxcare/config.json` when XDG_CONFIG_HOME is unset).

## Automation

Automation uses a generated hardened `systemd --user` oneshot service/timer. Scheduled maintenance remains unprivileged and can only scan, notify, or move already-safe user-space candidates into Safety Quarantine. Privileged provider operations never run unattended.


## Health Intelligence (alpha.3)

`src/health.rs` is a pure system-analysis/state layer; GTK rendering remains in `src/app.rs`. Health collection runs on a worker thread so calls to `systemctl`, `ufw`, and `ss` do not block the GTK main loop.

The score is deliberately conservative: unknown signals have zero penalty. Network-bound listeners are informational and are not treated as internet-exposed ports. Memory pressure uses `/proc/meminfo` `MemAvailable`, not a simplistic "free RAM" metric.

Two local histories are stored under XDG state:

- `health-snapshots.json`: lightweight system snapshots, rate-limited to one sample every six hours and capped at 120 samples.
- `scan-snapshots.json`: one aggregate snapshot per completed Smart Scan, capped at 60 scans.

`What Changed?` compares the current system with a roughly week-old baseline when available (otherwise the oldest local baseline) and compares the two most recent completed Smart Scans. State files are written atomically with user-only permissions.


## Boot Doctor (alpha.4)

`src/boot.rs` is a read-only systemd diagnostics layer. It invokes fixed commands without a shell and with `LC_ALL=C` for parse stability:

- `systemd-analyze time --no-pager`
- `systemd-analyze blame --no-pager`
- `systemd-analyze critical-chain --no-pager`
- `systemctl --failed --type=service --no-legend --plain --no-pager`

Collection runs on a worker thread; GTK only renders the returned typed report. Boot Doctor does not cross the privileged D-Bus boundary and cannot stop, disable, mask, or edit units.

## Storage Runway (alpha.4)

`src/storage_runway.rs` stores bounded root-filesystem samples under XDG state. Samples are rate-limited to one every six hours, capped at 180 entries, and analysis uses at most the most recent 30 days.

Forecasting is deliberately gated: at least three samples spanning twelve hours are required, and only meaningful positive growth can produce an 80/90/95% fill estimate. A linear regression is used to reduce sensitivity to one noisy sample. The UI always describes these values as trend forecasts rather than guarantees.


## Package Doctor (alpha.5)

`src/package_doctor.rs` is a read-only package-analysis layer. It executes fixed binaries directly with `LC_ALL=C` and no shell: `/usr/bin/apt-get check`, `/usr/bin/dpkg --audit`, `/usr/bin/apt-mark showhold`, and `apt-get -s` simulations for upgrade/autoremove candidate visibility. Active APT source entries are parsed locally from `.list` and deb822 `.sources` files.

The diagnostic layer never runs `apt update`, never writes package state, and never crosses the privileged D-Bus boundary. Existing APT cache cleanup/autoremove operations remain separate provider actions; autoremove receives no package list from GTK and APT computes the current set again inside the helper.

## Battery Lab (alpha.5)

`src/battery.rs` reads Linux power-supply sysfs directly and returns typed battery devices. Discovery is based on each supply's `type=Battery`, not hard-coded `BAT0` naming. `src/modules/battery_lab.rs` renders the report asynchronously.

Capacity health uses only matching-unit pairs: `energy_full / energy_full_design` or `charge_full / charge_full_design`. Runtime estimates are explicitly instantaneous estimates based on current power/current, and unavailable kernel attributes remain Unknown. No battery setting or firmware threshold is mutated in alpha.5. System Vitals reuses this collector for its compact battery card.


## Alpha.6 network and container diagnostics

`network_doctor.rs` is a read-only collector. It consumes NetworkManager (`nmcli`) status, kernel route data (`ip route`), resolver metadata (`resolvectl` with `/etc/resolv.conf` fallback), and the existing Security HUD listener audit. `modules/network.rs` renders those signals without exposing configuration mutations.

`container_analyzer.rs` treats Docker and Podman as optional independent providers. It uses bounded read-only CLI calls to engine `system df`/`info` commands, parses structured output, and returns storage categories to `modules/containers.rs`. No prune API exists in this release.

## Process Intelligence telemetry

`src/process_intelligence.rs` samples Linux `/proc` and `/proc/diskstats` without privilege. A process identity is `(pid, start_ticks)` so PID reuse does not merge unrelated processes. The sampler keeps fast in-memory readings for UI refresh, persists a reduced snapshot every 15 seconds, and retains at most six hours of local history.

Persisted process data is intentionally narrow: process name, PID/start identity, CPU, RSS memory and disk-I/O rates. Command lines and accessed file paths are not stored. System network throughput is recorded, but LinuxCare does not infer per-process network byte ownership from network-namespace `/proc/<pid>/net` data.

Anomaly detection is baseline-gated and read-only. No process is automatically terminated or throttled based on an anomaly.


## Hardware Doctor and SMART boundary (alpha.8)

`src/hardware.rs` owns unprivileged hardware inventory and normalized SMART parsing. `src/modules/hardware.rs` renders inventory immediately but only requests low-level drive health after an explicit user action.

SMART access crosses the existing system D-Bus helper through one typed method:

```text
ReadStorageHealthJson(validated_block_device_basename) -> normalized_json
```

The helper validates the basename against `/sys/block`, constructs the `/dev` path itself, and invokes a fixed smartctl JSON query under a bounded timeout. Raw smartctl output is parsed inside the helper with the shared normalized parser; identifiers such as serial numbers and WWNs are not included in the returned `StorageHealthReading`.

The storage speed test remains unprivileged and file-based. It prefers aligned `O_DIRECT` sequential I/O, falls back to synchronized buffered I/O only when cache eviction can be requested successfully, and refuses filesystem types that would make the result clearly non-physical. It is intentionally described as filesystem throughput rather than raw block-device performance.

## Safety Quarantine and maintenance event flow (alpha.9)

User-space candidates with `ExecutionKind::UserAllowedDirectory` no longer enter the irreversible directory-removal path during normal Smart Cleaner execution. The executor calls `quarantine::quarantine_candidate`, which persists intent and then atomically renames each allowlisted directory to a hidden sibling on the same filesystem. The well-known cache/trash directory is recreated immediately so applications can continue operating.

Quarantine records live under `XDG_STATE_HOME/linuxcare/quarantine.json` (fallback `~/.local/state/linuxcare/quarantine.json`). The actual protected data remains beside its original directory under an internal `.linuxcare-quarantine-*` name, avoiding cross-filesystem copy semantics. Restore succeeds only when the recreated original path is absent or still an empty, user-owned real directory. If it contains new application data, restore is refused rather than overwriting it.

The Undo retention window is a configurable purge-eligibility policy (30 minutes, 1 hour by default, 1 day, or 7 days), not a claim that the data is already gone. Expired records are purged only when automatic expiry is enabled, either while LinuxCare is running, at launch, or during a scheduled maintenance run. Manual purge uses the same no-symlink-following removal boundary and is permanent. Quarantine also has a configurable total capacity cap and a separate unattended-cleanup budget.

`timeline.rs` maintains a separate bounded local event stream for Smart Scan, Cleanup, Restore, and Purge. It deliberately stores actual-freed, protected, restored, and observed-candidate byte counts as different fields. Legacy `history.json` remains readable for backward compatibility.



## Consolidated Intelligence & System Doctor architecture (0.4.0-alpha.1)

The 0.3 line groups diagnostics by workflow instead of adding one sidebar page per collector. `maintenance_intelligence.rs` is an orchestration/read-model layer: it calls typed, read-only collectors and produces explainable recommendations. It never executes the suggested action.

Health Score 2.0 keeps eight independent domains: Storage, Performance, Reliability, Security, Network, Packages, Thermal, and Battery. Each domain is `Option<u8>`; unavailable domains are excluded from the weighted denominator rather than converted into a failure or a perfect score. Detailed diagnostic pages remain the source of context; the dashboard score is a compact summary, not a replacement for evidence.

New typed collectors are split from GTK rendering:

- `service_doctor.rs`: system/user systemd unit state, failed/restarting units, and unit-file counts.
- `filesystem_doctor.rs`: `/proc/self/mountinfo`, `statvfs`, bounded current-boot filesystem-error evidence, and read-only snapshot awareness.
- `thermal_doctor.rs`: hwmon temperatures, cpufreq context, power profile, and exposed throttle counters.
- `network_doctor.rs`: local NetworkManager/route/DNS/interface state; external DNS/local-gateway probes exist only behind an explicit UI action.
- `package_doctor.rs`: fixed read-only apt/dpkg simulations and repository metadata analysis.
- `battery.rs`: live power-supply data plus bounded private trend history.

The three new top-level workflows are Maintenance Center, Filesystem Doctor, and Thermal & Power. Existing Service, Package, Network, Battery, and Dashboard workflows are upgraded in place to keep navigation bounded.


## User-level automation boundary (0.4.0-alpha.1)

Automation is implemented with a generated `systemd --user` oneshot service and timer, not a permanent daemon. The scheduled runner uses the same unprivileged scanner set but filters unattended mutations to `SAFE + UserAllowedDirectory` candidates only. The generated service includes `NoNewPrivileges=true`, read-only host/home protection plus narrow user write paths, and never uses the privileged D-Bus helper. Polkit-backed APT, Snap, journal, SMART and other privileged operations remain interactive-only.

The scheduled runner writes Smart Scan/Timeline/Notification state through private XDG directories. If the package binary is removed, `ConditionPathIsExecutable` prevents the generated service from attempting a missing executable.


## Deep Network and GNOME integration boundary (0.5.0-alpha.1)

`deep_network.rs` provides a live-only socket ownership read model using the fixed `ss -H -t -u -n -a -p` invocation. LinuxCare groups visible ownership by process/PID and counts distinct remote endpoints, but endpoint strings are not written to telemetry/history. Ownership hidden by kernel permissions remains Unknown.

eBPF support in this release is readiness detection only: bpffs mount state, cgroup v2, optional `bpftool`, and `/proc/sys/kernel/unprivileged_bpf_disabled`. LinuxCare does **not** load, attach, pin, or persist BPF programs and therefore does not expose per-process upload/download byte claims.

`gnome_integration.rs` reads the current GNOME Shell major version and LinuxCare Vitals metadata, then optionally calls `gnome-extensions enable|disable` for LinuxCare's own UUID. It does not restart GNOME Shell, edit unrelated extensions, or cross the privileged helper boundary. GTK and the Shell extension continue sharing only the narrow top-bar settings object in `config.json`.

Storage Intelligence 3.0 remains separate from Storage Runway: Runway forecasts root-filesystem capacity pressure, while Storage Intelligence compares bounded Smart Scan category snapshots using meaningful elapsed baselines, rate/confidence, and category churn.

## Release engineering boundary (0.6.0-alpha.1)

Ubuntu 24.04 is the required release build environment. Rust 1.93 is pinned for formatting/lint consistency and Rust 1.92 remains an explicit MSRV compatibility check. Ubuntu 26.04 runs as a non-blocking compatibility job while its GitHub-hosted image is preview. Release artifacts include deterministic Debian metadata, SHA256 checksums, an SPDX Cargo dependency SBOM, and GitHub provenance/SBOM attestations.

## Beta lifecycle and compatibility boundary (0.7.0-beta.1)

The runtime architecture is intentionally unchanged from the 0.6 hardening line. Beta preparation adds testable lifecycle seams around it:

```text
source/security gate
      ↓
native Rust/GTK gate
      ↓
1024×600 GUI startup smoke (isolated HOME + D-Bus)
      ↓
Debian build + lint/metadata validation
      ↓
staged install/upgrade/uninstall
      ↓
real legacy-package upgrade/install/remove smoke on ephemeral CI
```

The manual system installer accepts `LINUXCARE_DESTDIR` only for staging/tests; without it the installer still requires root. The staging path does not call host `systemctl` or update the host icon cache.

Persisted automation and cleanup-policy structs use safe defaults for missing fields so older partial JSON remains loadable as the Beta schema evolves.

