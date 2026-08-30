use crate::{
    model::{CleanupCandidate, CleanupCategory, CleanupRisk, ExecutionKind, ScannerResult},
    scan::{fs_stats, ScanContext, Scanner},
};
use std::time;

pub struct DevCacheScanner;

impl Scanner for DevCacheScanner {
    fn name(&self) -> &'static str {
        "Developer package caches"
    }

    fn scan(&self, ctx: &ScanContext) -> ScannerResult {
        let started = time::Instant::now();
        let mut result = ScannerResult {
            module: self.name().into(),
            ..Default::default()
        };

        let targets = [
            (
                "Cargo package cache",
                ".cargo/registry/cache",
                "Downloaded crate archives for Rust/Cargo projects.",
                "Cargo will automatically redownload any required crate tarballs on next build.",
                "cargo-package-cache",
            ),
            (
                "NPM package cache",
                ".npm/_cacache",
                "Cached tarballs and metadata for Node.js/NPM packages.",
                "NPM will redownload packages when running npm install without cache.",
                "npm-package-cache",
            ),
            (
                "Pip download cache",
                ".cache/pip",
                "Downloaded wheels and source tarballs for Python packages.",
                "Pip will download packages directly from PyPI when installing new packages.",
                "pip-download-cache",
            ),
            (
                "Uv package cache",
                ".cache/uv",
                "Cached wheels and index archives for uv Python package manager.",
                "uv will re-fetch wheels as needed during environment creation.",
                "uv-package-cache",
            ),
            (
                "Yarn package cache",
                ".cache/yarn",
                "Cached packages and tarballs for Yarn package manager.",
                "Yarn will re-download modules when running yarn install.",
                "yarn-package-cache",
            ),
        ];

        for (title, rel_path, desc, cons, id) in targets {
            let path = ctx.home.join(rel_path);
            if !path.exists() {
                continue;
            }

            let stats = fs_stats::measure(&path, ctx);
            result.files_scanned = result.files_scanned.saturating_add(stats.files);
            result.directories_scanned =
                result.directories_scanned.saturating_add(stats.directories);

            if stats.errors > 0 {
                result.warnings.push(format!(
                    "{title} measurement encountered {} filesystem error(s); the displayed size may be an undercount.",
                    stats.errors
                ));
            }

            if stats.bytes > 0 {
                result.candidates.push(CleanupCandidate {
                    id: id.into(),
                    category: CleanupCategory::DeveloperCache,
                    title: title.into(),
                    description: desc.into(),
                    path: Some(path),
                    size_bytes: stats.bytes,
                    risk: CleanupRisk::Safe,
                    reason: "These are temporary build and download caches that are automatically reconstructed upon use.".into(),
                    consequence: cons.into(),
                    requires_privilege: false,
                    execution: ExecutionKind::UserAllowedDirectory,
                    privileged_action: None,
                });
            }
        }

        result.duration_ms = started.elapsed().as_millis();
        result
    }
}
