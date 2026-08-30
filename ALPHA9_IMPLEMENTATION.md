# LinuxCare 0.2.0-alpha.9 — Safety Undo & Maintenance Timeline

This preview changes the user-space cleanup contract from immediate irreversible deletion to a bounded Safety Quarantine workflow.

## Safety Quarantine

- Applies only to `ExecutionKind::UserAllowedDirectory` targets already constrained by LinuxCare's user allowlist.
- Persists transaction intent under LinuxCare private state before detaching data.
- Uses same-filesystem `renameat2(RENAME_NOREPLACE)` to move the directory entry into a hidden `.linuxcare-quarantine-*` sibling.
- Recreates the expected cache/trash directory immediately for application compatibility.
- Keeps protected bytes allocated during the 30-minute undo window; UI does not count those bytes as freed.
- Restore is refused if the application has repopulated the recreated target, preventing overwrite of new state.
- Manual purge is permanent and requires a second click within five seconds.
- Expired quarantine is purged while LinuxCare is open or on the next launch.

## Maintenance Timeline

A new private, bounded `timeline.json` records:

- Smart Scan completion (observed candidate bytes only; scan removes nothing)
- cleanup completion/cancellation
- Undo/restore
- purge/expiry

Actual freed, protected, restored, and observed candidate bytes remain separate values. Privileged APT/Snap/journal and provider operations are recorded but never represented as undoable.

## Compatibility

Legacy `history.json` remains available and is displayed as earlier cleanup history. New cleanup results add serde-defaulted quarantine fields so older history records remain readable.

## Verification

Before tagging:

```bash
cargo fmt
./scripts/verify.sh
```

Then execute the alpha.9 quarantine smoke tests in `VERIFICATION.md` on the Ubuntu development host.
