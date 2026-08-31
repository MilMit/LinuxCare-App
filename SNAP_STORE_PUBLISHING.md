# LinuxCare on Ubuntu App Center / Snap Store

LinuxCare is distributed through the public Snap Store and Ubuntu App Center.

## Store edition

The Snap Store edition uses:

- Ubuntu core24
- GNOME extension
- strict confinement
- LINUXCARE_STORE_EDITION=1

The Store edition deliberately excludes privileged host mutations available in
the full Debian edition, including:

- APT mutation and autoremove
- disabled Snap revision removal
- system journal vacuuming
- privileged SMART/NVMe operations
- Flatpak runtime mutation
- GNOME Shell extension management
- LinuxCare root D-Bus helper
- Polkit policy

## personal-files review

LinuxCare requests the `dot-linuxcare-cleanup-data` personal-files plug for
explicitly declared user-owned cache, trash and LinuxCare state locations.

Because `personal-files` requires Store review, the first public Store revision
requires installation approval.

See `SNAP_STORE_REVIEW.md` for the complete interface rationale.

## Release channels

LinuxCare 0.7.0-beta.2 currently uses:

- `grade: devel`
- `confinement: strict`

Pre-release versions are intended for the Snap Store `beta` channel.

The `stable` channel must only be used after LinuxCare reaches a stable release
and `snapcraft.yaml` uses `grade: stable`.

## CI validation

The LinuxCare Snap Store workflow:

1. builds the strict Snap on Ubuntu 24.04;
2. installs it without `--classic`;
3. verifies `confinement: strict`;
4. verifies no root helper or Polkit policy is shipped;
5. validates GNOME runtime and GSettings;
6. tests diagnostic interfaces;
7. performs a GTK/Libadwaita GUI startup smoke test;
8. uploads the exact tested Snap artifact.

## Project

- Snap: `linuxcare`
- Publisher: MilMit
- Website: https://milmit.net
- Source: https://github.com/MilMit/LinuxCare-App
- Issues: https://github.com/MilMit/LinuxCare-App/issues
