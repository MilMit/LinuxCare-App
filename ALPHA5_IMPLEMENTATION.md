# LinuxCare 0.2.0-alpha.5 implementation report

## Scope

This candidate implements Package Doctor and Battery Lab on top of the alpha.4 source candidate.

## Package Doctor

- Read-only `apt-get check` dependency consistency.
- Read-only `dpkg --audit`.
- `apt-mark showhold` held-package visibility.
- `apt-get -s autoremove` candidate preview.
- `apt-get -s upgrade` cached-metadata upgrade preview.
- Local active APT source-entry counting for legacy `.list` and deb822 `.sources`.
- Explicit Unknown state when a diagnostic cannot run.
- Existing APT cache cleanup/autoremove remain separate Polkit helper operations.

## Battery Lab

- Discovers all kernel `type=Battery` power-supply devices instead of hard-coding BAT0/BAT1/BAT2.
- Reads status, capacity, full/design/current energy, cycle count, voltage/current/power, technology/scope, runtime estimate and supported charge thresholds.
- Health is calculated only from compatible `energy_*` or compatible `charge_*` full/design pairs.
- No battery telemetry is persisted and no firmware/threshold value is changed.
- System Vitals now reuses the same central battery collector.

## Verification boundary

Static structural/metadata checks can be prepared here, but native Rust/GTK verification must run on the Ubuntu development host:

```bash
cargo fmt
./scripts/verify.sh
```

Do not tag alpha.5 until this gate is green and Package Doctor/Battery Lab smoke tests pass.
