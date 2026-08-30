use crate::{
    cleanup_policy, health,
    model::{CleanupCandidate, CleanupRisk, ExecutionKind},
    notification_center::{self, NotificationSeverity},
    quarantine,
    scan::{self, ScanEvent},
    timeline::{self, MaintenanceEvent, MaintenanceKind},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCadence {
    Daily,
    Weekly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutomationMode {
    ScanOnly,
    ScanAndNotify,
    SafeQuarantine,
}

impl AutomationMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::ScanOnly => "Scan only",
            Self::ScanAndNotify => "Scan + notify",
            Self::SafeQuarantine => "Scan + safe quarantine",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomationConfig {
    pub enabled: bool,
    pub cadence: AutomationCadence,
    pub weekday: String,
    pub hour: u8,
    pub minute: u8,
    pub mode: AutomationMode,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cadence: AutomationCadence::Weekly,
            weekday: "Sun".to_string(),
            hour: 10,
            minute: 0,
            mode: AutomationMode::ScanAndNotify,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AutomationStatus {
    pub enabled: bool,
    pub active: bool,
    pub timer_path: Option<PathBuf>,
    pub next_run: Option<String>,
    pub last_trigger: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct ScheduledRunReport {
    pub candidates: usize,
    pub observed_bytes: u64,
    pub safe_candidates: usize,
    pub quarantined_operations: usize,
    pub quarantined_bytes: u64,
    pub errors: usize,
}

pub fn load(home: &Path) -> io::Result<AutomationConfig> {
    let path = config_path(home);
    if !path.exists() {
        return Ok(AutomationConfig::default());
    }
    let payload = fs::read(path)?;
    serde_json::from_slice(&payload).map_err(io::Error::other)
}

pub fn save(home: &Path, config: &AutomationConfig) -> io::Result<()> {
    let path = config_path(home);
    save_json_private(&path, config)
}

pub fn install_schedule(home: &Path, config: &AutomationConfig) -> io::Result<()> {
    if std::env::var_os("SNAP").is_some()
        || std::env::var_os("FLATPAK_ID").is_some()
        || Path::new("/.flatpak-info").exists()
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "user-systemd automation is disabled in sandboxed/development store builds; use the full .deb/host installation",
        ));
    }
    validate_config(config)?;
    let dir = user_systemd_dir(home);
    fs::create_dir_all(&dir)?;

    let executable = maintenance_binary_path()?;
    let executable_text = systemd_escape_path(&executable);
    let service = format!(
        "[Unit]\nDescription=LinuxCare scheduled maintenance\nDocumentation=https://milmit.net\nConditionPathIsExecutable={}\n\n[Service]\nType=oneshot\nExecStart={} --scheduled\nNice=10\nIOSchedulingClass=idle\nNoNewPrivileges=true\nPrivateTmp=true\nProtectSystem=strict\nProtectHome=read-only\nReadWritePaths=-%h/.cache -%h/.config/linuxcare -%h/.local/state/linuxcare -%h/.local/share/Trash -%h/.cargo/registry -%h/.npm\n\n",
        executable_text,
        executable_text
    );
    let timer = format!(
        "[Unit]\nDescription=Run LinuxCare maintenance on schedule\n\n[Timer]\nOnCalendar={}\nPersistent=true\nRandomizedDelaySec=5m\nAccuracySec=1m\nUnit=linuxcare-maintenance.service\n\n[Install]\nWantedBy=timers.target\n",
        on_calendar(config)
    );

    write_private(
        &dir.join("linuxcare-maintenance.service"),
        service.as_bytes(),
    )?;
    write_private(&dir.join("linuxcare-maintenance.timer"), timer.as_bytes())?;

    run_systemctl_user(["daemon-reload"])?;

    let mut enabled_config = config.clone();
    enabled_config.enabled = true;
    save(home, &enabled_config)?;
    if let Err(err) = run_systemctl_user(["enable", "--now", "linuxcare-maintenance.timer"]) {
        enabled_config.enabled = false;
        let _ = save(home, &enabled_config);
        return Err(err);
    }
    Ok(())
}

pub fn disable_schedule(home: &Path) -> io::Result<()> {
    let _ = run_systemctl_user(["disable", "--now", "linuxcare-maintenance.timer"]);
    let dir = user_systemd_dir(home);
    for name in [
        "linuxcare-maintenance.timer",
        "linuxcare-maintenance.service",
    ] {
        let path = dir.join(name);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    let _ = run_systemctl_user(["daemon-reload"]);
    let mut config = load(home).unwrap_or_default();
    config.enabled = false;
    save(home, &config)
}

pub fn status(home: &Path) -> AutomationStatus {
    let timer_path = user_systemd_dir(home).join("linuxcare-maintenance.timer");
    let enabled = command_success(
        "systemctl",
        &["--user", "is-enabled", "linuxcare-maintenance.timer"],
    );
    let active = command_success(
        "systemctl",
        &["--user", "is-active", "linuxcare-maintenance.timer"],
    );
    let detail = if enabled && active {
        "User systemd timer is enabled and active.".to_string()
    } else if timer_path.exists() {
        "Timer files exist but systemd does not report the timer as both enabled and active."
            .to_string()
    } else {
        "No LinuxCare user timer is installed.".to_string()
    };
    let timer_properties = timer_properties();
    AutomationStatus {
        enabled,
        active,
        timer_path: timer_path.exists().then_some(timer_path),
        next_run: timer_properties.0,
        last_trigger: timer_properties.1,
        detail,
    }
}

pub fn run_now() -> io::Result<()> {
    run_systemctl_user(["start", "--no-block", "linuxcare-maintenance.service"])
}

pub fn run_scheduled(home: &Path) -> io::Result<ScheduledRunReport> {
    if !home.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "scheduled maintenance HOME directory is unavailable",
        ));
    }

    let config = load(home).unwrap_or_default();
    if !config.enabled {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "scheduled maintenance is disabled in LinuxCare settings",
        ));
    }
    let policy = cleanup_policy::load(home).unwrap_or_default();
    if config.mode == AutomationMode::SafeQuarantine && policy.auto_purge_expired {
        if let Ok(purged) = quarantine::purge_expired(home) {
            for item in purged {
                let mut event = MaintenanceEvent::new(
                    MaintenanceKind::Purge,
                    format!("Scheduled quarantine expiry: {}", item.title),
                    "The configured Undo retention window expired; the protected copy was permanently removed.",
                );
                event.actual_freed_bytes = item.bytes;
                let _ = timeline::append(home, event);
            }
        }
    }
    let exclusions = crate::settings::load(home)
        .map(|settings| settings.exclusions)
        .unwrap_or_default();
    let (tx, rx) = mpsc::channel();
    let _handle = scan::start_scan(home.to_path_buf(), exclusions, tx);
    let mut candidates = Vec::<CleanupCandidate>::new();
    let mut scan_errors = 0usize;

    while let Ok(event) = rx.recv() {
        match event {
            ScanEvent::ModuleFinished { result, .. } => {
                scan_errors = scan_errors.saturating_add(result.warnings.len());
                candidates.extend(result.candidates);
            }
            ScanEvent::Finished => break,
            ScanEvent::Cancelled => {
                return Err(io::Error::other("scheduled Smart Scan was cancelled"));
            }
            ScanEvent::Started { .. } | ScanEvent::ModuleStarted { .. } => {}
        }
    }

    let _ = health::record_scan_snapshot(home, &candidates);
    let observed_bytes = candidates.iter().map(|item| item.size_bytes).sum();
    let safe: Vec<_> = candidates
        .iter()
        .filter(|item| {
            item.risk == CleanupRisk::Safe
                && item.execution == ExecutionKind::UserAllowedDirectory
                && item.executable_now()
                && automation_path_allowed(item, home)
        })
        .cloned()
        .collect();

    let mut report = ScheduledRunReport {
        candidates: candidates.len(),
        observed_bytes,
        safe_candidates: safe.len(),
        quarantined_operations: 0,
        quarantined_bytes: 0,
        errors: scan_errors,
    };

    let mut event = MaintenanceEvent::new(
        MaintenanceKind::Scan,
        "Scheduled Smart Scan",
        format!(
            "Background scan observed {} candidate(s). Privileged cleanup is never attempted by the user timer.",
            candidates.len()
        ),
    );
    event.observed_bytes = observed_bytes;
    event.operations = candidates.len();
    event.errors = scan_errors;
    let _ = timeline::append(home, event);

    if config.mode == AutomationMode::SafeQuarantine {
        let mut budget = policy.automation_max_bytes;
        for candidate in safe {
            if candidate.size_bytes > budget {
                continue;
            }
            match quarantine::quarantine_candidate(&candidate, home) {
                Ok(outcome) => {
                    budget = budget.saturating_sub(outcome.protected_bytes);
                    report.quarantined_operations = report.quarantined_operations.saturating_add(1);
                    report.quarantined_bytes = report
                        .quarantined_bytes
                        .saturating_add(outcome.protected_bytes);
                }
                Err(_) => report.errors = report.errors.saturating_add(1),
            }
        }
        if report.quarantined_bytes > 0 {
            let mut cleanup_event = MaintenanceEvent::new(
                MaintenanceKind::Cleanup,
                "Scheduled safe cleanup",
                "Only user-space SAFE candidates were moved to Safety Quarantine. No privileged cleanup ran in the background.",
            );
            cleanup_event.protected_bytes = report.quarantined_bytes;
            cleanup_event.operations = report.quarantined_operations;
            cleanup_event.errors = report.errors;
            let _ = timeline::append(home, cleanup_event);
        }
    }

    if config.mode != AutomationMode::ScanOnly {
        let title = if report.errors > 0 {
            "LinuxCare maintenance needs review"
        } else {
            "LinuxCare maintenance completed"
        };
        let body = if report.quarantined_bytes > 0 {
            format!(
                "{} observed; {} protected in Safety Quarantine for Undo.",
                crate::format::bytes(report.observed_bytes),
                crate::format::bytes(report.quarantined_bytes)
            )
        } else {
            format!(
                "{} observed across {} cleanup candidate(s).",
                crate::format::bytes(report.observed_bytes),
                report.candidates
            )
        };
        let severity = if report.errors > 0 {
            NotificationSeverity::Attention
        } else {
            NotificationSeverity::Info
        };
        let _ =
            notification_center::emit(home, "scheduled-maintenance", title, &body, severity, false);
    }

    Ok(report)
}

fn validate_config(config: &AutomationConfig) -> io::Result<()> {
    if config.hour > 23 || config.minute > 59 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid schedule time",
        ));
    }
    if !matches!(
        config.weekday.as_str(),
        "Mon" | "Tue" | "Wed" | "Thu" | "Fri" | "Sat" | "Sun"
    ) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid weekday",
        ));
    }
    Ok(())
}

fn on_calendar(config: &AutomationConfig) -> String {
    match config.cadence {
        AutomationCadence::Daily => format!("*-*-* {:02}:{:02}:00", config.hour, config.minute),
        AutomationCadence::Weekly => format!(
            "{} *-*-* {:02}:{:02}:00",
            config.weekday, config.hour, config.minute
        ),
    }
}

fn maintenance_binary_path() -> io::Result<PathBuf> {
    let current = std::env::current_exe()?;
    let sibling = current.with_file_name("linuxcare-maintenance");
    if sibling.exists() {
        Ok(sibling)
    } else if Path::new("/usr/bin/linuxcare-maintenance").exists() {
        Ok(PathBuf::from("/usr/bin/linuxcare-maintenance"))
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "linuxcare-maintenance binary is not installed next to LinuxCare",
        ))
    }
}

fn systemd_escape_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    let escaped = text
        .replace('%', "%%")
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r', '\t'], "");
    format!("\"{escaped}\"")
}

fn automation_path_allowed(candidate: &CleanupCandidate, home: &Path) -> bool {
    let Some(path) = candidate.path.as_deref() else {
        return false;
    };
    let cache = home.join(".cache");
    let trash = home.join(".local/share/Trash/files");
    let npm = home.join(".npm/_cacache");
    let cargo = home.join(".cargo/registry/cache");
    (path.starts_with(&cache) && path != cache.as_path())
        || path == trash.as_path()
        || path == npm.as_path()
        || path == cargo.as_path()
}

fn run_systemctl_user<const N: usize>(args: [&str; N]) -> io::Result<()> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "systemctl --user failed with {status}"
        )))
    }
}

fn timer_properties() -> (Option<String>, Option<String>) {
    let output = Command::new("systemctl")
        .args([
            "--user",
            "show",
            "linuxcare-maintenance.timer",
            "--property=NextElapseUSecRealtime",
            "--property=LastTriggerUSec",
            "--no-pager",
        ])
        .stdin(Stdio::null())
        .output();
    let Ok(output) = output else {
        return (None, None);
    };
    if !output.status.success() {
        return (None, None);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut next = None;
    let mut last = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("NextElapseUSecRealtime=") {
            let value = value.trim();
            if !value.is_empty() && value != "n/a" {
                next = Some(value.to_string());
            }
        } else if let Some(value) = line.strip_prefix("LastTriggerUSec=") {
            let value = value.trim();
            if !value.is_empty() && value != "n/a" {
                last = Some(value.to_string());
            }
        }
    }
    (next, last)
}

fn command_success(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn config_path(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("linuxcare/automation.json")
}

fn user_systemd_dir(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("systemd/user")
}

fn save_json_private<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("config path has no parent"))?;
    fs::create_dir_all(parent)?;
    set_private_dir(parent)?;
    let payload = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    write_private(path, &payload)
}

fn write_private(path: &Path, payload: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(payload)?;
    file.sync_all()?;
    fs::rename(tmp, path)?;
    Ok(())
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

#[cfg(test)]
mod beta_migration_tests {
    use super::*;

    #[test]
    fn older_partial_automation_config_uses_defaults() {
        let old = r#"{"enabled": true}"#;
        let parsed: AutomationConfig = serde_json::from_str(old).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.cadence, AutomationCadence::Weekly);
        assert_eq!(parsed.weekday, "Sun");
        assert_eq!(parsed.hour, 10);
        assert_eq!(parsed.minute, 0);
        assert_eq!(parsed.mode, AutomationMode::ScanAndNotify);
    }
}
