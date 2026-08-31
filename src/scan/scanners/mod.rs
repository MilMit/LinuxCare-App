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

fn is_store_edition() -> bool {
    std::env::var_os("LINUXCARE_STORE_EDITION").as_deref() == Some(std::ffi::OsStr::new("1"))
}

pub fn default_scanners() -> Vec<Box<dyn Scanner>> {
    let mut scanners: Vec<Box<dyn Scanner>> = vec![
        Box::new(user_cache::UserCacheScanner),
        Box::new(app_caches::AppCacheScanner),
        Box::new(dev_cache::DevCacheScanner),
        Box::new(thumbnails::ThumbnailScanner),
        Box::new(trash::TrashScanner),
        Box::new(crash_reports::CrashReportScanner),
    ];

    // Strict Snap Store builds intentionally do not advertise host mutations
    // they cannot safely perform inside the sandbox. The full Debian edition
    // retains APT, journal, disabled-Snap and Flatpak maintenance through the
    // existing privilege-separated providers.
    if !is_store_edition() {
        scanners.push(Box::new(apt_cache::AptCacheScanner));
        scanners.push(Box::new(journal::JournalScanner));
        scanners.push(Box::new(snap::SnapScanner));
        scanners.push(Box::new(flatpak::FlatpakScanner));
    }

    scanners
}
