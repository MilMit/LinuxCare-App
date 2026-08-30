use crate::{
    format,
    model::{
        CleanupCandidate, CleanupCategory, CleanupRisk, ExecutionKind, PrivilegedAction,
        ScannerResult,
    },
    scan::{fs_stats, ScanContext, Scanner},
};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;

pub struct JournalScanner;

impl Scanner for JournalScanner {
    fn name(&self) -> &'static str {
        "System journal"
    }

    fn scan(&self, ctx: &ScanContext) -> ScannerResult {
        let started = Instant::now();
        let mut result = ScannerResult {
            module: self.name().into(),
            ..Default::default()
        };
        let mut total_bytes = 0u64;
        let mut old_archived_bytes = 0u64;
        let mut traversal_errors = 0u64;

        for p in [Path::new("/var/log/journal"), Path::new("/run/log/journal")] {
            if !p.exists() {
                continue;
            }
            let s = fs_stats::measure(p, ctx);
            total_bytes = total_bytes.saturating_add(s.bytes);
            result.files_scanned += s.files;
            result.directories_scanned += s.directories;
            traversal_errors = traversal_errors.saturating_add(s.errors);
            old_archived_bytes =
                old_archived_bytes.saturating_add(estimate_old_archived(p, 30, ctx));
        }

        if traversal_errors > 0 {
            result.warnings.push(format!(
                "System journal could not be measured completely ({} filesystem error(s)). Authentication may be required for a complete analysis.",
                traversal_errors
            ));
        }

        let roots = [Path::new("/var/log/journal"), Path::new("/run/log/journal")];
        for days in [7u32, 30u32, 90u32] {
            let mut archived_bytes = 0u64;
            for r in roots {
                if r.exists() {
                    archived_bytes =
                        archived_bytes.saturating_add(estimate_old_archived(r, days as u64, ctx));
                }
            }
            if archived_bytes > 0 || days == 30 {
                result.candidates.push(CleanupCandidate {
                    id: format!("system-journal-{days}d"),
                    category: CleanupCategory::SystemJournal,
                    title: format!("System journal — keep {days} days"),
                    description: format!(
                        "Journal files use about {} in total. Approximately {} is in archived files older than {days} days.",
                        format::bytes(total_bytes),
                        format::bytes(archived_bytes)
                    ),
                    path: None,
                    size_bytes: archived_bytes,
                    risk: CleanupRisk::Review,
                    reason: format!("System logs may be useful for troubleshooting. Removing logs older than {days} days frees storage while retaining recent diagnostics."),
                    consequence: "System events older than the retained window will be permanently removed and no longer available for troubleshooting.".into(),
                    requires_privilege: true,
                    execution: ExecutionKind::PrivilegedProvider,
                    privileged_action: Some(PrivilegedAction::VacuumJournal { keep_days: days }),
                });
            }
        }

        result.duration_ms = started.elapsed().as_millis();
        result
    }
}

fn estimate_old_archived(root: &Path, keep_days: u64, ctx: &ScanContext) -> u64 {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(keep_days.saturating_mul(86_400)))
        .unwrap_or(UNIX_EPOCH);
    let mut bytes = 0u64;

    for entry in WalkDir::new(root).follow_links(false).into_iter() {
        if ctx.is_cancelled() {
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        // Active journal files normally lack '@'. Vacuum targets archived journal files.
        if !name.ends_with(".journal") || !name.contains('@') {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified < cutoff {
            bytes = bytes.saturating_add(meta.len());
        }
    }
    bytes
}
