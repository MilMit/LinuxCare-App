# LinuxCare Release Checklist

## Blocking before any tag

- [ ] `cargo fmt`
- [ ] `./scripts/verify.sh` passes on Ubuntu 24.04/GNOME
- [ ] `./scripts/build-deb.sh` succeeds
- [ ] `./scripts/validate-package.sh` passes
- [ ] Safety Quarantine/Undo smoke tests pass on disposable data
- [ ] Polkit/D-Bus helper smoke tests pass
- [ ] SMART path is tested with and without smartmontools
- [ ] Automation user timer install/run/disable is tested
- [ ] GNOME extension enable/disable and shared settings are tested

## Security / supply chain

- [ ] RustSec workflow is green
- [ ] CodeQL workflow is green
- [ ] No ignored advisory exists without a written rationale
- [ ] Release contains SHA256SUMS and SPDX SBOM
- [ ] GitHub provenance and SBOM attestations are present and verifiable
- [ ] `.deb` contains no setuid/setgid payload

## Distribution metadata

- [ ] `appstreamcli validate --no-net data/net.milmit.LinuxCare.metainfo.xml`
- [ ] `desktop-file-validate data/net.milmit.LinuxCare.desktop`
- [ ] Add real AppStream screenshots before a polished software-center submission
- [ ] Replace generic website URLs with the real public repository/bug tracker once that repository URL is final
- [ ] Confirm the distribution/license notice before a public stable source release; do not invent an open-source license
- [ ] Verify GNOME Shell `shell-version` entries against versions actually tested by the extension

## Compatibility

- [ ] Ubuntu 24.04 blocking CI is green
- [ ] Rust 1.92 MSRV check is green
- [ ] Ubuntu 26.04 preview compatibility result reviewed
- [ ] Real Ubuntu 26.04/GNOME hardware or VM smoke test before declaring full 26.04 support

## Stable-release bar

Do not call LinuxCare stable merely because it compiles. Stable requires the destructive-path smoke tests, privilege-boundary checks, package validation, metadata review and upgrade/uninstall behavior to be exercised on a clean machine and an upgrade from the previous package.

## Beta candidate gate

Before publishing `v0.7.0-beta.2` or any later Beta:

- [ ] `cargo fmt` leaves no diff
- [ ] `./scripts/beta-gate.sh` passes on Ubuntu 24.04
- [ ] real `sudo ./scripts/smoke-deb-install.sh` passes on an ephemeral Ubuntu host
- [ ] Debian package validation/Lintian passes with no error-level findings
- [ ] Ubuntu 26.04 preview compatibility job has been reviewed even while non-blocking
- [ ] keyboard navigation and visible focus have been manually checked at 1024×600
- [ ] GNOME reduced-motion/animations-disabled behavior has been manually checked
- [ ] UI has been checked at 100%, 150% and 200% text scaling
- [ ] SMART, Polkit prompts, Safety Quarantine Undo/Restore Aside, Automation timer, and GNOME extension have each received one real-machine smoke test
- [ ] user config/state survives package removal; stale package-owned extension files do not survive upgrade
- [ ] repository URL, screenshots and distribution license terms are real/final before public store submission

## Beta release pack (0.7.0-beta.2)

- [ ] `./scripts/prepare-beta-release.sh --real-install-smoke` passes on a disposable Ubuntu release host/VM.
- [ ] `target/release-pack/` contains `.deb`, binary tarball, deterministic source tarball, `SBOM.spdx.json`, `RELEASE-MANIFEST.json`, `SHA256SUMS.txt`, release notes, and build info.
- [ ] `linuxcare_0.7.0-beta.2_amd64.deb` passes `scripts/validate-package.sh` and the real install/upgrade/remove smoke.
- [ ] Real AppStream screenshots are captured from the verified Beta UI, structurally validated, and manually privacy-reviewed before any store-facing submission.
- [ ] Distribution/license policy is explicitly confirmed; do not imply an open-source license if none was chosen.
- [ ] GitHub Release is marked prerelease and uses `RELEASE_NOTES.md` / `RELEASE_NOTES_0.7.0-beta.2.md` as the release body/archive.
- [ ] GitHub provenance and SPDX SBOM attestations are generated for binary artifacts.
