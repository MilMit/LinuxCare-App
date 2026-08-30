# LinuxCare 0.2.0-alpha.3 implementation report

## Scope

Alpha.3 adds the first Health Intelligence layer on top of the alpha.2 Truth & Safety hardening work.

## Implemented

1. `src/health.rs` with a conservative health scoring engine.
2. Verified signals: system storage, MemAvailable-based RAM pressure, failed system/user services, UFW state, and network-bound listener count.
3. Unknown signal values do not reduce the score.
4. Health collection runs outside the GTK main loop.
5. Persistent system snapshots under XDG state, sampled at most once every six hours and capped at 120.
6. Roughly seven-day comparison baseline when enough history exists; oldest baseline fallback for younger installations.
7. Persistent Smart Scan snapshots capped at 60.
8. Scan category deltas and total safe-reclaimable deltas in What Changed.
9. Atomic state writes with restrictive permissions.
10. Dashboard-only Health Intelligence panel; Cleaner still records scan snapshots without duplicate UI.
11. Health panel refreshes after a completed Smart Scan on the Dashboard instance.
12. Unit tests for score behavior and kernel-change reporting.
13. Security HUD uses tri-state UFW reporting (active/inactive/unknown).
14. Same-port socket merging cannot hide a network-bound listener behind a loopback-only row.
15. `scripts/verify.sh` has explicit native dependency preflight and retains static/security checks even before Rust compilation.

## Deliberate non-claims

- A network-bound socket is **not** called internet-exposed.
- A missing UFW/systemd/ss signal is **not** treated as unhealthy.
- High Linux page-cache use is **not** called wasted RAM; memory pressure is based on `MemAvailable`.
- Alpha.3 has **not** been declared compile-verified in the preparation container because the Rust/GTK toolchain is unavailable there.

## Release gate

On an Ubuntu/GNOME development machine with Rust 1.92 and the required native dependencies:

```bash
./scripts/verify.sh
cargo build --release --bins
./scripts/build-deb.sh
```

Do not tag `v0.2.0-alpha.3` until all three complete successfully and the Dashboard/Smart Cleaner are manually smoke-tested.
