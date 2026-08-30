use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const HISTORY_INTERVAL_SECS: u64 = 15 * 60;
const HISTORY_WINDOW_SECS: u64 = 14 * 24 * 60 * 60;
const MAX_HISTORY_SAMPLES: usize = 1600;

#[derive(Debug, Clone, Default)]
pub struct BatteryDevice {
    pub name: String,
    pub scope: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub technology: Option<String>,
    pub status: String,
    pub capacity_percent: Option<u8>,
    pub health_percent: Option<f64>,
    pub cycle_count: Option<u64>,
    pub energy_now_wh: Option<f64>,
    pub energy_full_wh: Option<f64>,
    pub energy_design_wh: Option<f64>,
    pub voltage_v: Option<f64>,
    pub current_a: Option<f64>,
    pub power_w: Option<f64>,
    pub time_remaining_minutes: Option<u64>,
    pub charge_start_threshold: Option<u8>,
    pub charge_end_threshold: Option<u8>,
}

impl BatteryDevice {
    pub fn display_name(&self) -> String {
        match (&self.manufacturer, &self.model) {
            (Some(manufacturer), Some(model)) => format!("{manufacturer} {model}"),
            (None, Some(model)) => model.clone(),
            (Some(manufacturer), None) => manufacturer.clone(),
            (None, None) => self.name.clone(),
        }
    }

    pub fn health_label(&self) -> &'static str {
        match self.health_percent {
            Some(value) if value >= 90.0 => "Excellent",
            Some(value) if value >= 80.0 => "Good",
            Some(value) if value >= 70.0 => "Worn",
            Some(_) => "High wear",
            None => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BatteryLabReport {
    pub batteries: Vec<BatteryDevice>,
    pub external_power_online: Option<bool>,
    pub trends: Vec<BatteryTrend>,
}

#[derive(Debug, Clone)]
pub struct BatteryTrend {
    pub name: String,
    pub sample_count: usize,
    pub span_hours: f64,
    pub health_delta_percent: Option<f64>,
    pub average_discharge_power_w: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BatteryHistorySample {
    timestamp_unix: u64,
    name: String,
    capacity_percent: Option<u8>,
    health_percent: Option<f64>,
    power_w: Option<f64>,
    status: String,
}

pub fn collect_report() -> BatteryLabReport {
    let root = Path::new("/sys/class/power_supply");
    let mut report = BatteryLabReport::default();
    let Ok(entries) = fs::read_dir(root) else {
        return report;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let supply_type = read_text(&path.join("type")).unwrap_or_default();
        if supply_type.eq_ignore_ascii_case("Battery") {
            report.batteries.push(read_battery_device(&path));
        } else if matches!(supply_type.as_str(), "Mains" | "USB" | "USB_C" | "USB_PD") {
            if let Some(online) = read_u64(&path.join("online")) {
                let online = online != 0;
                report.external_power_online = Some(
                    report
                        .external_power_online
                        .map(|current| current || online)
                        .unwrap_or(online),
                );
            }
        }
    }

    report.batteries.sort_by_key(|battery| {
        (
            battery.scope.as_deref() != Some("System"),
            battery.name.clone(),
        )
    });
    report
}

pub fn collect_report_with_history(home: &Path) -> BatteryLabReport {
    let mut report = collect_report();
    let path = history_path(home);
    let mut history = load_history(&path).unwrap_or_default();
    maybe_record_history(&path, &mut history, &report.batteries);
    report.trends = build_trends(&history, now_unix());
    report
}

fn maybe_record_history(
    path: &Path,
    history: &mut Vec<BatteryHistorySample>,
    batteries: &[BatteryDevice],
) {
    if batteries.is_empty() {
        return;
    }
    let now = now_unix();
    let should_record = history
        .last()
        .map(|sample| now.saturating_sub(sample.timestamp_unix) >= HISTORY_INTERVAL_SECS)
        .unwrap_or(true);
    if !should_record {
        return;
    }
    for battery in batteries {
        history.push(BatteryHistorySample {
            timestamp_unix: now,
            name: battery.name.clone(),
            capacity_percent: battery.capacity_percent,
            health_percent: battery.health_percent,
            power_w: battery.power_w,
            status: battery.status.clone(),
        });
    }
    let cutoff = now.saturating_sub(HISTORY_WINDOW_SECS);
    history.retain(|sample| sample.timestamp_unix >= cutoff);
    if history.len() > MAX_HISTORY_SAMPLES {
        let drain = history.len() - MAX_HISTORY_SAMPLES;
        history.drain(0..drain);
    }
    let _ = save_history(path, history);
}

fn build_trends(history: &[BatteryHistorySample], now: u64) -> Vec<BatteryTrend> {
    let mut names = history
        .iter()
        .map(|sample| sample.name.clone())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();

    names
        .into_iter()
        .filter_map(|name| {
            let samples = history
                .iter()
                .filter(|sample| sample.name == name)
                .collect::<Vec<_>>();
            let first = samples.first()?;
            let last = samples.last()?;
            let span_hours =
                last.timestamp_unix.saturating_sub(first.timestamp_unix) as f64 / 3600.0;
            let health_delta_percent = if span_hours >= 24.0 {
                match (first.health_percent, last.health_percent) {
                    (Some(before), Some(after)) => Some(after - before),
                    _ => None,
                }
            } else {
                None
            };
            let six_hours_ago = now.saturating_sub(6 * 60 * 60);
            let discharge_power = samples
                .iter()
                .filter(|sample| {
                    sample.timestamp_unix >= six_hours_ago
                        && sample.status.eq_ignore_ascii_case("Discharging")
                })
                .filter_map(|sample| sample.power_w)
                .collect::<Vec<_>>();
            let average_discharge_power_w = if discharge_power.is_empty() {
                None
            } else {
                Some(discharge_power.iter().sum::<f64>() / discharge_power.len() as f64)
            };
            Some(BatteryTrend {
                name,
                sample_count: samples.len(),
                span_hours,
                health_delta_percent,
                average_discharge_power_w,
            })
        })
        .collect()
}

fn history_path(home: &Path) -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"))
        .join("linuxcare")
        .join("battery-history.json")
}

fn load_history(path: &Path) -> io::Result<Vec<BatteryHistorySample>> {
    let data = fs::read(path)?;
    serde_json::from_slice(&data).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn save_history(path: &Path, history: &[BatteryHistorySample]) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "battery state path has no parent",
        )
    })?;
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
        ".battery-history-{}-{nonce}.tmp",
        std::process::id()
    ));
    let payload = serde_json::to_vec_pretty(history)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut file = OpenOptions::new().create_new(true).write(true).open(&tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&payload)?;
    file.sync_all()?;
    fs::rename(&tmp, path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn read_battery_device(path: &Path) -> BatteryDevice {
    let raw_energy_now = read_f64(&path.join("energy_now"));
    let raw_energy_full = read_f64(&path.join("energy_full"));
    let raw_energy_design = read_f64(&path.join("energy_full_design"));
    let raw_charge_now = read_f64(&path.join("charge_now"));
    let raw_charge_full = read_f64(&path.join("charge_full"));
    let raw_charge_design = read_f64(&path.join("charge_full_design"));

    let voltage_now_v = read_f64(&path.join("voltage_now")).map(|value| value / 1_000_000.0);
    let voltage_design_v =
        read_f64(&path.join("voltage_min_design")).map(|value| value / 1_000_000.0);
    let voltage_v = voltage_now_v.or(voltage_design_v);
    let current_a = read_f64(&path.join("current_now")).map(|value| value / 1_000_000.0);
    let power_w = read_f64(&path.join("power_now"))
        .map(|value| value / 1_000_000.0)
        .or_else(|| match (voltage_now_v, current_a) {
            (Some(voltage), Some(current)) => Some((voltage * current).abs()),
            _ => None,
        });

    let energy_now_wh = raw_energy_now
        .map(|value| value / 1_000_000.0)
        .or_else(|| charge_to_wh(raw_charge_now, voltage_now_v.or(voltage_design_v)));
    let energy_full_wh = raw_energy_full
        .map(|value| value / 1_000_000.0)
        .or_else(|| charge_to_wh(raw_charge_full, voltage_design_v.or(voltage_now_v)));
    let energy_design_wh = raw_energy_design
        .map(|value| value / 1_000_000.0)
        .or_else(|| charge_to_wh(raw_charge_design, voltage_design_v.or(voltage_now_v)));

    let health_percent = match (raw_energy_full, raw_energy_design) {
        (Some(full), Some(design)) => ratio_percent(Some(full), Some(design)),
        _ => match (raw_charge_full, raw_charge_design) {
            (Some(full), Some(design)) => ratio_percent(Some(full), Some(design)),
            _ => None,
        },
    };

    let capacity_percent = read_u64(&path.join("capacity"))
        .map(|value| value.min(100) as u8)
        .or_else(|| {
            match (raw_energy_now, raw_energy_full) {
                (Some(now), Some(full)) => ratio_percent(Some(now), Some(full)),
                _ => match (raw_charge_now, raw_charge_full) {
                    (Some(now), Some(full)) => ratio_percent(Some(now), Some(full)),
                    _ => None,
                },
            }
            .map(|value| value.clamp(0.0, 100.0).round() as u8)
        });

    let status = read_text(&path.join("status")).unwrap_or_else(|| "Unknown".to_string());
    let time_remaining_minutes = estimate_minutes(
        &status,
        energy_now_wh,
        energy_full_wh,
        power_w,
        raw_charge_now,
        raw_charge_full,
        read_f64(&path.join("current_now")),
    );

    BatteryDevice {
        name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Battery")
            .to_string(),
        scope: read_text(&path.join("scope")),
        manufacturer: read_text(&path.join("manufacturer")),
        model: read_text(&path.join("model_name")),
        technology: read_text(&path.join("technology")),
        status,
        capacity_percent,
        health_percent,
        cycle_count: read_u64(&path.join("cycle_count")),
        energy_now_wh,
        energy_full_wh,
        energy_design_wh,
        voltage_v,
        current_a,
        power_w,
        time_remaining_minutes,
        charge_start_threshold: read_u64(&path.join("charge_control_start_threshold"))
            .map(|value| value.min(100) as u8),
        charge_end_threshold: read_u64(&path.join("charge_control_end_threshold"))
            .map(|value| value.min(100) as u8),
    }
}

fn charge_to_wh(charge_micro_ah: Option<f64>, voltage_v: Option<f64>) -> Option<f64> {
    Some(charge_micro_ah? / 1_000_000.0 * voltage_v?)
}

fn ratio_percent(current: Option<f64>, reference: Option<f64>) -> Option<f64> {
    let current = current?;
    let reference = reference?;
    if reference <= 0.0 {
        return None;
    }
    Some((current / reference * 100.0).clamp(0.0, 150.0))
}

fn estimate_minutes(
    status: &str,
    energy_now_wh: Option<f64>,
    energy_full_wh: Option<f64>,
    power_w: Option<f64>,
    charge_now: Option<f64>,
    charge_full: Option<f64>,
    current_micro_a: Option<f64>,
) -> Option<u64> {
    let hours = if let Some(power_w) = power_w.filter(|value| *value > 0.2) {
        if status.eq_ignore_ascii_case("Discharging") {
            energy_now_wh.map(|energy| energy / power_w)
        } else if status.eq_ignore_ascii_case("Charging") {
            match (energy_now_wh, energy_full_wh) {
                (Some(now), Some(full)) => Some((full - now).max(0.0) / power_w),
                _ => None,
            }
        } else {
            None
        }
    } else if let Some(current) = current_micro_a.filter(|value| value.abs() > 1.0) {
        let current = current.abs();
        if status.eq_ignore_ascii_case("Discharging") {
            charge_now.map(|charge| charge / current)
        } else if status.eq_ignore_ascii_case("Charging") {
            match (charge_now, charge_full) {
                (Some(now), Some(full)) => Some((full - now).max(0.0) / current),
                _ => None,
            }
        } else {
            None
        }
    } else {
        None
    }?;

    if !hours.is_finite() || !(0.0..=240.0).contains(&hours) {
        return None;
    }
    Some((hours * 60.0).round() as u64)
}

fn read_text(path: &Path) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn read_f64(path: &Path) -> Option<f64> {
    read_text(path)?.parse::<f64>().ok()
}

fn read_u64(path: &Path) -> Option<u64> {
    read_text(path)?.parse::<u64>().ok()
}

pub fn format_minutes(minutes: Option<u64>, status: &str) -> String {
    let Some(minutes) = minutes else {
        return "Unavailable".to_string();
    };
    let hours = minutes / 60;
    let mins = minutes % 60;
    let suffix = if status.eq_ignore_ascii_case("Charging") {
        "until full"
    } else {
        "remaining"
    };
    if hours > 0 {
        format!("{hours}h {mins}m {suffix}")
    } else {
        format!("{mins}m {suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_charge_to_energy() {
        let wh = charge_to_wh(Some(5_000_000.0), Some(12.0)).unwrap();
        assert!((wh - 60.0).abs() < 0.001);
    }

    #[test]
    fn health_ratio_is_bounded() {
        assert_eq!(ratio_percent(Some(45.0), Some(60.0)), Some(75.0));
        assert_eq!(ratio_percent(Some(0.0), Some(0.0)), None);
    }

    #[test]
    fn discharge_time_uses_energy_and_power() {
        let minutes = estimate_minutes(
            "Discharging",
            Some(30.0),
            Some(50.0),
            Some(15.0),
            None,
            None,
            None,
        );
        assert_eq!(minutes, Some(120));
    }
}
