# LinuxCare 0.2.0-alpha.8 — Hardware & Storage Diagnostics Preview

## Scope

This candidate adds a truthful hardware diagnostics layer without turning LinuxCare into a generic privileged hardware command runner.

### Hardware Doctor

- Local inventory is collected from `/proc`, DMI sysfs, DRM sysfs, `/sys/block`, and hwmon.
- No serial numbers are intentionally collected or persisted.
- Physical storage excludes loop/zram/device-mapper/md virtual block devices from SMART targets.
- Temperature values are bounded to a plausible -20..150 °C range before rendering.

### SMART / NVMe health

- User initiated only; no startup SMART probing.
- Dedicated Polkit action: `net.milmit.LinuxCare.read-storage-health`.
- Helper accepts only a validated block-device basename present in `/sys/block`.
- Helper constructs `/dev/<device>` and invokes an allowlisted smartctl binary through `/usr/bin/timeout` with fixed arguments:

```text
smartctl -a -n standby,0 -j /dev/<validated-device>
```

- Valid smartctl JSON is normalized to a bounded `StorageHealthReading`; raw output, model serial/WWN identifiers, and arbitrary SMART attributes are not returned.
- smartctl exit status is allowed to be non-zero when valid JSON exists because the tool uses status bits for health findings.

### Benchmark

- 256 MiB temporary file under `~/.cache/linuxcare`.
- Direct-I/O path uses 4096-byte-aligned memory and `O_DIRECT` for write and read, followed by `sync_data()` on the write phase.
- Fallback path requires successful `sync_data()` and `POSIX_FADV_DONTNEED` before measuring the read.
- Refuses tmpfs/ramfs/overlay/squashfs/system pseudo filesystems and refuses low-free-space targets.
- Result is explicitly labelled sequential **filesystem throughput**, not raw-device or vendor benchmark performance.

## Release gate

Run on Ubuntu:

```bash
cargo fmt
./scripts/verify.sh
```

Then smoke-test Hardware Doctor both with and without `smartmontools`, authenticate the SMART action, test an NVMe and ATA/SATA device where available, and verify the benchmark reports Direct I/O or an explicit fallback/refusal state.
