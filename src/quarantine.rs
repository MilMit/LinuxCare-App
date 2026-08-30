use crate::model::{CleanupCandidate, CleanupCategory, ExecutionKind};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

const MAX_RECORDS: usize = 128;
const QUARANTINE_PREFIX: &str = ".linuxcare-quarantine-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuarantineState {
    Preparing,
    Active,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantinePath {
    pub original: PathBuf,
    pub quarantined: PathBuf,
    #[serde(default)]
    pub mode: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineRecord {
    pub id: String,
    pub created_unix: u64,
    pub expires_unix: u64,
    pub title: String,
    pub category: CleanupCategory,
    pub estimated_bytes: u64,
    pub state: QuarantineState,
    pub paths: Vec<QuarantinePath>,
}

#[derive(Debug, Clone)]
pub struct QuarantineOutcome {
    pub id: String,
    pub protected_bytes: u64,
    pub expires_unix: u64,
}

#[derive(Debug, Clone)]
pub struct PurgeOutcome {
    pub id: String,
    pub title: String,
    pub bytes: u64,
    pub expired: bool,
}

#[derive(Debug, Error)]
pub enum QuarantineError {
    #[error("candidate is not eligible for Safety Quarantine")]
    NotEligible,
    #[error("candidate has no filesystem path")]
    MissingPath,
    #[error("refusing to quarantine a path outside LinuxCare's user allowlist: {0}")]
    OutsideAllowlist(PathBuf),
    #[error("undo is blocked because {0} has been recreated and is no longer empty")]
    RestoreConflict(PathBuf),
    #[error("quarantine record was not found")]
    RecordNotFound,
    #[error("Safety Quarantine limit would be exceeded ({current} + {incoming} > {limit} bytes)")]
    QuarantineLimitExceeded {
        current: u64,
        incoming: u64,
        limit: u64,
    },
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
}

pub fn quarantine_candidate(
    candidate: &CleanupCandidate,
    home: &Path,
) -> Result<QuarantineOutcome, QuarantineError> {
    if candidate.execution != ExecutionKind::UserAllowedDirectory {
        return Err(QuarantineError::NotEligible);
    }

    let primary = candidate
        .path
        .as_deref()
        .ok_or(QuarantineError::MissingPath)?;
    let primary = fs::canonicalize(primary)?;
    validate_target(&primary, home)?;

    let mut targets = vec![primary];
    if candidate.category == CleanupCategory::Trash {
        let info = home.join(".local/share/Trash/info");
        if info.exists() {
            let info = fs::canonicalize(info)?;
            validate_target(&info, home)?;
            targets.push(info);
        }
    }

    let policy = crate::cleanup_policy::load(home).unwrap_or_default();
    let current_protected = total_protected_bytes(home);
    if current_protected.saturating_add(candidate.size_bytes) > policy.quarantine_limit_bytes {
        return Err(QuarantineError::QuarantineLimitExceeded {
            current: current_protected,
            incoming: candidate.size_bytes,
            limit: policy.quarantine_limit_bytes,
        });
    }

    let now = unix_now();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let id = format!("q-{now}-{}-{nonce}", std::process::id());

    let mut paths = Vec::with_capacity(targets.len());
    for (index, target) in targets.iter().enumerate() {
        let meta = fs::symlink_metadata(target)?;
        let mode = mode_of(&meta);
        let parent = target.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "cleanup root has no parent")
        })?;
        let leaf = target
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "cleanup root has no name"))?
            .to_string_lossy();
        let quarantined = parent.join(format!(
            "{QUARANTINE_PREFIX}{now}-{}-{index}-{leaf}",
            std::process::id()
        ));
        if quarantined.exists() {
            return Err(QuarantineError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "quarantine destination unexpectedly exists",
            )));
        }
        paths.push(QuarantinePath {
            original: target.clone(),
            quarantined,
            mode,
        });
    }

    let mut record = QuarantineRecord {
        id: id.clone(),
        created_unix: now,
        expires_unix: now.saturating_add(policy.quarantine_retention_secs),
        title: candidate.title.clone(),
        category: candidate.category,
        estimated_bytes: candidate.size_bytes,
        state: QuarantineState::Preparing,
        paths,
    };

    // Persist intent before detaching anything. If LinuxCare crashes mid-operation,
    // the next launch can still discover any quarantine path that was already moved.
    let mut records = load(home).unwrap_or_default();
    records.insert(0, record.clone());
    records.truncate(MAX_RECORDS);
    save(home, &records)?;

    for (moved, entry) in record.paths.iter().enumerate() {
        if let Err(err) = detach_to(entry) {
            let rollback_ok = rollback_prefix(&record.paths[..moved]);
            records.retain(|item| item.id != record.id);
            if !rollback_ok {
                record.paths.retain(|path| path.quarantined.exists());
                record.state = QuarantineState::Active;
                records.insert(0, record.clone());
            }
            let _ = save(home, &records);
            return Err(QuarantineError::Io(err));
        }
    }

    record.state = QuarantineState::Active;
    if let Some(stored) = records.iter_mut().find(|item| item.id == record.id) {
        *stored = record.clone();
    }
    save(home, &records)?;

    Ok(QuarantineOutcome {
        id,
        protected_bytes: candidate.size_bytes,
        expires_unix: record.expires_unix,
    })
}

pub fn load(home: &Path) -> io::Result<Vec<QuarantineRecord>> {
    let path = manifest_path(home);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let payload = fs::read(path)?;
    let mut records: Vec<QuarantineRecord> = serde_json::from_slice(&payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // A Preparing record can be left behind by a crash. Keep only paths that actually
    // reached quarantine and expose them as Active so the user can recover or purge them.
    for record in &mut records {
        record.paths.retain(|entry| entry.quarantined.exists());
        if record.state == QuarantineState::Preparing && !record.paths.is_empty() {
            record.state = QuarantineState::Active;
        }
    }
    records.retain(|record| !record.paths.is_empty());
    Ok(records)
}

pub fn restore(home: &Path, id: &str) -> Result<u64, QuarantineError> {
    let mut records = load(home)?;
    let index = records
        .iter()
        .position(|record| record.id == id)
        .ok_or(QuarantineError::RecordNotFound)?;
    let record = records[index].clone();

    for entry in &record.paths {
        if !entry.quarantined.exists() {
            return Err(QuarantineError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "quarantine data is missing: {}",
                    entry.quarantined.display()
                ),
            )));
        }
        validate_quarantined_path(&entry.quarantined, home)?;
        if entry.original.exists() {
            validate_restore_placeholder(&entry.original, home)?;
            if fs::read_dir(&entry.original)?.next().is_some() {
                return Err(QuarantineError::RestoreConflict(entry.original.clone()));
            }
        }
    }

    for (restored, entry) in record.paths.iter().enumerate() {
        if entry.original.exists() {
            fs::remove_dir(&entry.original)?;
        }
        if let Err(err) = rename_noreplace(&entry.quarantined, &entry.original) {
            rollback_restored(&record.paths[..restored]);
            return Err(QuarantineError::Io(err));
        }
    }

    records.remove(index);
    save(home, &records)?;
    Ok(record.estimated_bytes)
}

pub fn restore_aside(home: &Path, id: &str) -> Result<(u64, Vec<PathBuf>), QuarantineError> {
    let mut records = load(home)?;
    let index = records
        .iter()
        .position(|record| record.id == id)
        .ok_or(QuarantineError::RecordNotFound)?;
    let record = records[index].clone();
    let now = unix_now();
    let mut destinations = Vec::with_capacity(record.paths.len());

    for (position, entry) in record.paths.iter().enumerate() {
        if !entry.quarantined.exists() {
            return Err(QuarantineError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "quarantine data is missing: {}",
                    entry.quarantined.display()
                ),
            )));
        }
        validate_quarantined_path(&entry.quarantined, home)?;
        let parent = entry
            .original
            .parent()
            .ok_or_else(|| io::Error::other("restore target has no parent"))?;
        validate_restore_parent(parent, home)?;
        let leaf = entry
            .original
            .file_name()
            .ok_or_else(|| io::Error::other("restore target has no name"))?
            .to_string_lossy();
        let destination = parent.join(format!("{leaf}.linuxcare-restored-{now}-{position}"));
        if destination.exists() {
            return Err(QuarantineError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "recovery destination already exists: {}",
                    destination.display()
                ),
            )));
        }
        destinations.push(destination);
    }

    for (moved, (entry, destination)) in record.paths.iter().zip(destinations.iter()).enumerate() {
        if let Err(err) = rename_noreplace(&entry.quarantined, destination) {
            for (rollback_entry, rollback_destination) in record
                .paths
                .iter()
                .zip(destinations.iter())
                .take(moved)
                .rev()
            {
                let _ = rename_noreplace(rollback_destination, &rollback_entry.quarantined);
            }
            return Err(QuarantineError::Io(err));
        }
    }

    records.remove(index);
    save(home, &records)?;
    Ok((record.estimated_bytes, destinations))
}

pub fn purge(home: &Path, id: &str) -> Result<u64, QuarantineError> {
    let mut records = load(home)?;
    let index = records
        .iter()
        .position(|record| record.id == id)
        .ok_or(QuarantineError::RecordNotFound)?;
    let record = records[index].clone();

    for entry in &record.paths {
        if entry.quarantined.exists() {
            validate_quarantined_path(&entry.quarantined, home)?;
            remove_without_following(&entry.quarantined)?;
        }
    }

    records.remove(index);
    save(home, &records)?;
    Ok(record.estimated_bytes)
}

pub fn purge_expired(home: &Path) -> io::Result<Vec<PurgeOutcome>> {
    let now = unix_now();
    let records = load(home)?;
    let expired: Vec<_> = records
        .iter()
        .filter(|record| record.expires_unix <= now)
        .cloned()
        .collect();

    let mut outcomes = Vec::new();
    for record in expired {
        if purge(home, &record.id).is_ok() {
            outcomes.push(PurgeOutcome {
                id: record.id,
                title: record.title,
                bytes: record.estimated_bytes,
                expired: true,
            });
        }
    }
    Ok(outcomes)
}

pub fn total_protected_bytes(home: &Path) -> u64 {
    load(home)
        .unwrap_or_default()
        .into_iter()
        .map(|record| record.estimated_bytes)
        .sum()
}

pub fn is_internal_quarantine_name(name: &str) -> bool {
    name.starts_with(QUARANTINE_PREFIX)
}

fn validate_target(target: &Path, home: &Path) -> Result<(), QuarantineError> {
    let home = fs::canonicalize(home)?;
    let canonical = fs::canonicalize(target)?;
    if !canonical.starts_with(&home) {
        return Err(QuarantineError::OutsideAllowlist(canonical));
    }
    let meta = fs::symlink_metadata(&canonical)?;
    if meta.file_type().is_symlink() || !meta.is_dir() || !owned_by_current_user(&meta) {
        return Err(QuarantineError::OutsideAllowlist(canonical));
    }

    let cache_dir = home.join(".cache");
    let trash_files = home.join(".local/share/Trash/files");
    let trash_info = home.join(".local/share/Trash/info");
    let cargo_cache = home.join(".cargo/registry/cache");
    let npm_cache = home.join(".npm/_cacache");
    let user_coredumps = home.join(".local/share/coredump");

    if (canonical.starts_with(&cache_dir) && canonical != cache_dir)
        || canonical == trash_files
        || canonical == trash_info
        || canonical == cargo_cache
        || canonical == npm_cache
        || canonical == user_coredumps
    {
        return Ok(());
    }
    Err(QuarantineError::OutsideAllowlist(canonical))
}

fn validate_quarantined_path(path: &Path, home: &Path) -> Result<(), QuarantineError> {
    let home = fs::canonicalize(home)?;
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "quarantine path has no parent")
    })?;
    let parent = fs::canonicalize(parent)?;
    if !parent.starts_with(&home) {
        return Err(QuarantineError::OutsideAllowlist(path.to_path_buf()));
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if !is_internal_quarantine_name(name) {
        return Err(QuarantineError::OutsideAllowlist(path.to_path_buf()));
    }
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_dir() || !owned_by_current_user(&meta) {
        return Err(QuarantineError::OutsideAllowlist(path.to_path_buf()));
    }
    Ok(())
}

fn validate_restore_parent(path: &Path, home: &Path) -> Result<(), QuarantineError> {
    let home = fs::canonicalize(home)?;
    let canonical = fs::canonicalize(path)?;
    if !canonical.starts_with(&home) {
        return Err(QuarantineError::OutsideAllowlist(canonical));
    }
    let meta = fs::symlink_metadata(&canonical)?;
    if meta.file_type().is_symlink() || !meta.is_dir() || !owned_by_current_user(&meta) {
        return Err(QuarantineError::OutsideAllowlist(canonical));
    }
    Ok(())
}

fn validate_restore_placeholder(path: &Path, home: &Path) -> Result<(), QuarantineError> {
    let home = fs::canonicalize(home)?;
    let canonical = fs::canonicalize(path)?;
    if !canonical.starts_with(&home) {
        return Err(QuarantineError::OutsideAllowlist(canonical));
    }
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_dir() || !owned_by_current_user(&meta) {
        return Err(QuarantineError::OutsideAllowlist(path.to_path_buf()));
    }
    Ok(())
}

fn detach_to(entry: &QuarantinePath) -> io::Result<()> {
    let meta = fs::symlink_metadata(&entry.original)?;
    if meta.file_type().is_symlink() || !meta.is_dir() || !owned_by_current_user(&meta) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "cleanup target changed before quarantine",
        ));
    }
    rename_noreplace(&entry.original, &entry.quarantined)?;
    match fs::create_dir(&entry.original) {
        Ok(()) => set_mode(&entry.original, entry.mode),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            let replacement = fs::symlink_metadata(&entry.original)?;
            if replacement.file_type().is_symlink()
                || !replacement.is_dir()
                || !owned_by_current_user(&replacement)
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "application recreated cleanup target as an unsafe object",
                ));
            }
            Ok(())
        }
        Err(err) => {
            let _ = rename_noreplace(&entry.quarantined, &entry.original);
            Err(err)
        }
    }
}

fn rollback_prefix(entries: &[QuarantinePath]) -> bool {
    entries.iter().rev().all(rollback_one)
}

fn rollback_one(entry: &QuarantinePath) -> bool {
    if !entry.quarantined.exists() {
        return true;
    }
    if entry.original.exists() {
        let Ok(meta) = fs::symlink_metadata(&entry.original) else {
            return false;
        };
        if meta.file_type().is_symlink() || !meta.is_dir() || !owned_by_current_user(&meta) {
            return false;
        }
        let Ok(mut iter) = fs::read_dir(&entry.original) else {
            return false;
        };
        if iter.next().is_some() || fs::remove_dir(&entry.original).is_err() {
            return false;
        }
    }
    rename_noreplace(&entry.quarantined, &entry.original).is_ok()
}

fn rollback_restored(entries: &[QuarantinePath]) {
    for entry in entries.iter().rev() {
        if entry.original.exists() && !entry.quarantined.exists() {
            let _ = rename_noreplace(&entry.original, &entry.quarantined);
        }
    }
}

fn remove_without_following(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || meta.is_file() {
        fs::remove_file(path)
    } else if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported quarantine filesystem object",
        ))
    }
}

fn save(home: &Path, records: &[QuarantineRecord]) -> io::Result<()> {
    let path = manifest_path(home);
    let dir = path.parent().expect("quarantine manifest has parent");
    fs::create_dir_all(dir)?;
    set_private_dir(dir)?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = dir.join(format!("quarantine-{}-{nonce}.tmp", std::process::id()));
    let payload = serde_json::to_vec_pretty(records)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut file = OpenOptions::new().create_new(true).write(true).open(&tmp)?;
    set_private_file(&tmp)?;
    file.write_all(&payload)?;
    file.sync_all()?;
    fs::rename(&tmp, &path)?;
    set_private_file(&path)?;
    Ok(())
}

fn manifest_path(home: &Path) -> PathBuf {
    state_root(home).join("quarantine.json")
}

fn state_root(home: &Path) -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_STATE_HOME") {
        PathBuf::from(dir).join("linuxcare")
    } else {
        home.join(".local/state/linuxcare")
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(unix)]
fn mode_of(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode()
}

#[cfg(not(unix))]
fn mode_of(_meta: &fs::Metadata) -> u32 {
    0
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
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
        "Safety Quarantine requires Linux renameat2",
    ))
}

#[cfg(unix)]
fn set_private_dir(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_dir(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::model::{CleanupRisk, PrivilegedAction};

    fn temp_home(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!(
            "linuxcare-quarantine-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(home.join(".cache/test-app")).unwrap();
        home
    }

    fn candidate(path: PathBuf) -> CleanupCandidate {
        CleanupCandidate {
            id: "test".into(),
            category: CleanupCategory::UserCache,
            title: "Test cache".into(),
            description: String::new(),
            path: Some(path),
            size_bytes: 4,
            risk: CleanupRisk::Safe,
            reason: String::new(),
            consequence: String::new(),
            requires_privilege: false,
            execution: ExecutionKind::UserAllowedDirectory,
            privileged_action: None::<PrivilegedAction>,
        }
    }

    #[test]
    fn quarantine_then_restore_round_trip() {
        let home = temp_home("restore");
        let target = home.join(".cache/test-app");
        fs::write(target.join("payload"), b"data").unwrap();
        let outcome = quarantine_candidate(&candidate(target.clone()), &home).unwrap();
        assert!(target.exists());
        assert!(fs::read_dir(&target).unwrap().next().is_none());
        restore(&home, &outcome.id).unwrap();
        assert_eq!(fs::read(target.join("payload")).unwrap(), b"data");
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn restore_refuses_non_empty_recreated_target() {
        let home = temp_home("conflict");
        let target = home.join(".cache/test-app");
        fs::write(target.join("old"), b"data").unwrap();
        let outcome = quarantine_candidate(&candidate(target.clone()), &home).unwrap();
        fs::write(target.join("new"), b"new").unwrap();
        let err = restore(&home, &outcome.id).unwrap_err();
        assert!(matches!(err, QuarantineError::RestoreConflict(_)));
        assert!(!load(&home).unwrap().is_empty());
        let _ = fs::remove_dir_all(home);
    }
}
