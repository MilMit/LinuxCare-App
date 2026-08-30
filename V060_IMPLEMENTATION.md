# LinuxCare 0.6.0-alpha.1 — Release Hardening Preview

This batch deliberately focuses on release quality rather than adding another large diagnostic surface.

## Added

- Pinned Rust 1.93 release-quality toolchain while retaining a Rust 1.92 MSRV check.
- Ubuntu 24.04 required CI gate plus Ubuntu 26.04 preview compatibility job.
- RustSec dependency audit and CodeQL Rust security workflow.
- Dependabot coverage for Cargo and GitHub Actions.
- Debian smoke validation, lintian integration, setuid/setgid payload rejection, and maintainer-script syntax checks.
- Deterministic Debian metadata/timestamps and root ownership.
- SPDX 2.3 Cargo dependency SBOM generation from the locked dependency set.
- GitHub Actions provenance and SBOM attestations with `actions/attest@v4`.
- Tag/version release guard so a `vX.Y.Z` tag cannot publish mismatched package metadata.
- AppStream/Desktop/ShellCheck validation when the relevant tools are installed.
- Structured GitHub bug-report template with explicit guidance to sanitize logs.
- Deprecated GNOME extension-local `version` metadata removed before extension-store submission.

## Deliberately not claimed

- Ubuntu 26.04 is treated as a compatibility signal while the GitHub-hosted image remains public preview.
- Snap/classic and Flatpak manifests remain development artifacts and are not claimed full-feature parity with the `.deb` host integration.
- The Cargo SPDX document covers locked Rust packages; distribution-native libraries are validated through the Debian/Ubuntu packaging path rather than misrepresented as Cargo dependencies.
