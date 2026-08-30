# LinuxCare 0.2.0-alpha.6 implementation report

## Network Doctor 2.0

- Read-only NetworkManager connectivity state.
- Default gateway/device/metric discovery from the kernel route table.
- DNS discovery through systemd-resolved with resolv.conf fallback.
- NetworkManager interface inventory.
- Existing UFW/listener audit reused for exposure context.
- Captive portal, limited/offline, missing DNS, and route-topology review notes.

## Docker / Podman Analyzer

- Optional Docker and Podman providers are detected independently.
- Fixed read-only CLI queries run with a six-second timeout.
- Docker system-df JSON-lines and Podman system-df JSON parsing.
- Storage categories, total/active items, approximate used bytes, and reclaimable estimates.
- No prune/remove/stop/kill/volume-delete action exists.

## Included correction

- Integrates the Rust 1.93 Clippy `manual_range_contains` correction previously found in Battery Lab runtime validation.

## Verification requirement

Run `cargo fmt` and `./scripts/verify.sh` on the Ubuntu development host. Alpha.6 is a source candidate until that native gate passes.
