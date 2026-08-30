# LinuxCare 0.7.0-beta.1 — Public Beta

LinuxCare 0.7.0-beta.1 is the first Beta release candidate of the safety-first Linux maintenance suite for Ubuntu/GNOME. This release freezes the current feature set and focuses on verification, packaging, upgrade safety, privilege separation, and reproducible release artifacts.

## Highlights

- Safety-first Smart Cleaner with user-space Safety Quarantine, Undo, bounded retention, and Maintenance Timeline.
- System Health Score 2.0 and explainable Maintenance Center recommendations.
- Live Vitals, Process Intelligence, historical trends, and anomaly detection.
- Boot Doctor, Service Doctor, Package Doctor 2.0, Filesystem Doctor, Thermal & Power Doctor, Battery Lab 2.0, Hardware Doctor, and authenticated SMART/NVMe health.
- Network Doctor 3.0 plus Deep Network socket ownership and truthful eBPF readiness diagnostics.
- Storage Runway, Storage Intelligence, guarded filesystem throughput benchmark, Docker/Podman storage analysis, and read-only Btrfs/Snapper/Timeshift awareness.
- Real user-level systemd maintenance automation with optional desktop notifications.
- GNOME Shell top-bar extension with read-only live metrics and typed D-Bus power-profile control.

## Safety model

LinuxCare keeps the GTK application unprivileged. System-level operations are exposed through a narrow D-Bus/Polkit helper rather than a generic root shell. User-space cleanup eligible for Undo is moved to same-filesystem quarantine and is not counted as freed space until it is permanently purged.

Scheduled maintenance never performs privileged APT, Snap, Flatpak, journal, firewall, SMART mutation, or package-removal actions in the background. Deep Network does not capture packet payloads and does not persist remote endpoint history.

## Beta verification status

The project Beta gate has completed successfully on the development Ubuntu host, including Rust formatting, `cargo check`, Clippy with warnings denied, unit tests, development build, isolated 1024×600 GUI startup, static/security validation, package staging, and staged install/upgrade/uninstall lifecycle checks.

Before publishing a binary release, run the real root-level Debian install/remove smoke on an ephemeral VM or disposable test host:

```bash
sudo ./scripts/smoke-deb-install.sh
```

## Optional runtime integrations

LinuxCare remains functional when optional tools are absent, but some diagnostics require them:

- `smartmontools` — authenticated SMART/NVMe health.
- `libnotify-bin` — desktop notification delivery.
- `bpftool` — richer eBPF readiness diagnostics; LinuxCare does not attach BPF programs in this Beta.
- `gnome-extensions` — explicit enable/disable management of the LinuxCare Vitals extension.

## Known Beta limitations

- Per-process network bandwidth is intentionally unavailable until a real eBPF/cgroup accounting backend exists.
- SMART/NVMe health is read-only; LinuxCare does not run self-tests, firmware updates, sanitize, or format operations.
- Reclaimable container-storage figures are estimates and can differ from actual freed bytes because of shared layers and active references.
- Storage Runway requires sufficient history before it forecasts capacity pressure.
- A quarantined cleanup still consumes disk space until purge.
- The Snap definition remains development-oriented; the Full Edition currently relies on host Polkit/D-Bus/systemd integration and is not being presented as a Store-equivalent replacement for the verified Debian package.

## AppStream / store media

Real application screenshots are intentionally not fabricated in this source tree. Before a store-facing release, capture the actual Beta UI and validate the assets with:

```bash
python3 scripts/validate-screenshots.py assets/screenshots
```

AppStream recommends 16:9 screenshots with a width of at least 620 px. The repository includes a screenshot capture checklist and manifest under `assets/screenshots/`.

## Install

The preferred Beta artifact is the Debian package produced by the verified release workflow:

```bash
sudo apt install ./linuxcare_0.7.0-beta.1_amd64.deb
```

The release workflow also produces a binary tarball, deterministic source tarball, SPDX 2.3 SBOM, release manifest, and SHA256 checksums.

## Reporting issues

When reporting a bug, include the LinuxCare version, Ubuntu/GNOME version, reproduction steps, and only sanitized logs. Do not post passwords, tokens, private file contents, or unrelated personal paths.
