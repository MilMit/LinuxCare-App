# LinuxCare classic confinement request — retired

LinuxCare no longer requests classic confinement for its Snap Store edition.

The Store package has been redesigned for `confinement: strict` and deliberately
separates Store-safe functionality from the full Debian edition. The strict snap
does not ship LinuxCare's root helper, Polkit policy, or system D-Bus service and
does not attempt APT mutation, disabled-Snap removal, journal vacuuming,
privileged SMART access, or GNOME Shell extension management.

See `SNAP_STORE_REVIEW.md` for the current strict-confinement architecture and
interface rationale.
