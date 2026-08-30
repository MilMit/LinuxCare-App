# LinuxCare 0.3.0-alpha.1 — Consolidated Intelligence & System Doctor implementation

This candidate intentionally batches the next two roadmap packs to reduce release churn and verification round-trips.

## Included

- Maintenance Intelligence + explainable Recommended Actions
- Health Score 2.0 with weighted known-domain scoring
- Service Doctor
- Package Doctor 2.0
- Network Doctor 3.0 with explicit-only active probe
- Filesystem Doctor
- Btrfs/Snapper/Timeshift awareness (read-only)
- Thermal & Power Doctor
- Battery Lab 2.0 local trend history

## Guardrails

- Recommendations do not auto-execute.
- Unknown evidence stays Unknown.
- No automatic system-service disable/mask.
- No automatic APT metadata refresh, source mutation, package repair, or kernel removal.
- Normal Network Doctor refresh creates no external probe traffic.
- No fsck/snapshot mutation.
- No governor/power-profile/charge-threshold mutation.
- Existing Polkit/D-Bus helper boundaries for privileged maintenance and SMART remain unchanged.
- alpha.9 quarantine accounting/Undo semantics remain unchanged.

## Verification status

Static metadata/security checks can be run in the artifact environment. Native Rust/GTK compilation must be verified on the Ubuntu development host with `cargo fmt` and `./scripts/verify.sh`. The candidate must not be tagged until that gate is green.
