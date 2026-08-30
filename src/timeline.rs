use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_EVENTS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceKind {
    Scan,
    Cleanup,
    Restore,
    Purge,
}

impl MaintenanceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Scan => "SCAN",
            Self::Cleanup => "CLEANUP",
            Self::Restore => "UNDO",
            Self::Purge => "PURGE",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Scan => "system-search-symbolic",
            Self::Cleanup => "user-trash-symbolic",
            Self::Restore => "edit-undo-symbolic",
            Self::Purge => "edit-delete-symbolic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceEvent {
    pub timestamp_unix: u64,
    #[serde(default = "default_version")]
    pub version: String,
    pub kind: MaintenanceKind,
    pub title: String,
    pub detail: String,
    #[serde(default)]
    pub actual_freed_bytes: u64,
    #[serde(default)]
    pub protected_bytes: u64,
    #[serde(default)]
    pub restored_bytes: u64,
    #[serde(default)]
    pub observed_bytes: u64,
    #[serde(default)]
    pub operations: usize,
    #[serde(default)]
    pub errors: usize,
    #[serde(default)]
    pub privileged: bool,
}

impl MaintenanceEvent {
    pub fn new(kind: MaintenanceKind, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            timestamp_unix: unix_now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind,
            title: title.into(),
            detail: detail.into(),
            actual_freed_bytes: 0,
            protected_bytes: 0,
            restored_bytes: 0,
            observed_bytes: 0,
            operations: 0,
            errors: 0,
            privileged: false,
        }
    }
}

pub fn load(home: &Path) -> io::Result<Vec<MaintenanceEvent>> {
    let path = timeline_path(home);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let payload = fs::read(path)?;
    serde_json::from_slice(&payload).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn append(home: &Path, event: MaintenanceEvent) -> io::Result<()> {
    let mut events = load(home).unwrap_or_default();
    events.insert(0, event);
    events.truncate(MAX_EVENTS);
    save(home, &events)
}

fn save(home: &Path, events: &[MaintenanceEvent]) -> io::Result<()> {
    let path = timeline_path(home);
    let dir = path.parent().expect("timeline path has parent");
    fs::create_dir_all(dir)?;
    set_private_dir(dir)?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = dir.join(format!("timeline-{}-{nonce}.tmp", std::process::id()));
    let payload = serde_json::to_vec_pretty(events)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut file = OpenOptions::new().create_new(true).write(true).open(&tmp)?;
    set_private_file(&tmp)?;
    file.write_all(&payload)?;
    file.sync_all()?;
    fs::rename(&tmp, &path)?;
    set_private_file(&path)?;
    Ok(())
}

fn timeline_path(home: &Path) -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_STATE_HOME") {
        PathBuf::from(dir).join("linuxcare/timeline.json")
    } else {
        home.join(".local/state/linuxcare/timeline.json")
    }
}

fn default_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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
