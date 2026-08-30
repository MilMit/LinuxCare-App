# LinuxCare 0.2.0-alpha.2 implementation report

Date: 2026-08-30

## Completed hardening

1. Unified GTK and GNOME Shell settings on `$XDG_CONFIG_HOME/linuxcare/config.json`.
2. Removed the GNOME extension's `pkexec` / shell-based RAM flush.
3. Removed `DropCaches` from the GUI, privileged D-Bus helper, client, and Polkit policy.
4. Preserved `ProtectKernelTunables=true` because LinuxCare no longer needs to mutate kernel tunables.
5. Added a dedicated `net.milmit.LinuxCare.autoremove-packages` Polkit action.
6. APT autoremove now reports completion without fabricating a reclaimed-byte total.
7. `/var/crash` is analysis-only; `~/.local/share/coredump` is an exact safe user-cleanup target.
8. Process termination and APT autoremove require a second explicit click.
9. Listening sockets bound beyond loopback are labelled `NETWORK`, not `PUBLIC`.
10. Removed the false `SMART verified` statement.
11. Reworked the benchmark to 128 MiB with `sync_data()` + `posix_fadvise(POSIX_FADV_DONTNEED)` before the read phase.
12. Replaced nonfunctional automation switches with preview-only UI.
13. Debian packaging now includes application icons and the GNOME Shell extension, plus post-remove cache/systemd refresh.
14. GitHub alpha/beta/RC tags publish as prereleases and release artifacts receive provenance attestations.
15. Snap metadata is `grade: devel`; classic packaging is explicitly development-only for this alpha.
16. README, SECURITY, ARCHITECTURE, VERIFICATION, CHANGELOG, Cargo metadata, AppStream, Snap, and Debian packaging are synchronized to `0.2.0-alpha.2`.
17. GNOME extension config monitoring now watches the config directory, so atomic Rust config-file replacement is detected reliably.
18. GNOME extension refresh interval now honors the shared `top_bar.interval_sec` setting.

## Static validation performed here

- Cargo TOML parsed successfully.
- AppStream / Polkit / D-Bus XML parsed successfully.
- GNOME extension metadata JSON parsed successfully.
- Snapcraft and GitHub workflow YAML parsed successfully.
- all shell scripts passed `bash -n`.
- GNOME extension passed JavaScript syntax parsing with Node in ESM mode.
- a comment/string-aware structural delimiter pass succeeded for every Rust source file.
- security searches found no executable `drop_caches`, `DropCaches`, `pkexec`, old `settings.json`, or false `SMART verified` references in runtime source.

## Not verified here

The artifact-generation environment does not have Rust/Cargo or GTK4/Libadwaita development headers. An attempted package-index refresh was unavailable within the environment, so no compile claim is made.

Before installing the helper, run on Ubuntu:

```bash
cd LinuxCare-0.2.0-alpha.2
./scripts/verify.sh
cargo build --release --bins
./scripts/build-deb.sh
```

Then test on a disposable/non-critical Ubuntu GNOME machine:

```text
- launch GUI as normal user
- helper health / GetVersion
- APT clean Polkit prompt
- APT autoremove separate Polkit prompt + double confirmation
- disabled Snap revision removal
- journal vacuum 7/30/90 days
- user coredump cleanup
- /var/crash remains review-only
- process termination double confirmation
- GNOME extension reads/writes config.json
- extension contains no privileged RAM action
- benchmark values are plausible across repeated runs
- .deb installs icon + extension + helper integration
```

## Deliberately deferred

- Health Score / What Changed timeline
- quarantine + undo
- real SMART/NVMe health provider
- system-wide service/startup mutation provider
- old-kernel cleanup
- Docker/Podman cleanup
- production Store/App Center confinement redesign
