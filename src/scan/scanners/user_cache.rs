use crate::{
    model::{CleanupCandidate, CleanupCategory, CleanupRisk, ExecutionKind, ScannerResult},
    scan::{fs_stats, ScanContext, Scanner},
};
use std::{fs, time::Instant};

pub struct UserCacheScanner;

impl Scanner for UserCacheScanner {
    fn name(&self) -> &'static str {
        "Application caches"
    }

    fn scan(&self, ctx: &ScanContext) -> ScannerResult {
        let started = Instant::now();
        let root = ctx.home.join(".cache");
        let mut result = ScannerResult {
            module: self.name().into(),
            ..Default::default()
        };

        let Ok(entries) = fs::read_dir(&root) else {
            result
                .warnings
                .push(format!("Could not read {}", root.display()));
            result.duration_ms = started.elapsed().as_millis();
            return result;
        };

        for entry in entries.flatten() {
            if ctx.is_cancelled() {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "thumbnails" || crate::quarantine::is_internal_quarantine_name(&name) {
                continue;
            }
            let path = entry.path();
            let stats = fs_stats::measure(&path, ctx);
            result.files_scanned += stats.files;
            result.directories_scanned += stats.directories;
            if stats.errors > 0 {
                result.warnings.push(format!(
                    "{} cache could not be read completely ({} filesystem error(s)); its displayed size may be an undercount.",
                    name, stats.errors
                ));
            }
            if stats.bytes == 0 {
                continue;
            }

            result.candidates.push(CleanupCandidate {
                id: format!("user-cache:{name}"),
                category: CleanupCategory::UserCache,
                title: name.clone(),
                description: format!("Cached data owned by {name} or a component using this cache directory."),
                path: Some(path),
                size_bytes: stats.bytes,
                risk: CleanupRisk::Review,
                reason: "Application caches can often be recreated, but clearing them can remove offline state or make the next launch slower.".into(),
                consequence: "The application may need to download or rebuild cached data again.".into(),
                requires_privilege: false,
                execution: ExecutionKind::UserAllowedDirectory,
                privileged_action: None,
            });
        }

        result
            .candidates
            .sort_by_key(|c| std::cmp::Reverse(c.size_bytes));
        result.duration_ms = started.elapsed().as_millis();
        result
    }
}
