<p align="center">
  <img src="data/icons/hicolor/scalable/apps/net.milmit.LinuxCare.svg" width="128" alt="LinuxCare logo">
</p>

<h1 align="center">LinuxCare</h1>

<p align="center">
  Safety-first Linux maintenance, diagnostics, health intelligence, and GNOME integration — built natively with Rust, GTK4, and Libadwaita.
</p>

<p align="center">
  <a href="https://github.com/MilMit/LinuxCare-App/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/MilMit/LinuxCare-App/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/MilMit/LinuxCare-App/actions/workflows/security.yml"><img alt="Security" src="https://github.com/MilMit/LinuxCare-App/actions/workflows/security.yml/badge.svg"></a>
  <a href="https://github.com/MilMit/LinuxCare-App/releases"><img alt="Release" src="https://img.shields.io/github/v/release/MilMit/LinuxCare?include_prereleases&label=release"></a>
  <img alt="Ubuntu / GNOME" src="https://img.shields.io/badge/platform-Ubuntu%20%2F%20GNOME-E95420?logo=ubuntu&logoColor=white">
  <img alt="Rust 1.93" src="https://img.shields.io/badge/Rust-1.93-000000?logo=rust&logoColor=white">
  <img alt="Status: Public Beta" src="https://img.shields.io/badge/status-public%20beta-blue">
</p>

<p align="center">
  <a href="https://github.com/MilMit/LinuxCare-App/releases/download/v0.7.0-beta.1/linuxcare_0.7.0-beta.1_amd64.deb"><strong>⬇ Download .deb</strong></a>
  &nbsp;•&nbsp;
  <a href="https://github.com/MilMit/LinuxCare-App/releases/tag/v0.7.0-beta.1"><strong>All release assets</strong></a>
  &nbsp;•&nbsp;
  <a href="RELEASE_NOTES.md"><strong>Release notes</strong></a>
</p>

## Public Beta

**LinuxCare 0.7.0-beta.1** is the first public Beta of LinuxCare. The feature set is frozen for this release line while testing focuses on package reliability, upgrade safety, privilege separation, compatibility, and real-world bug fixing.

> [!IMPORTANT]
> This is Beta software. Review proposed maintenance actions before applying them, keep normal backups, and report reproducible issues with sanitized logs.

### Download

**Recommended package for Ubuntu/GNOME:**

[Download `linuxcare_0.7.0-beta.1_amd64.deb`](https://github.com/MilMit/LinuxCare-App/releases/download/v0.7.0-beta.1/linuxcare_0.7.0-beta.1_amd64.deb)

All release artifacts, source archives, checksums, SBOM, and release metadata are available on the [v0.7.0-beta.1 release page](https://github.com/MilMit/LinuxCare-App/releases/tag/v0.7.0-beta.1).

### Install

After downloading the Debian package:

```bash
sudo apt install ./linuxcare_0.7.0-beta.1_amd64.deb
```

Launch from the GNOME application menu or run:

```bash
linuxcare
```

### Verify the package

Download `SHA256SUMS.txt` alongside the `.deb`, then run:

```bash
grep 'linuxcare_0.7.0-beta.1_amd64.deb' SHA256SUMS.txt | sha256sum -c -
```

For the complete verification procedure, see [`VERIFY_RELEASE.md`](VERIFY_RELEASE.md).

## What LinuxCare does

| Area | Capabilities |
| --- | --- |
| **Vitals** | Live CPU, RAM, temperature, network, battery, historical trends, and process intelligence |
| **Smart Cleaner** | Risk-aware cleanup, Safety Quarantine, Undo, retention limits, and truthful freed-space accounting |
| **Health Intelligence** | Explainable Health Score 2.0, Maintenance Center recommendations, and local “What Changed?” baselines |
| **System Doctors** | Boot, Service, Package, Filesystem, Thermal & Power, Battery, Hardware, and Storage diagnostics |
| **Network** | Route/DNS/interface diagnostics, listener context, socket ownership, firewall context, and explicit active probes |
| **Storage** | Storage Analyzer, Storage Runway, cleanup growth intelligence, guarded filesystem throughput benchmark, and container storage analysis |
| **Hardware** | DMI/CPU/GPU/storage inventory plus explicit authenticated read-only SMART/NVMe health checks |
| **Automation** | User-level systemd maintenance timers with safe unprivileged modes and optional desktop notifications |
| **GNOME** | LinuxCare Vitals top-bar extension with compact live metrics and power-profile integration |

## Safety model

LinuxCare is intentionally designed so the GTK application **does not run as root**.

System-level operations pass through a narrow D-Bus helper protected by Polkit. The helper exposes fixed typed operations rather than a generic privileged shell. LinuxCare does not use generic `pkexec`, `sh -c`, or `bash -c` execution for maintenance actions.

Additional safety rules include:

- User-space cleanup eligible for Undo is moved into same-filesystem Safety Quarantine.
- Quarantined bytes are **not counted as freed disk space** until permanent purge.
- Scheduled maintenance remains unprivileged and does not silently run privileged APT, Snap, Flatpak, journal, firewall, SMART mutation, or package-removal operations.
- Unknown or unavailable diagnostic data is not silently converted into a healthy zero.
- Network listeners are reported as exposure context, not automatically labelled Internet-public.
- SMART/NVMe checks are explicit, read-only, device-validated, and return normalized health data instead of arbitrary root command output.
- Deep Network does not persist remote endpoint history or capture packet payloads.

See [`ARCHITECTURE.md`](ARCHITECTURE.md) and [`SECURITY.md`](SECURITY.md) for the deeper design and privilege-boundary documentation.

## GNOME Shell extension

The included extension is **LinuxCare Vitals** with UUID:

```text
linuxcare-vitals@milmit.net
```

The packaged metadata currently declares GNOME Shell versions **45–50**.

Check and enable it with:

```bash
gnome-extensions info linuxcare-vitals@milmit.net
gnome-extensions enable linuxcare-vitals@milmit.net
```

If GNOME has just installed the extension and it is not visible yet, log out and back in once.

## Beta verification

The public Beta has passed the project release gates used for this build, including:

- Rust formatting and native dependency preflight.
- `cargo check`.
- Clippy with warnings denied.
- Unit tests.
- Development and optimized release builds.
- Static/security regression checks.
- Desktop, AppStream, GNOME, D-Bus, Polkit, and package metadata validation.
- Isolated **1024×600** GUI startup smoke.
- Staged install / upgrade / uninstall lifecycle testing.
- Real Debian install / legacy-upgrade / remove smoke testing on Ubuntu.
- Deterministic release packaging, SHA-256 checksums, release manifest, and SPDX 2.3 SBOM generation.

Primary CI runs on **Ubuntu 24.04 with Rust 1.93**, with an MSRV gate on **Rust 1.92**. See [`.github/workflows/ci.yml`](.github/workflows/ci.yml) for the current automated matrix.

## Build from source

Install the native development dependencies on Ubuntu:

```bash
sudo apt update
sudo apt install -y \
  build-essential pkg-config \
  libgtk-4-dev libadwaita-1-dev libgraphene-1.0-dev \
  desktop-file-utils appstream shellcheck xvfb dbus-x11
```

Use the pinned Rust toolchain and build the workspace:

```bash
rustup toolchain install 1.93.0 --component rustfmt clippy
cargo +1.93.0 build --release --workspace
```

Run the full project verification gate:

```bash
./scripts/beta-gate.sh
```

Build the Debian package:

```bash
./scripts/build-deb.sh
```

## Optional runtime integrations

LinuxCare remains useful when optional tools are absent, but some diagnostics become richer when these are available:

- `smartmontools` — authenticated SMART/NVMe health.
- `libnotify-bin` — desktop notification delivery.
- `bpftool` — richer eBPF readiness diagnostics; this Beta does not attach BPF programs.
- `gnome-extensions` — explicit LinuxCare Vitals enable/disable management.

## Screenshots

Real application screenshots are intentionally not fabricated. The repository currently contains the screenshot capture checklist and validation manifest under [`assets/screenshots/`](assets/screenshots/). Privacy-reviewed screenshots from the actual Beta UI will be added there for README/AppStream presentation.

## Known Beta limitations

- Per-process network bandwidth is intentionally unavailable until a real eBPF/cgroup accounting backend exists.
- SMART/NVMe health is read-only; LinuxCare does not run self-tests, firmware updates, sanitize, or format operations.
- Container reclaimable-space values are estimates and may differ from actual freed bytes because of shared layers and active references.
- Storage Runway needs enough local history before it can forecast capacity pressure.
- Data protected by Safety Quarantine still occupies disk space until purge.
- The Full Edition currently depends on host Polkit/D-Bus/systemd integration; the development Snap definition should not be treated as equivalent to the verified Debian package without separate confinement/store review.

## Documentation

- [`RELEASE_NOTES.md`](RELEASE_NOTES.md) — current Beta release notes
- [`CHANGELOG.md`](CHANGELOG.md) — development history
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — architecture and privilege boundaries
- [`SECURITY.md`](SECURITY.md) — security model and reporting guidance
- [`BETA_TESTING.md`](BETA_TESTING.md) — Beta test guidance
- [`VERIFY_RELEASE.md`](VERIFY_RELEASE.md) — release integrity verification

## Reporting bugs

Use [GitHub Issues](https://github.com/MilMit/LinuxCare-App/issues) and include:

- LinuxCare version.
- Ubuntu and GNOME versions.
- Clear reproduction steps.
- Expected and actual behavior.
- Sanitized logs only when useful.

Do **not** post passwords, authentication tokens, private file contents, unrelated personal paths, or sensitive endpoint data.

## Licensing

The repository currently does not declare an open-source license. Do not assume public source availability grants redistribution or relicensing rights. Licensing/distribution policy will be finalized separately from the Beta verification status.

---

<p align="center">
  <strong>LinuxCare</strong> — maintain Linux with evidence, explicit boundaries, and reversible actions where reversal is real.<br>Created by <a href="https://github.com/miladdadgar">Milad Dadgar</a> and published by <a href="https://github.com/MilMit">MilMit</a>.
</p>
