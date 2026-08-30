# LinuxCare 0.5.0-alpha.1 implementation notes

## Scope

This consolidated batch adds four related surfaces without expanding the privileged helper API:

1. Deep Network live socket ownership and eBPF readiness.
2. Storage Intelligence 3.0 longer-baseline trend analysis.
3. systemd automation Next/Last status plus explicit manual Run Now.
4. GNOME Shell extension status/control and top-bar rendering polish.

## Deep Network

`src/deep_network.rs` executes the fixed local command:

```text
ss -H -t -u -n -a -p
```

Visible socket ownership is grouped by process/PID. LinuxCare never fills in missing ownership. Distinct remote endpoint counts are calculated in memory for the current view only and are not persisted.

The eBPF block is readiness detection, not instrumentation. It checks bpffs, cgroup v2, optional `bpftool`, and the kernel unprivileged-BPF policy. No BPF object is loaded or attached in this release, so LinuxCare deliberately does not display per-process network byte rates.

## Storage Intelligence 3.0

`src/storage_intelligence.rs` now chooses an older meaningful Smart Scan baseline within 30 days where possible. It reports baseline delta, normalized daily rate, category churn, and a confidence label based on actual history depth/time span.

This is separate from Storage Runway: Runway predicts filesystem-capacity pressure; Storage Intelligence attributes growth within cleanup categories.

## Automation UX

The existing generated `systemd --user` timer/service remains the execution boundary. `AutomationStatus` now reads `NextElapseUSecRealtime` and `LastTriggerUSec`. **Run Now** only asks systemd to start the same installed oneshot user service; it does not run a shell or bypass the scheduled-maintenance restrictions.

## GNOME integration

`src/gnome_integration.rs` checks GNOME Shell major version, LinuxCare Vitals install location, metadata compatibility, and enabled state. Enable/disable is limited to `linuxcare-vitals@milmit.net` through `gnome-extensions`.

The extension uses the shared `config.json` top-bar settings and now supports a 1/2/5/10 second refresh interval from GTK Settings. Display labels use compact CPU/TMP/RAM/NET/BAT text instead of emoji-heavy indicators.

## Verification status

Artifact-preparation static/security checks pass. Native Rust/GTK compilation is intentionally not claimed in the artifact environment because it lacks cargo/rustc/GTK development packages.

On Ubuntu/GNOME run:

```bash
cargo fmt
./scripts/verify.sh 2>&1 | tee verify-0.5.0-alpha.1.log
```

Then exercise the smoke checks in `VERIFICATION.md` before tagging.
