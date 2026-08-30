use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Debug, Clone)]
pub struct DirUsage {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub files_count: u64,
}

pub fn analyze_home(home: &Path, cancel: Arc<AtomicBool>) -> Vec<DirUsage> {
    let mut results = Vec::new();
    let targets = [
        "Downloads",
        "Documents",
        "Pictures",
        "Videos",
        "Music",
        "Desktop",
        ".local/share",
        ".cache",
        ".var/app",
    ];

    for target in targets {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let full = home.join(target);
        if !full.exists() {
            continue;
        }
        let (bytes, files) = measure_dir(&full, &cancel);
        if bytes > 0 {
            results.push(DirUsage {
                name: target.to_string(),
                path: full,
                bytes,
                files_count: files,
            });
        }
    }

    results.sort_by_key(|d| std::cmp::Reverse(d.bytes));
    results
}

fn measure_dir(dir: &Path, cancel: &Arc<AtomicBool>) -> (u64, u64) {
    let mut bytes = 0u64;
    let mut files = 0u64;
    let walker = walkdir::WalkDir::new(dir).follow_links(false);

    for entry in walker.into_iter().flatten() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                files = files.saturating_add(1);
                bytes = bytes.saturating_add(meta.len());
            }
        }
    }

    (bytes, files)
}

#[derive(Debug, Clone)]
pub struct LargeFileInfo {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
}

pub fn find_large_files(
    home: &Path,
    min_bytes: u64,
    cancel: Arc<AtomicBool>,
) -> Vec<LargeFileInfo> {
    let mut results = Vec::new();
    let walker = walkdir::WalkDir::new(home).follow_links(false);

    for entry in walker.into_iter().flatten() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() && meta.len() >= min_bytes {
                let path = entry.path().to_path_buf();
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                results.push(LargeFileInfo {
                    name,
                    path,
                    bytes: meta.len(),
                });
            }
        }
    }

    results.sort_by_key(|f| std::cmp::Reverse(f.bytes));
    results.truncate(30);
    results
}
