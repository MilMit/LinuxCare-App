# LinuxCare strict Snap Store review rationale

## Summary

`linuxcare` is packaged for the Snap Store using **strict confinement**.

The Store edition is intentionally narrower than the full Debian edition. It
keeps LinuxCare's desktop diagnostics, live vitals, storage/network/process
insight, battery/hardware views, security auditing and safety-first cleanup of
specific user-owned cache/trash locations, while removing host mutations that
would require classic confinement or a privileged root helper.

The strict snap does **not** ship or expose:

- the LinuxCare root D-Bus helper
- a Polkit plug or policy
- APT cache mutation or autoremove
- disabled Snap revision removal
- system journal vacuuming
- privileged SMART/NVMe reads
- GNOME Shell extension enable/disable management

Those capabilities remain available in the LinuxCare Debian package.

## Requested interfaces

### Desktop/runtime interfaces

The application uses Snapcraft's `gnome` extension on `core24`, which provides
the normal GTK/GNOME desktop interfaces and the `gnome-46-2404` runtime.

### `home`

Used for non-hidden user-owned files selected or inspected by LinuxCare.

### `personal-files`: `dot-linuxcare-cleanup-data`

LinuxCare is a cleanup application, so its primary user-space maintenance data
lives in hidden per-user locations that the ordinary `home` interface does not
expose. The declaration is intentionally limited to the exact locations used by
LinuxCare scanners/quarantine/state:

- `$HOME/.cache`
- `$HOME/.local/share/Trash`
- `$HOME/.cargo/registry/cache`
- `$HOME/.npm/_cacache`
- `$HOME/.local/share/coredump`
- `$HOME/.config/linuxcare`
- `$HOME/.local/state/linuxcare`
- `$HOME/.local/share/linuxcare`

LinuxCare does not request arbitrary access to the rest of the user's hidden
home directory. Cleanup operations apply an internal allowlist, reject symlinked
roots, require user ownership, and use Safety Quarantine where appropriate.

### Read-only system observation interfaces

LinuxCare requests observation interfaces because its purpose includes system
health and diagnostics:

- `system-observe` — process/system status
- `hardware-observe` — hardware information exposed through `/proc` and `/sys`
- `network-observe` — socket/network status
- `network-setup-observe` — read-only network configuration
- `network-manager-observe` — read-only NetworkManager state where available
- `log-observe` — read-only system log diagnostics
- `upower-observe` — battery/power information
- `removable-media` — storage analysis for user-attached media
- `network` — ordinary network access used by diagnostics that need it

No control interfaces such as `snapd-control`, `packagekit-control`,
`firewall-control`, `process-control`, `block-devices`, or `network-control` are
requested by the Store edition.

## Product-mode enforcement

The snap sets `LINUXCARE_STORE_EDITION=1`.

LinuxCare uses this flag to avoid presenting host-mutating cleanup scanners and
to reject calls to the classic/DEB-only privileged provider with a clear message
instead of attempting to escape confinement. Flatpak mutation and GNOME Shell
extension management are likewise disabled in the Store edition.

## CI enforcement

The `LinuxCare Snap Store` workflow:

1. builds the snap on Ubuntu 24.04;
2. installs it without `--classic`;
3. verifies the installed snap reports `confinement: strict`;
4. verifies there is no Polkit connection, root helper service, or helper D-Bus slot;
5. validates the GNOME runtime/GSettings environment;
6. runs a GTK/Libadwaita startup smoke test under Xvfb with GLib criticals fatal;
7. uploads the exact tested snap as the workflow artifact.

## Project

- Snap: `linuxcare`
- Publisher: MilMit
- Website: https://milmit.net
- Source: https://github.com/MilMit/LinuxCare
- Issues: https://github.com/MilMit/LinuxCare/issues
