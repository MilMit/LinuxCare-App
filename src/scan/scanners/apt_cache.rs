use crate::{
    model::{
        CleanupCandidate, CleanupCategory, CleanupRisk, ExecutionKind, PrivilegedAction,
        ScannerResult,
    },
    scan::{fs_stats, ScanContext, Scanner},
};
use std::{path::Path, time::Instant};

pub struct AptCacheScanner;

impl Scanner for AptCacheScanner {
    fn name(&self) -> &'static str {
        "APT package cache"
    }

    fn scan(&self, ctx: &ScanContext) -> ScannerResult {
        let started = Instant::now();
        let path = Path::new("/var/cache/apt/archives");
        let mut result = ScannerResult {
            module: self.name().into(),
            ..Default::default()
        };
        if path.exists() {
            let stats = fs_stats::measure(path, ctx);
            result.files_scanned = stats.files;
            result.directories_scanned = stats.directories;
            if stats.errors > 0 {
                result.warnings.push(format!(
                    "APT cache could not be measured completely ({} filesystem error(s)); the displayed size may be an undercount.",
                    stats.errors
                ));
            }
            if stats.bytes > 0 {
                result.candidates.push(CleanupCandidate {
                    id: "apt-download-cache".into(),
                    category: CleanupCategory::AptCache,
                    title: "APT download cache".into(),
                    description: "Downloaded .deb package files retained by APT.".into(),
                    path: Some(path.to_path_buf()),
                    size_bytes: stats.bytes,
                    risk: CleanupRisk::Safe,
                    reason: "Installed packages do not depend on these downloaded installer files.".into(),
                    consequence: "APT may need to download package files again for reinstall or downgrade operations.".into(),
                    requires_privilege: true,
                    execution: ExecutionKind::PrivilegedProvider,
                    privileged_action: Some(PrivilegedAction::CleanAptCache),
                });
            }
        }
        result.duration_ms = started.elapsed().as_millis();
        result
    }
}
