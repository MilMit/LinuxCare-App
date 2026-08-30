# LinuxCare 0.7.0-beta.1 — Beta Preparation

This batch intentionally freezes feature expansion and focuses on making the existing product testable as a Beta candidate.

## What changed

- Native GUI now exposes `--version` and `--help` without needing a display.
- Startup no longer treats `/` as the user's home when HOME is unavailable.
- Automation and Cleanup Policy JSON accept missing fields using safe defaults for upgrade compatibility.
- Shared settings temp files are explicitly created as 0600.
- GNOME Power Profile switching uses typed system D-Bus rather than command-line `gdbus` spawning.
- GNOME app opening uses fixed `/usr/bin/linuxcare` AppInfo launch without shell fallback.
- Manual installer/uninstaller support a temporary `LINUXCARE_DESTDIR` for lifecycle tests, while real host installation still requires root.
- Manual extension installation replaces only LinuxCare's own extension directory so stale files from older manual copies cannot survive.
- New security, GUI, staged lifecycle and real Debian lifecycle smoke suites are automated in CI.
- UI copy is less emoji-heavy and the default window is smaller while existing adaptive `AdwFlap` behavior remains.

## Beta gates

Run on the Ubuntu development host:

```bash
cargo fmt
./scripts/beta-gate.sh
```

Run the real Debian package lifecycle only on an ephemeral VM/test host:

```bash
sudo ./scripts/smoke-deb-install.sh
```

## Still manual before public Beta

- keyboard/screen-reader accessibility audit
- reduced-motion and high-text-scale review
- real GNOME extension enable/disable smoke
- real Polkit prompt flows
- real SMART/NVMe device tests
- real Safety Quarantine conflict/Undo/Restore Aside scenarios
- repository URL, screenshots and final license/distribution terms
