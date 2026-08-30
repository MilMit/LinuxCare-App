# LinuxCare 0.7.0-beta.1 release status

## Verified

The development-host Beta gate has been reported successful for the BuildFix5 baseline and covered:

- Rust formatting and native preflight.
- `cargo check`.
- Clippy with warnings denied.
- Unit tests.
- Development build.
- Static/security regression checks.
- Desktop/AppStream/GNOME metadata validation.
- Isolated 1024×600 GUI startup smoke.
- Staged install, upgrade, and uninstall lifecycle verification.
- Preservation of per-user LinuxCare configuration/history on system integration removal.

The release-pack changes after that successful gate are limited to release engineering, metadata cleanup, documentation, asset validation, and GitHub workflow packaging. Runtime Rust feature logic is unchanged.

## Required before publishing binary artifacts

1. Run the release pack on the Ubuntu release host:

   ```bash
   ./scripts/prepare-beta-release.sh --real-install-smoke
   ```

2. Prefer running the real `.deb` smoke on an ephemeral VM/test host rather than a workstation that matters.

3. Capture and privacy-review real AppStream screenshots before any store-facing submission. Do not publish generated/mock screenshots as application screenshots.

4. Decide the final public repository/bug-tracker URLs before adding `vcs-browser` or `bugtracker` entries back to AppStream metadata.

5. Confirm the distribution/license policy before broad public source redistribution. The current AppStream metadata identifies the project as proprietary; it must not be silently relabeled as open source.

## Ubuntu App Center note

A downloaded third-party `.deb` can be installed through Ubuntu App Center, but searchable deb applications in App Center come from Ubuntu repositories. The normal independent-distribution path for App Center discovery is Snap Store publication. LinuxCare's Full Edition currently depends on host Polkit/D-Bus/systemd integration, so the development classic Snap should not be presented as equivalent to the verified Debian Full Edition without separate store review and confinement work.
