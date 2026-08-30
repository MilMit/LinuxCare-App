# LinuxCare Security Model — 0.7.0-beta.1

LinuxCare is destructive-capable software. UI confirmation is useful, but the real security boundary is scope validation plus privilege separation.

## Reporting a vulnerability

Do not open a public issue for a suspected privilege-boundary, path-validation, quarantine, Polkit/D-Bus, or information-disclosure vulnerability. Report it privately to **info@milmit.net** with the LinuxCare version, affected distribution, reproduction steps, and the minimum sanitized logs needed to reproduce the issue. Do not include passwords, tokens, unrelated personal files, or full home-directory dumps.

## Privilege boundary

The GTK GUI runs as the logged-in user. It does not run `sudo`, `pkexec`, a shell, or arbitrary root commands.

The D-Bus-activated root helper exposes only these bounded operations:

- clean APT download cache
- autoremove dependencies that APT itself reports as no longer required
- remove one disabled Snap revision after revalidation
- vacuum system journal data using an allowlisted retention policy
- read normalized SMART/NVMe health for one validated physical block device after explicit authorization

Forced kernel cache dropping was removed in alpha.2.

## Polkit

Every privileged helper operation that requires authorization uses the D-Bus `AllowInteractiveAuth` flag and authorizes the incoming unique system-bus sender. APT cache cleanup, APT autoremove, and low-level storage-health reads use separate Polkit action IDs.

## Argument policy

There are no filesystem path arguments in the privileged API. Snap identifiers and revisions are validated and then rechecked against snapd. Journal retention is restricted to exactly 7, 30, or 90 days. APT operations accept no user-controlled command arguments.

## Process execution

No shell interpreter is used by the privileged helper. Fixed executables are invoked directly with a cleared environment:

```text
/usr/bin/apt-get clean
/usr/bin/apt-get autoremove -y
/usr/bin/snap remove <validated-name> --revision=<validated-revision>
/usr/bin/journalctl --rotate --vacuum-time=<allowlisted-days>days
/usr/bin/timeout 8s /usr/sbin/smartctl -a -n standby,0 -j /dev/<validated-block-device>
```

The smartctl binary may also be `/usr/bin/smartctl` on distributions that install it there. The helper selects only from this fixed allowlist.

## User-space deletion

Unprivileged deletion is restricted to owned, canonical directories in a fixed allowlist, including safe subdirectories below `~/.cache`, Trash directories, developer caches, and `~/.local/share/coredump`.

`/var/crash` is analysis-only in this release.

Cleanup roots are checked for ownership and symlinks, atomically detached with `renameat2(RENAME_NOREPLACE)`, recreated, and only then recursively removed from the detached path.

## Destructive UI actions

APT autoremove and process termination require a second explicit click. A successful provider operation that cannot report reclaimed bytes returns zero; LinuxCare no longer substitutes an estimate and presents it as measured recovery.

## GNOME Shell extension

The extension is intentionally unprivileged. It reads metrics, opens LinuxCare, changes power profile through the system D-Bus service, and stores top-bar preferences in the same `config.json` used by the GTK application. It no longer exposes a RAM cache flush or `pkexec` path.

## System service hardening

The helper remains protected by systemd restrictions such as `NoNewPrivileges`, `PrivateTmp`, `ProtectHome`, `ProtectSystem=full`, `ProtectKernelTunables`, control-group/kernel protections, SUID/SGID restrictions, AF_UNIX-only networking, native syscall architecture, and W^X memory policy.

## Still blocked

Generic command execution, arbitrary path deletion, old-kernel removal, Docker/Podman prune, privileged/system-wide service or startup mutation, volume deletion and scheduled destructive cleanup remain intentionally unavailable.


## Health snapshot privacy (alpha.3)

Health Intelligence stores only local aggregate system metrics required for comparisons: storage usage, RAM totals/availability-derived usage, kernel version, failed-service counts, UFW state, network-bound listener count, and aggregate Smart Scan category sizes. It does **not** store command output, process command lines, socket peer addresses, file names from scan candidates, or file contents.

The files are stored under `XDG_STATE_HOME/linuxcare` (fallback `~/.local/state/linuxcare`), the directory is mode `0700`, snapshot files are mode `0600`, and updates use write+fsync+atomic rename. No telemetry or network upload is performed.


## Boot Doctor safety (alpha.4)

Boot Doctor is read-only. It executes fixed `systemd-analyze`/`systemctl` binaries directly and never through a shell. It exposes no automatic service-disable, stop, mask, or edit action. Slow startup units are presented as investigation targets because duration alone is not proof that a service is unnecessary.

## Storage Runway privacy (alpha.4)

Storage Runway stores only timestamps plus aggregate root-filesystem used/total byte counts. It stores no paths, filenames, file contents, mount credentials, or telemetry. State is kept in the same user-only XDG state root (`0700` directory, `0600` atomic files) and is never uploaded.


## Package Doctor safety (alpha.5)

Package Doctor is diagnostic-first and unprivileged. Fixed APT/dpkg commands are invoked directly, never through a shell. The automatic check does not run `apt update`, contact repositories intentionally, repair dpkg state, install upgrades, or remove packages. A command that cannot complete is represented as Unknown rather than a false healthy result.

APT cache cleanup and autoremove remain behind the typed system D-Bus helper and separate Polkit authorizations. The GTK diagnostic report cannot supply arbitrary package names to the helper.

## Battery Lab privacy and mutation boundary (alpha.5)

Battery Lab reads local kernel attributes under `/sys/class/power_supply` only. It does not persist battery serial numbers or telemetry, upload data, change charge thresholds, change charging policy, or invoke vendor firmware tools. Health is a full-vs-design capacity estimate only when compatible units exist; missing attributes remain Unknown.


## Alpha.6 read-only provider boundary

Network Doctor may execute fixed read-only programs/arguments (`nmcli`, `ip`, `resolvectl`) and reuses LinuxCare's fixed `ss`/UFW audit. It does not accept user-supplied command strings and has no route, DNS, link or firewall mutation method.

Container Analyzer executes only fixed Docker/Podman inspection commands through a six-second `timeout` wrapper. No `prune`, `rm`, `rmi`, volume deletion, container stop/kill, or arbitrary engine command is exposed. Reclaimable byte counts are informational estimates only.

## Process Intelligence privacy boundary

Process Intelligence reads non-privileged `/proc` counters. Its local history intentionally excludes process command lines and file paths. The state directory/file use private Unix permissions where supported and history is bounded to six hours. Per-process network byte attribution is not guessed from `/proc/<pid>/net`; that requires a future, explicitly designed eBPF/cgroup data path.

Anomaly results are advisory only. They never trigger automatic process termination, throttling, firewall changes, or cleanup.

## Hardware Doctor / SMART boundary (alpha.8)

Hardware inventory is read-only and sourced from kernel-exported `/proc` and `/sys` data. Low-level SMART/NVMe access is never automatic and requires the dedicated `net.milmit.LinuxCare.read-storage-health` Polkit action.

The privileged SMART method does not accept a path or command. It accepts only a short block-device basename that must already exist under `/sys/block`; the helper constructs `/dev/<name>` itself. The only external health command is an allowlisted smartctl binary with a fixed JSON/no-wake argument set and an eight-second timeout. No self-test, firmware operation, security command, raw write, sanitize, format, or arbitrary smartctl flag can be requested by GTK.

Raw smartctl JSON remains inside the helper. LinuxCare normalizes only health fields needed by the UI and intentionally excludes serial numbers, WWNs, and raw textual output before returning a D-Bus reply.

The benchmark writes only a temporary file below the user's own cache directory and never opens a raw block device. It refuses low-free-space and clearly non-representative virtual/system filesystems, deletes its temporary file after each direct/fallback attempt, and reports the measurement mode.

## Safety Quarantine / Undo boundary (alpha.9)

Undo is available only for strict user-space allowlisted directories. LinuxCare persists a quarantine transaction before moving data, uses Linux `renameat2(RENAME_NOREPLACE)` on the directory entry, recreates the expected application directory, and never follows a replacement symlink as part of restore/purge. Quarantine paths are internal hidden siblings under the same user-owned filesystem hierarchy.

Undo is intentionally conservative: if the original directory has been repopulated after cleanup, LinuxCare refuses to replace it. This prevents an Undo action from destroying new application state. The protected copy remains available for review or purge.

Quarantine is not free space. Until purge, the bytes remain allocated and are reported separately from `recovered_bytes`. Manual purge is a permanent two-stage action. Privileged helper operations, Flatpak/provider operations, package maintenance, journal vacuuming, and Snap removal are not claimed to support rollback.

The quarantine manifest and maintenance timeline are local state files under a 0700 LinuxCare state directory and are written atomically with 0600 file permissions. Timeline summaries do not need to expose quarantined filesystem paths.



## 0.4.0-alpha.1 diagnostics/recommendation boundary

The consolidated intelligence batch does not add a generic privileged execution path. Maintenance Intelligence consumes reports and emits recommendations only; recommendations cannot execute commands.

- Service Doctor uses fixed `systemctl` read operations for system diagnostics. Interactive controls remain scoped to the explicitly rendered user-session service workflow; no automatic disable/mask policy is introduced.
- Package Doctor 2.0 uses fixed apt/dpkg read/simulation commands. It does not run `apt update`, edit source files, remove kernels, or repair package state automatically.
- Network Doctor 3.0 performs local state collection by default. The only active network probe requires an explicit click and is limited to the current local gateway plus DNS resolution of `example.com`; it does not scan remote hosts or ports.
- Filesystem Doctor reads mount/statvfs/journal evidence and snapshot metadata. It cannot run fsck, create/delete snapshots, or roll back a filesystem.
- Thermal & Power is read-only; it does not write governors, charge thresholds, power profiles, or thermal controls.
- Battery Lab history is local, bounded, and stored under private XDG state. It contains aggregate battery telemetry rather than user content.

Unavailable evidence remains Unknown/Partially checked. The application must not convert permission failure, a missing binary, a missing sensor, or an absent battery into a Healthy result.


## Scheduled maintenance boundary

`0.4.0-alpha.1` adds real background scheduling, but not background privilege. The user-level `linuxcare-maintenance` runner:

- never constructs or calls the privileged D-Bus provider;
- filters automatic cleanup to `CleanupRisk::Safe` plus `ExecutionKind::UserAllowedDirectory`;
- moves eligible data to Safety Quarantine instead of permanently deleting it;
- obeys a separate unattended-cleanup byte budget and the global Quarantine cap;
- runs under a generated user service with `NoNewPrivileges=true`, `ProtectSystem=strict` and `ProtectHome=read-only`;
- never runs APT autoremove/cache cleanup, journal vacuum, disabled-Snap removal, SMART, service mutation, filesystem repair or other root operations unattended.

Desktop notifications are best-effort and local. Notification history contains LinuxCare titles/status summaries only; it does not persist process command lines, file paths, network payloads, or SMART serial/WWN material.


## 0.5.0-alpha.1 Deep Network / desktop-integration boundary

- Deep Network executes a fixed `ss` query only; no packet capture, raw-socket sniffing, payload inspection, or endpoint-history persistence is implemented.
- Process ownership hidden by kernel permissions is reported as unavailable. LinuxCare never guesses a PID from a port or remote endpoint.
- eBPF is capability/readiness detection only in this release. No BPF program is loaded or attached and no elevated BPF permission is requested.
- GNOME integration is user-session only and restricted to `linuxcare-vitals@milmit.net`; enable/disable uses `gnome-extensions` without Shell restart or privileged execution.
- Automation `Run Now` starts the already-installed hardened user service and does not execute the maintenance binary through a shell.
- Remote socket endpoint metadata is rendered live and not added to Process Intelligence history, Timeline, Notification Center, or Smart Scan snapshots.


## 0.7.0-beta.1 regression boundary

Beta verification now fails if generic Shell execution returns to the GNOME extension, if scheduled maintenance references the privileged provider/helper boundary, if the root helper attempts to launch generic interpreters/elevation frontends, if required Polkit actions disappear, if D-Bus activation stops pointing to the fixed helper path, or if installation scripts introduce SUID/SGID payloads.

The GNOME extension now changes Power Profiles using a typed `Gio.DBus` call. It no longer launches a `gdbus` command line, and opening LinuxCare no longer has a command-line spawning fallback.

Manual installer lifecycle tests run against a temporary staging root. Uninstall continues to preserve user configuration/history by design.

## 0.6.0-alpha.1 supply-chain boundary

Release CI audits `Cargo.lock` against RustSec, runs CodeQL for Rust, validates package payload/maintainer scripts, rejects unexpected setuid/setgid payloads, emits a locked-Cargo SPDX SBOM, and uses GitHub artifact attestations for provenance and SBOM association. These controls improve traceability; they are not a claim that an artifact is vulnerability-free.
