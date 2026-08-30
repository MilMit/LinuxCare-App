use std::{fs, path::Path, process::Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalState {
    Good,
    Review,
    Critical,
    Unknown,
}

impl ThermalState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Good => "GOOD",
            Self::Review => "REVIEW",
            Self::Critical => "CRITICAL",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::Good => "risk-safe",
            Self::Review => "risk-review",
            Self::Critical => "risk-dangerous",
            Self::Unknown => "health-neutral",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThermalSensor {
    pub name: String,
    pub temperature_c: f64,
    pub max_c: Option<f64>,
    pub critical_c: Option<f64>,
    pub state: ThermalState,
}

#[derive(Debug, Clone, Default)]
pub struct CpuPowerState {
    pub governor: Option<String>,
    pub current_mhz: Option<f64>,
    pub max_mhz: Option<f64>,
    pub power_profile: Option<String>,
    pub throttle_events: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ThermalDoctorReport {
    pub sensors: Vec<ThermalSensor>,
    pub cpu: CpuPowerState,
    pub max_temperature_c: Option<f64>,
    pub status_label: String,
    pub summary: String,
}

pub fn collect_report() -> ThermalDoctorReport {
    let sensors = collect_hwmon_sensors();
    let cpu = collect_cpu_power_state();
    let max_temperature_c = sensors
        .iter()
        .map(|sensor| sensor.temperature_c)
        .reduce(f64::max);

    let critical = sensors
        .iter()
        .any(|sensor| sensor.state == ThermalState::Critical);
    let review = sensors
        .iter()
        .any(|sensor| sensor.state == ThermalState::Review);
    let throttled = cpu.throttle_events.unwrap_or(0) > 0;

    let (status_label, summary) = if critical {
        (
            "Thermal pressure".to_string(),
            "At least one temperature sensor is at a critical or near-critical threshold. Sustained load should be reduced while cooling is reviewed.".to_string(),
        )
    } else if review || throttled {
        (
            "Review".to_string(),
            if throttled {
                "Temperature is not currently critical, but the kernel exposes one or more CPU throttle events since boot. This is evidence of past thermal/power limiting, not necessarily a current fault.".to_string()
            } else {
                "One or more sensors are warm enough to justify monitoring under sustained load."
                    .to_string()
            },
        )
    } else if sensors.is_empty() {
        (
            "Unknown".to_string(),
            "No usable hwmon temperature sensor was exposed to LinuxCare. This can be normal in some VMs or hardware/driver combinations.".to_string(),
        )
    } else {
        (
            "Healthy".to_string(),
            "Available temperature sensors are below LinuxCare's review thresholds and no thermal warning is currently inferred.".to_string(),
        )
    };

    ThermalDoctorReport {
        sensors,
        cpu,
        max_temperature_c,
        status_label,
        summary,
    }
}

fn collect_hwmon_sensors() -> Vec<ThermalSensor> {
    let root = Path::new("/sys/class/hwmon");
    let Ok(hwmons) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut sensors = Vec::new();
    for hwmon in hwmons.flatten() {
        let path = hwmon.path();
        let chip = read_text(&path.join("name")).unwrap_or_else(|| "hwmon".to_string());
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let Some(index) = name
                .strip_prefix("temp")
                .and_then(|rest| rest.strip_suffix("_input"))
            else {
                continue;
            };
            let Some(raw) = read_f64(&entry.path()) else {
                continue;
            };
            let temperature_c = raw / 1000.0;
            if !temperature_c.is_finite() || !(-40.0..=200.0).contains(&temperature_c) {
                continue;
            }
            let label = read_text(&path.join(format!("temp{index}_label")))
                .unwrap_or_else(|| format!("temp{index}"));
            let max_c = read_f64(&path.join(format!("temp{index}_max"))).map(|v| v / 1000.0);
            let critical_c = read_f64(&path.join(format!("temp{index}_crit"))).map(|v| v / 1000.0);
            let state = classify_temperature(temperature_c, max_c, critical_c);
            sensors.push(ThermalSensor {
                name: format!("{chip} • {label}"),
                temperature_c,
                max_c,
                critical_c,
                state,
            });
        }
    }
    sensors.sort_by(|a, b| {
        b.temperature_c
            .partial_cmp(&a.temperature_c)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sensors.truncate(24);
    sensors
}

fn classify_temperature(
    temperature_c: f64,
    max_c: Option<f64>,
    critical_c: Option<f64>,
) -> ThermalState {
    if critical_c.is_some_and(|crit| temperature_c >= crit - 3.0) || temperature_c >= 95.0 {
        ThermalState::Critical
    } else if max_c.is_some_and(|max| temperature_c >= max) || temperature_c >= 82.0 {
        ThermalState::Review
    } else {
        ThermalState::Good
    }
}

fn collect_cpu_power_state() -> CpuPowerState {
    let cpu0 = Path::new("/sys/devices/system/cpu/cpu0/cpufreq");
    let governor = read_text(&cpu0.join("scaling_governor"));
    let current_mhz = read_f64(&cpu0.join("scaling_cur_freq")).map(|value| value / 1000.0);
    let max_mhz = read_f64(&cpu0.join("cpuinfo_max_freq")).map(|value| value / 1000.0);
    let power_profile = Command::new("powerprofilesctl")
        .arg("get")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            (!value.is_empty()).then_some(value)
        });
    let throttle_events = collect_throttle_events();
    CpuPowerState {
        governor,
        current_mhz,
        max_mhz,
        power_profile,
        throttle_events,
    }
}

fn collect_throttle_events() -> Option<u64> {
    let root = Path::new("/sys/devices/system/cpu");
    let Ok(cpus) = fs::read_dir(root) else {
        return None;
    };
    let mut found = false;
    let mut total = 0u64;
    for cpu in cpus.flatten() {
        let name = cpu.file_name().to_string_lossy().to_string();
        let Some(index) = name.strip_prefix("cpu") else {
            continue;
        };
        if index.is_empty() || !index.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let throttle = cpu.path().join("thermal_throttle");
        for file in ["core_throttle_count", "package_throttle_count"] {
            if let Some(value) = read_u64(&throttle.join(file)) {
                found = true;
                total = total.saturating_add(value);
            }
        }
    }
    found.then_some(total)
}

fn read_text(path: &Path) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn read_f64(path: &Path) -> Option<f64> {
    read_text(path)?.parse().ok()
}

fn read_u64(path: &Path) -> Option<u64> {
    read_text(path)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_high_temperature() {
        assert_eq!(
            classify_temperature(96.0, Some(90.0), Some(100.0)),
            ThermalState::Critical
        );
        assert_eq!(
            classify_temperature(84.0, Some(90.0), Some(100.0)),
            ThermalState::Review
        );
        assert_eq!(
            classify_temperature(55.0, Some(90.0), Some(100.0)),
            ThermalState::Good
        );
    }
}
