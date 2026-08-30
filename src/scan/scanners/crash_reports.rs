use crate::{
    format,
    model::{CleanupCandidate, CleanupCategory, CleanupRisk, ExecutionKind, ScannerResult},
    scan::{fs_stats, ScanContext, Scanner},
};
use std::{path::Path, time::Instant};

pub struct CrashReportScanner;

impl Scanner for CrashReportScanner {
    fn name(&self) -> &'static str {
        "Crash Reports & Dumps"
    }

    fn scan(&self, ctx: &ScanContext) -> ScannerResult {
        let started = Instant::now();
        let mut result = ScannerResult {
            module: self.name().into(),
            ..Default::default()
        };

        let crash_paths = [
            (
                "/var/crash",
                CleanupRisk::Review,
                ExecutionKind::AnalysisOnly,
            ),
            (
                ".local/share/coredump",
                CleanupRisk::Safe,
                ExecutionKind::UserAllowedDirectory,
            ),
        ];

        for (target, risk, execution) in crash_paths {
            if ctx.is_cancelled() {
                break;
            }
            let path = if target.starts_with('/') {
                Path::new(target).to_path_buf()
            } else {
                ctx.home.join(target)
            };

            if !path.exists() {
                continue;
            }

            let stats = fs_stats::measure(&path, ctx);
            result.files_scanned += stats.files;
            result.directories_scanned += stats.directories;

            if stats.bytes > 0 {
                let system_scope = path.as_path() == Path::new("/var/crash");
                result.candidates.push(CleanupCandidate {
                    id: format!("crash-reports:{}", path.display()),
                    category: CleanupCategory::CrashReports,
                    title: format!("Crash report dump ({})", path.display()),
                    description: if system_scope {
                        format!(
                            "System crash reports occupying {} (review-only in this release)",
                            format::bytes(stats.bytes)
                        )
                    } else {
                        format!("User coredumps occupying {}", format::bytes(stats.bytes))
                    },
                    path: Some(path),
                    size_bytes: stats.bytes,
                    risk,
                    reason: "Crash reports record diagnostic details when applications fail. Keep them while troubleshooting; otherwise user-owned coredumps may be cleared.".into(),
                    consequence: "Deleted crash data will no longer be available for debugging or bug reports.".into(),
                    requires_privilege: false,
                    execution,
                    privileged_action: None,
                });
            }
        }

        result.duration_ms = started.elapsed().as_millis();
        result
    }
}
