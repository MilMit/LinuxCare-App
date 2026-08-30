# LinuxCare 0.7.0-beta.1 release status

## Published public Beta

LinuxCare `0.7.0-beta.1` has been published as a GitHub **Pre-release** at:

https://github.com/MilMit/LinuxCare/releases/tag/v0.7.0-beta.1

The release contains the verified Debian package, binary archive, deterministic source archive, SPDX 2.3 SBOM, release manifest, and SHA-256 checksums.

## Verified

The development-host Beta gate completed successfully and covered:

- Rust formatting and native preflight.
- `cargo check`.
- Clippy with warnings denied.
- Unit tests.
- Development and optimized release builds.
- Static/security regression checks.
- Desktop/AppStream/GNOME metadata validation.
- Isolated 1024×600 GUI startup smoke.
- Staged install, upgrade, and uninstall lifecycle verification.
- Preservation of per-user LinuxCare configuration/history on system integration removal.
- Debian package construction and smoke validation.
- Real root-level Debian install / staged legacy-upgrade / remove smoke testing.
- Deterministic release-asset generation, SHA-256 checksums, release manifest, and SPDX SBOM generation.

The real Debian smoke installed `linuxcare 0.7.0-beta.1`, upgraded the staged previous version, resolved its runtime dependencies, and removed LinuxCare cleanly.

## Release engineering status

The published Beta release uses tag `v0.7.0-beta.1` and is marked as a GitHub Pre-release rather than a stable release.

The release pack includes:

- `linuxcare_0.7.0-beta.1_amd64.deb`
- `linuxcare-0.7.0-beta.1-x86_64-linux-gnu.tar.gz`
- `linuxcare-0.7.0-beta.1-source.tar.gz`
- `SBOM.spdx.json`
- `SHA256SUMS.txt`
- `RELEASE-MANIFEST.json`

## Screenshots / store media

Real application screenshots have not been fabricated. The screenshot capture checklist and manifest remain under `assets/screenshots/`.

Before any store-facing submission, capture the actual Beta UI, privacy-review the images, and validate them with the repository screenshot tooling.

## Ubuntu App Center note

A downloaded third-party `.deb` can be installed through Ubuntu App Center, but searchable deb applications in App Center come from Ubuntu repositories. The normal independent-distribution path for App Center discovery is Snap Store publication.

LinuxCare Full Edition currently depends on host Polkit/D-Bus/systemd integration, so the development Snap should not be presented as equivalent to the verified Debian Full Edition without separate confinement and store-review work.

## Licensing note

The repository currently does not declare an open-source license. Public source visibility does not by itself grant redistribution or relicensing rights. Distribution/license policy should be finalized separately from the Beta verification status.
