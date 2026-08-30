use crate::{
    model::{
        CleanupCandidate, CleanupCategory, CleanupRisk, ExecutionKind, PrivilegedAction,
        ScannerResult,
    },
    scan::{ScanContext, Scanner},
};
use std::{fs, path::Path, process::Command, time::Instant};

pub struct SnapScanner;

impl Scanner for SnapScanner {
    fn name(&self) -> &'static str {
        "Snap revisions"
    }

    fn scan(&self, _ctx: &ScanContext) -> ScannerResult {
        let started = Instant::now();
        let mut result = ScannerResult {
            module: self.name().into(),
            ..Default::default()
        };
        let binary = Path::new("/usr/bin/snap");
        if !binary.exists() {
            result.duration_ms = started.elapsed().as_millis();
            return result;
        }
        let output = Command::new(binary)
            .args(["list", "--all"])
            .env("LC_ALL", "C")
            .output();
        let Ok(output) = output else {
            result
                .warnings
                .push("Snap is installed but could not be queried.".into());
            result.duration_ms = started.elapsed().as_millis();
            return result;
        };
        if !output.status.success() {
            result
                .warnings
                .push("Snap returned an error while listing revisions.".into());
            result.duration_ms = started.elapsed().as_millis();
            return result;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines().skip(1) {
            let cols: Vec<_> = line.split_whitespace().collect();
            if cols.len() < 4 || !cols.contains(&"disabled") {
                continue;
            }
            let name = cols[0];
            let revision = cols.get(2).copied().unwrap_or("?");
            let snap_file =
                Path::new("/var/lib/snapd/snaps").join(format!("{name}_{revision}.snap"));
            let size = fs::metadata(&snap_file).map(|m| m.len()).unwrap_or(0);
            result.files_scanned += 1;
            result.candidates.push(CleanupCandidate {
                id: format!("snap:{name}:{revision}"),
                category: CleanupCategory::SnapRevision,
                title: format!("{name} revision {revision}"),
                description: "A Snap revision explicitly reported as disabled by snapd.".into(),
                path: snap_file.exists().then_some(snap_file),
                size_bytes: size,
                risk: CleanupRisk::Safe,
                reason: "snapd marks this revision disabled, so it is not the active application revision.".into(),
                consequence: "This specific old revision will no longer be available locally for rollback.".into(),
                requires_privilege: true,
                execution: ExecutionKind::PrivilegedProvider,
                privileged_action: Some(PrivilegedAction::RemoveDisabledSnapRevision { name: name.to_string(), revision: revision.to_string() }),
            });
        }
        result.duration_ms = started.elapsed().as_millis();
        result
    }
}
