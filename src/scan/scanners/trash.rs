use crate::{
    model::{CleanupCandidate, CleanupCategory, CleanupRisk, ExecutionKind, ScannerResult},
    scan::{fs_stats, ScanContext, Scanner},
};
use std::time::Instant;

pub struct TrashScanner;

impl Scanner for TrashScanner {
    fn name(&self) -> &'static str {
        "Trash"
    }

    fn scan(&self, ctx: &ScanContext) -> ScannerResult {
        let started = Instant::now();
        let path = ctx.home.join(".local/share/Trash/files");
        let info_path = ctx.home.join(".local/share/Trash/info");
        let mut result = ScannerResult {
            module: self.name().into(),
            ..Default::default()
        };
        if !path.exists() {
            result.duration_ms = started.elapsed().as_millis();
            return result;
        }
        let stats = fs_stats::measure(&path, ctx);
        let info_stats = if info_path.exists() {
            fs_stats::measure(&info_path, ctx)
        } else {
            Default::default()
        };
        result.files_scanned = stats.files + info_stats.files;
        result.directories_scanned = stats.directories + info_stats.directories;
        let traversal_errors = stats.errors.saturating_add(info_stats.errors);
        if traversal_errors > 0 {
            result.warnings.push(format!(
                "Trash measurement encountered {traversal_errors} filesystem error(s); the displayed size may be an undercount."
            ));
        }
        let total_bytes = stats.bytes.saturating_add(info_stats.bytes);
        if total_bytes > 0 {
            result.candidates.push(CleanupCandidate {
                id: "trash".into(),
                category: CleanupCategory::Trash,
                title: "Trash".into(),
                description: "Files already moved to the desktop Trash.".into(),
                path: Some(path),
                size_bytes: total_bytes,
                risk: CleanupRisk::Safe,
                reason: "These files were already marked for deletion by the user.".into(),
                consequence: "Emptying Trash permanently removes these files and they can no longer be restored from Trash.".into(),
                requires_privilege: false,
                execution: ExecutionKind::UserAllowedDirectory,
                privileged_action: None,
            });
        }
        result.duration_ms = started.elapsed().as_millis();
        result
    }
}
