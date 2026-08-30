use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_EVENTS: usize = 100;
const RATE_LIMIT_SECS: u64 = 4 * 60 * 60;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationSeverity {
    Info,
    Attention,
    Critical,
}

impl NotificationSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Attention => "ATTENTION",
            Self::Critical => "CRITICAL",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::Info => "health-neutral",
            Self::Attention => "risk-review",
            Self::Critical => "risk-dangerous",
        }
    }

    fn urgency(self) -> &'static str {
        if self == Self::Critical {
            "critical"
        } else {
            "normal"
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub timestamp_unix: u64,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub severity: NotificationSeverity,
    pub desktop_delivered: bool,
}

pub fn emit(
    home: &Path,
    kind: &str,
    title: &str,
    body: &str,
    severity: NotificationSeverity,
    force: bool,
) -> io::Result<bool> {
    let now = now_unix();
    let mut events = load(home).unwrap_or_default();
    let rate_limited = !force
        && events.iter().any(|event| {
            event.kind == kind
                && event.title == title
                && now.saturating_sub(event.timestamp_unix) < RATE_LIMIT_SECS
        });

    if rate_limited {
        return Ok(false);
    }
    let delivered = desktop_notify(title, body, severity);

    events.push(NotificationEvent {
        timestamp_unix: now,
        kind: kind.to_string(),
        title: title.to_string(),
        body: body.to_string(),
        severity,
        desktop_delivered: delivered,
    });
    if events.len() > MAX_EVENTS {
        let drain = events.len() - MAX_EVENTS;
        events.drain(0..drain);
    }
    save(home, &events)?;
    Ok(delivered)
}

pub fn load(home: &Path) -> io::Result<Vec<NotificationEvent>> {
    let path = state_path(home);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let payload = fs::read(path)?;
    serde_json::from_slice(&payload).map_err(io::Error::other)
}

pub fn clear(home: &Path) -> io::Result<()> {
    save(home, &[])
}

fn desktop_notify(title: &str, body: &str, severity: NotificationSeverity) -> bool {
    Command::new("notify-send")
        .args([
            "--app-name=LinuxCare",
            "--icon=net.milmit.LinuxCare",
            "--urgency",
            severity.urgency(),
            title,
            body,
        ])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn state_path(home: &Path) -> PathBuf {
    state_root(home).join("notifications.json")
}

fn state_root(home: &Path) -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"))
        .join("linuxcare")
}

fn save(home: &Path, events: &[NotificationEvent]) -> io::Result<()> {
    let path = state_path(home);
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("notification state path has no parent"))?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let payload = serde_json::to_vec_pretty(events).map_err(io::Error::other)?;
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

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
