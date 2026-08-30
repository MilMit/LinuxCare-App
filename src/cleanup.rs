use crate::model::{CleanupCandidate, CleanupCategory, ExecutionKind};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CleanupError {
    #[error("candidate is not executable by the unprivileged v0.1 executor")]
    NotExecutable,
    #[error("candidate has no filesystem path")]
    MissingPath,
    #[error("refusing to clean a path outside LinuxCare's user allowlist: {0}")]
    OutsideAllowlist(PathBuf),
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
}

pub fn clean_candidate(candidate: &CleanupCandidate, home: &Path) -> Result<u64, CleanupError> {
    if candidate.execution != ExecutionKind::UserAllowedDirectory {
        return Err(CleanupError::NotExecutable);
    }

    let path = candidate.path.as_deref().ok_or(CleanupError::MissingPath)?;
    let target = fs::canonicalize(path)?;
    if !is_allowed_cleanup_target(&target, home)? {
        return Err(CleanupError::OutsideAllowlist(target));
    }

    let before = candidate.size_bytes;
    detach_and_remove_directory(&target)?;
    let _ = fs::create_dir_all(&target);

    // Freedesktop Trash pairs files with .trashinfo metadata. Leaving those records
    // behind would produce an inconsistent Trash, so Trash has a dedicated second
    // exact allowlisted target.
    if candidate.category == CleanupCategory::Trash {
        let info = home.join(".local/share/Trash/info");
        if info.exists() {
            let info = fs::canonicalize(info)?;
            if is_allowed_cleanup_target(&info, home)? {
                detach_and_remove_directory(&info)?;
                let _ = fs::create_dir_all(&info);
            }
        }
    }

    Ok(before)
}

fn is_allowed_cleanup_target(target: &Path, home: &Path) -> Result<bool, io::Error> {
    let home = fs::canonicalize(home)?;
    let canonical = fs::canonicalize(target)?;
    if !canonical.starts_with(&home) {
        return Ok(false);
    }
    let meta = fs::symlink_metadata(target)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.uid() != unsafe { libc::geteuid() } {
            return Ok(false);
        }
    }

    let cache_dir = home.join(".cache");
    let trash_files = home.join(".local/share/Trash/files");
    let trash_info = home.join(".local/share/Trash/info");
    let cargo_cache = home.join(".cargo/registry/cache");
    let npm_cache = home.join(".npm/_cacache");
    let user_coredumps = home.join(".local/share/coredump");

    // Allow any safe subdirectory directly under ~/.cache/
    if canonical.starts_with(&cache_dir) && canonical != cache_dir {
        return Ok(true);
    }
    if canonical == trash_files
        || canonical == trash_info
        || canonical == cargo_cache
        || canonical == npm_cache
        || canonical == user_coredumps
    {
        return Ok(true);
    }

    Ok(false)
}

/// Atomically detaches the active allowlisted directory before recursive removal.
///
/// This is deliberately different from walking `target` and deleting children in place.
/// On Linux, `renameat2(RENAME_NOREPLACE)` moves the directory entry itself and therefore
/// does not follow a last-moment symlink replacement at the cleanup root. The detached
/// object is inspected again before it is removed.
fn detach_and_remove_directory(target: &Path) -> Result<(), io::Error> {
    let original_meta = fs::symlink_metadata(target)?;
    if original_meta.file_type().is_symlink() || !original_meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cleanup root is not a real directory",
        ));
    }

    let parent = target
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "cleanup root has no parent"))?;
    let leaf = target
        .file_name()
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "cleanup root has no file name")
        })?
        .to_string_lossy();

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let mut staged = None;
    for attempt in 0..32u32 {
        let candidate = parent.join(format!(
            ".linuxcare-cleaning-{}-{nonce}-{attempt}-{leaf}",
            std::process::id()
        ));
        match rename_noreplace(target, &candidate) {
            Ok(()) => {
                staged = Some(candidate);
                break;
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }

    let staged = staged.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve a unique cleanup staging path",
        )
    })?;

    // Recreate the well-known directory immediately. If an application races us and
    // already recreates it, accept only a normal directory owned by the current user.
    match fs::create_dir(target) {
        Ok(()) => {
            let _ = fs::set_permissions(target, original_meta.permissions());
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            let replacement = fs::symlink_metadata(target)?;
            if replacement.file_type().is_symlink()
                || !replacement.is_dir()
                || !owned_by_current_user(&replacement)
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "cleanup target was replaced with an unsafe object; preserved detached data at {}",
                        staged.display()
                    ),
                ));
            }
        }
        Err(err) => {
            // Best-effort rollback. If rollback cannot happen, the detached data remains
            // beside the original path instead of being destroyed.
            let _ = rename_noreplace(&staged, target);
            return Err(err);
        }
    }

    remove_path_without_following(&staged)
}

fn owned_by_current_user(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.uid() == unsafe { libc::geteuid() }
    }

    #[cfg(not(unix))]
    {
        let _ = meta;
        true
    }
}

fn remove_path_without_following(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || meta.is_file() {
        fs::remove_file(path)
    } else if meta.is_dir() {
        // Rust's Unix implementation of remove_dir_all uses *at-style syscalls on
        // supported platforms and does not follow symbolic links encountered below root.
        fs::remove_dir_all(path)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to remove an unsupported filesystem object",
        ))
    }
}

#[cfg(target_os = "linux")]
fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let from = CString::new(from.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains NUL"))?;
    let to = CString::new(to.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "destination path contains NUL")
    })?;

    let rc = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };

    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "linux"))]
fn rename_noreplace(_from: &Path, _to: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "safe cleanup staging currently requires Linux renameat2",
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::model::{CleanupCategory, CleanupRisk};
    use std::{
        os::unix::fs::symlink,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_home(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("linuxcare-{label}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn candidate(path: PathBuf) -> CleanupCandidate {
        CleanupCandidate {
            id: "test".into(),
            category: CleanupCategory::ThumbnailCache,
            title: "test".into(),
            description: String::new(),
            path: Some(path),
            size_bytes: 1,
            risk: CleanupRisk::Safe,
            reason: String::new(),
            consequence: String::new(),
            requires_privilege: false,
            execution: ExecutionKind::UserAllowedDirectory,
            privileged_action: None,
        }
    }

    #[test]
    fn cleans_exact_allowlisted_directory_contents() {
        let home = temp_home("allow");
        let thumbs = home.join(".cache/thumbnails");
        fs::create_dir_all(&thumbs).unwrap();
        fs::write(thumbs.join("preview.png"), b"x").unwrap();

        clean_candidate(&candidate(thumbs.clone()), &home).unwrap();
        assert!(thumbs.exists());
        assert!(fs::read_dir(&thumbs).unwrap().next().is_none());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn refuses_symlinked_allowlist_root() {
        let home = temp_home("symlink-home");
        let outside = temp_home("symlink-outside");
        fs::create_dir_all(home.join(".cache")).unwrap();
        symlink(&outside, home.join(".cache/thumbnails")).unwrap();
        fs::write(outside.join("do-not-delete"), b"x").unwrap();

        let err = clean_candidate(&candidate(home.join(".cache/thumbnails")), &home).unwrap_err();
        assert!(matches!(err, CleanupError::OutsideAllowlist(_)));
        assert!(outside.join("do-not-delete").exists());
        let _ = fs::remove_dir_all(home);
        let _ = fs::remove_dir_all(outside);
    }
}
