use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CleanupPolicy {
    pub quarantine_retention_secs: u64,
    pub quarantine_limit_bytes: u64,
    pub automation_max_bytes: u64,
    pub auto_purge_expired: bool,
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        Self {
            quarantine_retention_secs: 60 * 60,
            quarantine_limit_bytes: 5 * 1024 * 1024 * 1024,
            automation_max_bytes: 1024 * 1024 * 1024,
            auto_purge_expired: true,
        }
    }
}

pub fn load(home: &Path) -> io::Result<CleanupPolicy> {
    let path = config_path(home);
    if !path.exists() {
        return Ok(CleanupPolicy::default());
    }
    let payload = fs::read(path)?;
    let policy: CleanupPolicy = serde_json::from_slice(&payload).map_err(io::Error::other)?;
    Ok(normalize(policy))
}

pub fn save(home: &Path, policy: &CleanupPolicy) -> io::Result<()> {
    let path = config_path(home);
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("cleanup policy path has no parent"))?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let policy = normalize(policy.clone());
    let payload = serde_json::to_vec_pretty(&policy).map_err(io::Error::other)?;
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(&payload)?;
    file.sync_all()?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn normalize(mut policy: CleanupPolicy) -> CleanupPolicy {
    policy.quarantine_retention_secs = match policy.quarantine_retention_secs {
        1800 | 3600 | 86400 | 604800 => policy.quarantine_retention_secs,
        _ => CleanupPolicy::default().quarantine_retention_secs,
    };
    const GIB: u64 = 1024 * 1024 * 1024;
    policy.quarantine_limit_bytes = policy.quarantine_limit_bytes.clamp(GIB, 100 * GIB);
    policy.automation_max_bytes = policy
        .automation_max_bytes
        .clamp(128 * 1024 * 1024, 20 * GIB)
        .min(policy.quarantine_limit_bytes);
    policy
}

pub fn retention_label(seconds: u64) -> &'static str {
    match seconds {
        1800 => "30 minutes",
        3600 => "1 hour",
        86400 => "1 day",
        604800 => "7 days",
        _ => "Custom",
    }
}

fn config_path(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("linuxcare/cleanup-policy.json")
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    #[test]
    fn older_partial_policy_uses_safe_defaults() {
        let old = r#"{"quarantine_retention_secs": 3600}"#;
        let parsed: CleanupPolicy = serde_json::from_str(old).unwrap();
        let normalized = normalize(parsed);
        assert_eq!(normalized.quarantine_retention_secs, 3600);
        assert_eq!(
            normalized.quarantine_limit_bytes,
            CleanupPolicy::default().quarantine_limit_bytes
        );
        assert_eq!(
            normalized.automation_max_bytes,
            CleanupPolicy::default().automation_max_bytes
        );
        assert!(normalized.auto_purge_expired);
    }
}
