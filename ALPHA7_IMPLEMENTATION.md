# LinuxCare 0.2.0-alpha.7 — Process Intelligence Preview

## Added

- Dedicated Process Intelligence page.
- `/proc`-based per-process CPU, RSS memory, and disk-I/O sampling.
- One-hour bounded local telemetry history, persisted every 15 seconds.
- 10-minute Historical Vitals summary for CPU, RAM, network throughput, and block-device I/O.
- Baseline-gated anomaly detection.
- PID reuse protection via PID + `/proc/<pid>/stat` start-time identity.

## Accuracy / privacy boundaries

LinuxCare deliberately does not derive per-process network throughput from `/proc/<pid>/net`, because those files describe the process network namespace rather than reliable per-PID byte ownership. Per-process network is reserved for a future eBPF/cgroup-backed implementation.

Persisted history excludes command lines and file paths. State files use private directory/file permissions on Unix.

## Verification

Run:

```bash
cargo fmt
./scripts/verify.sh
```
