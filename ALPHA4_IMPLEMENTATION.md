# LinuxCare 0.2.0-alpha.4 implementation report

## Goal

Turn the verified Health Intelligence base into a more useful diagnostic product by adding two read-only intelligence layers: Boot Doctor and Storage Runway.

## Boot Doctor

Boot Doctor is intentionally diagnostic, not destructive. It collects:

- `systemd-analyze time --no-pager`
- `systemd-analyze blame --no-pager`
- `systemd-analyze critical-chain --no-pager`
- failed system services through `systemctl --failed`

The UI shows total boot time, available phase timing, slowest units, failed services, the critical dependency chain, and a conservative recommendation. A slow unit is never automatically treated as safe to disable.

## Storage Runway

Storage Runway stores bounded local samples in:

`$XDG_STATE_HOME/linuxcare/storage-runway.json`

or `~/.local/state/linuxcare/storage-runway.json` when `XDG_STATE_HOME` is unset.

The state directory/file follow the same restrictive model as Health Intelligence (`0700` directory and `0600` files on Unix).

Forecasting rules:

1. Root filesystem samples are recorded at most once every six hours.
2. The analysis window is capped at 30 days.
3. At least three samples spanning at least 12 hours are required.
4. A linear regression estimates observed bytes/day growth.
5. Near-flat, shrinking, or insufficient history never receives a fake fill date.
6. 80%, 90%, and 95% forecasts are presented as trend estimates only.

## Safety boundaries

No new privileged API was added. Boot Doctor does not disable, mask, stop, or edit services. Storage Runway only reads filesystem statistics and writes private application state under the user's state directory.

## Verification required

Run on the Ubuntu development machine:

```bash
cargo fmt
./scripts/verify.sh
```

Do not tag `v0.2.0-alpha.4` until the complete format/check/clippy/test/build gate passes and both new pages are manually smoke-tested.
