mod app_caches;
mod apt_cache;
mod crash_reports;
mod dev_cache;
mod flatpak;
mod journal;
mod snap;
mod thumbnails;
mod trash;
mod user_cache;

use super::Scanner;

pub fn default_scanners() -> Vec<Box<dyn Scanner>> {
    vec![
        Box::new(user_cache::UserCacheScanner),
        Box::new(app_caches::AppCacheScanner),
        Box::new(dev_cache::DevCacheScanner),
        Box::new(thumbnails::ThumbnailScanner),
        Box::new(trash::TrashScanner),
        Box::new(apt_cache::AptCacheScanner),
        Box::new(journal::JournalScanner),
        Box::new(snap::SnapScanner),
        Box::new(flatpak::FlatpakScanner),
        Box::new(crash_reports::CrashReportScanner),
    ]
}
