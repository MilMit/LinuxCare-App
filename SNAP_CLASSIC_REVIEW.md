# LinuxCare classic confinement review rationale

This document is the technical rationale for the Snap Store classic-confinement review of `linuxcare`.

## Summary

LinuxCare is a safety-first native Ubuntu/GNOME maintenance and diagnostics application. Its core purpose requires observing and, only after explicit user authorization, maintaining resources owned by the host operating system rather than resources owned by the snap itself.

Strict confinement would remove or materially break core LinuxCare functionality. Classic confinement is therefore intentional rather than a packaging shortcut.

## Host access LinuxCare requires

LinuxCare needs to inspect or invoke host facilities that are not reliably available through strict-snap interfaces alone, including:

- APT/package-manager state and caches
- systemd services and boot diagnostics
- system journal data
- SMART/NVMe health tools when installed
- host network/socket diagnostics (`ss`, optional `bpftool`)
- Docker/Podman host storage and process information
- GNOME Shell extension state and host integration
- filesystem/storage diagnostics across host mounts
- selected privileged maintenance operations mediated by Polkit

It also needs access to arbitrary host executables when the corresponding optional diagnostic tool is installed by the user. This matches the documented classic-confinement use case for applications that must invoke or inspect host binaries outside the snap sandbox.

## Security architecture

Classic confinement does not mean the LinuxCare GUI runs as root.

LinuxCare follows these rules:

1. The GTK/Libadwaita GUI always runs unprivileged as the desktop user.
2. Privileged actions are restricted to a narrow D-Bus helper.
3. Each privileged operation is authorized through explicit Polkit actions.
4. LinuxCare does not expose a generic privileged shell or arbitrary command execution API.
5. User-space cleanup uses Safety Quarantine/Undo where appropriate.
6. Privileged cleanup is not falsely represented as undoable.
7. Quarantined bytes are not counted as freed until purge.
8. Hardware/network health is reported only from real probes; unavailable data is reported as unknown rather than healthy.

## Why strict confinement is insufficient

LinuxCare is not merely a desktop viewer. Its product purpose is to diagnose and safely maintain the actual Ubuntu host. Restricting it to the snap sandbox would produce misleading results for package state, boot/system services, journals, storage, network ownership and host maintenance, and would prevent the narrow host-level maintenance operations for which the application exists.

## Project and publisher

- Snap name: `linuxcare`
- Application: LinuxCare
- Publisher: MilMit
- Website: https://milmit.net
- Source: https://github.com/MilMit/LinuxCare
- Issue tracker: https://github.com/MilMit/LinuxCare/issues

The source repository includes the Polkit policy, D-Bus policy/service definition, systemd unit, packaging scripts, security regression checks, CI, Debian lifecycle tests and release verification material so reviewers can inspect the privilege boundary directly.
