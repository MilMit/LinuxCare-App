use crate::{format, system_info};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const SAMPLE_INTERVAL_SECS: u64 = 6 * 60 * 60;
const MAX_SAMPLES: usize = 180;
const ANALYSIS_WINDOW_SECS: u64 = 30 * 24 * 60 * 60;
const MIN_SPAN_SECS: u64 = 12 * 60 * 60;
const MEANINGFUL_GROWTH_BYTES_PER_DAY: f64 = 16.0 * 1024.0 * 1024.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageSample {
    timestamp_unix: u64,
    root_used_bytes: u64,
    root_total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct RunwayTarget {
    pub percent: u8,
    pub days: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct StorageRunwayReport {
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub used_percent: f64,
    pub growth_bytes_per_day: Option<f64>,
    pub sample_count: usize,
    pub span_days: f64,
    pub status_label: String,
    pub summary: String,
    pub targets: Vec<RunwayTarget>,
}

pub fn collect_report(home: &Path) -> StorageRunwayReport {
    let stats = system_info::collect_system_stats();
    let current = StorageSample {
        timestamp_unix: now_unix(),
        root_used_bytes: stats.disk_root_used_bytes,
        root_total_bytes: stats.disk_root_total_bytes,
    };

    let path = storage_history_path(home);
    let mut history = load_history(&path).unwrap_or_default();
    maybe_record(&path, &mut history, current.clone());

    let window_start = current.timestamp_unix.saturating_sub(ANALYSIS_WINDOW_SECS);
    let recent: Vec<&StorageSample> = history
        .iter()
        .filter(|sample| sample.timestamp_unix >= window_start)
        .collect();

    let span_secs = recent
        .first()
        .and_then(|first| {
            recent
                .last()
                .map(|last| last.timestamp_unix.saturating_sub(first.timestamp_unix))
        })
        .unwrap_or(0);
    let span_days = span_secs as f64 / 86_400.0;

    let growth_bytes_per_day = if recent.len() >= 3 && span_secs >= MIN_SPAN_SECS {
        linear_growth_bytes_per_day(&recent)
    } else {
        None
    };

    let used_percent = ratio(current.root_used_bytes, current.root_total_bytes) * 100.0;
    let targets = [80_u8, 90, 95]
        .into_iter()
        .map(|percent| RunwayTarget {
            percent,
            days: estimate_days_to_percent(
                current.root_used_bytes,
                current.root_total_bytes,
                percent,
                growth_bytes_per_day,
            ),
        })
        .collect::<Vec<_>>();

    let (status_label, summary) = status_and_summary(
        used_percent,
        growth_bytes_per_day,
        recent.len(),
        span_days,
        &targets,
    );

    StorageRunwayReport {
        used_bytes: current.root_used_bytes,
        total_bytes: current.root_total_bytes,
        used_percent,
        growth_bytes_per_day,
        sample_count: recent.len(),
        span_days,
        status_label,
        summary,
        targets,
    }
}

pub fn format_growth(bytes_per_day: Option<f64>) -> String {
    match bytes_per_day {
        Some(value) if value > MEANINGFUL_GROWTH_BYTES_PER_DAY => {
            format!("+{} / day", format::bytes(value as u64))
        }
        Some(value) if value < -MEANINGFUL_GROWTH_BYTES_PER_DAY => {
            format!("−{} / day", format::bytes(value.abs() as u64))
        }
        Some(_) => "Stable".to_string(),
        None => "Learning baseline".to_string(),
    }
}

pub fn format_days(days: Option<f64>) -> String {
    match days {
        Some(value) if value < 1.0 => "< 1 day".to_string(),
        Some(value) if value < 14.0 => format!("~{:.0} days", value.ceil()),
        Some(value) if value < 120.0 => format!("~{:.0} weeks", (value / 7.0).ceil()),
        Some(value) if value < 730.0 => format!("~{:.0} months", (value / 30.4).ceil()),
        Some(_) => "> 2 years".to_string(),
        None => "No forecast".to_string(),
    }
}

fn status_and_summary(
    used_percent: f64,
    growth_bytes_per_day: Option<f64>,
    sample_count: usize,
    span_days: f64,
    targets: &[RunwayTarget],
) -> (String, String) {
    if used_percent >= 95.0 {
        return (
            "Critical".to_string(),
            format!("Root storage is already at {used_percent:.0}%. Reclaim space before normal updates or applications run out of room."),
        );
    }
    if used_percent >= 90.0 {
        return (
            "High pressure".to_string(),
            format!("Root storage is at {used_percent:.0}%. LinuxCare recommends cleanup or capacity review now."),
        );
    }

    let days_to_90 = targets
        .iter()
        .find(|target| target.percent == 90)
        .and_then(|target| target.days);
    if let Some(days) = days_to_90 {
        if days <= 14.0 {
            return (
                "Pressure soon".to_string(),
                format!(
                    "At the observed trend, root storage may reach 90% in {}.",
                    format_days(Some(days))
                ),
            );
        }
        if days <= 60.0 {
            return (
                "Growing".to_string(),
                format!(
                    "Storage is growing fast enough to reach 90% in {} if the trend continues.",
                    format_days(Some(days))
                ),
            );
        }
    }

    match growth_bytes_per_day {
        Some(growth) if growth > MEANINGFUL_GROWTH_BYTES_PER_DAY => (
            "Growing".to_string(),
            format!("Root storage is increasing by roughly {}. Forecasts are trend estimates, not guarantees.", format_growth(Some(growth))),
        ),
        Some(_) => (
            "Stable".to_string(),
            "Recent root-disk usage is broadly stable. No near-term capacity pressure is projected from the observed samples.".to_string(),
        ),
        None => (
            "Learning".to_string(),
            format!("LinuxCare has {sample_count} usable storage sample(s) across {span_days:.1} days. It needs at least three samples spanning 12 hours before forecasting."),
        ),
    }
}

fn estimate_days_to_percent(
    used_bytes: u64,
    total_bytes: u64,
    target_percent: u8,
    growth_bytes_per_day: Option<f64>,
) -> Option<f64> {
    let growth = growth_bytes_per_day?;
    if growth <= MEANINGFUL_GROWTH_BYTES_PER_DAY || total_bytes == 0 {
        return None;
    }
    let target = total_bytes as f64 * target_percent as f64 / 100.0;
    let remaining = target - used_bytes as f64;
    if remaining <= 0.0 {
        return Some(0.0);
    }
    Some(remaining / growth)
}

fn linear_growth_bytes_per_day(samples: &[&StorageSample]) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let origin = samples.first()?.timestamp_unix as f64;
    let n = samples.len() as f64;
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xy = 0.0;
    let mut sum_x2 = 0.0;

    for sample in samples {
        let x_days = (sample.timestamp_unix as f64 - origin) / 86_400.0;
        let y = sample.root_used_bytes as f64;
        sum_x += x_days;
        sum_y += y;
        sum_xy += x_days * y;
        sum_x2 += x_days * x_days;
    }

    let denominator = n * sum_x2 - sum_x * sum_x;
    if denominator.abs() < f64::EPSILON {
        return None;
    }
    Some((n * sum_xy - sum_x * sum_y) / denominator)
}

fn maybe_record(path: &Path, history: &mut Vec<StorageSample>, current: StorageSample) {
    let should_record = history
        .last()
        .map(|last| {
            current.timestamp_unix.saturating_sub(last.timestamp_unix) >= SAMPLE_INTERVAL_SECS
        })
        .unwrap_or(true);
    if !should_record {
        return;
    }

    history.push(current);
    if history.len() > MAX_SAMPLES {
        let drain = history.len() - MAX_SAMPLES;
        history.drain(0..drain);
    }
    let _ = save_history(path, history);
}

fn ratio(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn storage_history_path(home: &Path) -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"))
        .join("linuxcare")
        .join("storage-runway.json")
}

fn load_history(path: &Path) -> io::Result<Vec<StorageSample>> {
    let data = fs::read(path)?;
    serde_json::from_slice(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn save_history(path: &Path, history: &[StorageSample]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "state path has no parent"))?;
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
    let tmp = dir.join(format!(
        ".storage-runway-{}-{nonce}.tmp",
        std::process::id()
    ));
    let payload = serde_json::to_vec_pretty(history)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut file = OpenOptions::new().create_new(true).write(true).open(&tmp)?;
    file.write_all(&payload)?;
    file.sync_all()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    }

    fs::rename(tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_detects_growth_per_day() {
        let gib = 1024_u64 * 1024 * 1024;
        let samples = [
            StorageSample {
                timestamp_unix: 0,
                root_used_bytes: 10 * gib,
                root_total_bytes: 100 * gib,
            },
            StorageSample {
                timestamp_unix: 86_400,
                root_used_bytes: 11 * gib,
                root_total_bytes: 100 * gib,
            },
            StorageSample {
                timestamp_unix: 172_800,
                root_used_bytes: 12 * gib,
                root_total_bytes: 100 * gib,
            },
        ];
        let refs = samples.iter().collect::<Vec<_>>();
        let growth = linear_growth_bytes_per_day(&refs).unwrap();
        assert!((growth - gib as f64).abs() < 1.0);
    }

    #[test]
    fn forecast_refuses_flat_growth() {
        assert_eq!(estimate_days_to_percent(50, 100, 90, Some(0.0)), None);
    }
}
