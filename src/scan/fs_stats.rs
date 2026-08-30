use super::ScanContext;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, Default)]
pub struct FsStats {
    pub bytes: u64,
    pub files: u64,
    pub directories: u64,
    pub errors: u64,
}

pub fn measure(path: &Path, ctx: &ScanContext) -> FsStats {
    let mut stats = FsStats::default();

    for entry in WalkDir::new(path).follow_links(false).into_iter() {
        if ctx.is_cancelled() {
            break;
        }

        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                stats.errors = stats.errors.saturating_add(1);
                continue;
            }
        };

        let meta = match entry.metadata() {
            Ok(meta) => meta,
            Err(_) => {
                stats.errors = stats.errors.saturating_add(1);
                continue;
            }
        };

        if meta.is_dir() {
            stats.directories = stats.directories.saturating_add(1);
        } else if meta.is_file() {
            stats.files = stats.files.saturating_add(1);
            stats.bytes = stats.bytes.saturating_add(meta.len());
        }
    }

    stats
}
