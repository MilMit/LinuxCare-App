use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_HISTORY_ENTRIES: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp_unix: u64,
    #[serde(default = "default_version")]
    pub version: String,
    pub recovered_bytes: u64,
    #[serde(default)]
    pub estimated_bytes: u64,
    pub operations: usize,
    pub errors: usize,
    #[serde(default)]
    pub privileged_used: bool,
    #[serde(default)]
    pub results: Vec<crate::model::CleanupOperationResult>,
}

fn default_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

impl HistoryEntry {
    pub fn now(recovered_bytes: u64, operations: usize, errors: usize) -> Self {
        Self {
            timestamp_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            recovered_bytes,
            estimated_bytes: recovered_bytes,
            operations,
            errors,
            privileged_used: false,
            results: Vec::new(),
        }
    }

    pub fn with_details(
        recovered_bytes: u64,
        estimated_bytes: u64,
        operations: usize,
        errors: usize,
        privileged_used: bool,
        results: Vec<crate::model::CleanupOperationResult>,
    ) -> Self {
        Self {
            timestamp_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            recovered_bytes,
            estimated_bytes,
            operations,
            errors,
            privileged_used,
            results,
        }
    }
}

pub fn load(home: &Path) -> io::Result<Vec<HistoryEntry>> {
    let path = history_path(home);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read(&path)?;
    serde_json::from_slice(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn append(home: &Path, entry: HistoryEntry) -> io::Result<()> {
    let mut entries = load(home).unwrap_or_default();
    entries.insert(0, entry);
    entries.truncate(MAX_HISTORY_ENTRIES);
    save_atomic(home, &entries)
}

fn save_atomic(home: &Path, entries: &[HistoryEntry]) -> io::Result<()> {
    let path = history_path(home);
    let dir = path.parent().expect("history path always has a parent");
    fs::create_dir_all(dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = dir.join(format!("history-{}-{nonce}.tmp", std::process::id()));
    let payload = serde_json::to_vec_pretty(entries)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut file = OpenOptions::new().create_new(true).write(true).open(&tmp)?;
    file.write_all(&payload)?;
    file.sync_all()?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn history_path(home: &Path) -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_STATE_HOME") {
        PathBuf::from(dir).join("linuxcare/history.json")
    } else {
        home.join(".local/state/linuxcare/history.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("linuxcare-history-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn appends_newest_first() {
        let home = temp_home();
        // Avoid a caller's XDG_STATE_HOME leaking into this isolated test by testing the
        // default path helper directly via a subprocess-free local environment assumption.
        let old = std::env::var_os("XDG_STATE_HOME");
        std::env::remove_var("XDG_STATE_HOME");
        append(
            &home,
            HistoryEntry {
                timestamp_unix: 1,
                version: "0.1.0-alpha.3".into(),
                recovered_bytes: 10,
                estimated_bytes: 10,
                operations: 1,
                errors: 0,
                privileged_used: false,
                results: vec![],
            },
        )
        .unwrap();
        append(
            &home,
            HistoryEntry {
                timestamp_unix: 2,
                version: "0.1.0-alpha.3".into(),
                recovered_bytes: 20,
                estimated_bytes: 20,
                operations: 2,
                errors: 0,
                privileged_used: true,
                results: vec![],
            },
        )
        .unwrap();
        let entries = load(&home).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].timestamp_unix, 2);
        assert_eq!(entries[0].version, "0.1.0-alpha.3");
        if let Some(value) = old {
            std::env::set_var("XDG_STATE_HOME", value);
        }
        let _ = fs::remove_dir_all(home);
    }
}
