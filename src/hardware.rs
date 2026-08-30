use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default)]
pub struct HardwareReport {
    pub system: SystemIdentity,
    pub cpu: CpuIdentity,
    pub memory: MemoryIdentity,
    pub gpus: Vec<GpuIdentity>,
    pub storage: Vec<StorageDevice>,
    pub sensors: Vec<TemperatureSensor>,
}

#[derive(Debug, Clone, Default)]
pub struct SystemIdentity {
    pub vendor: String,
    pub product: String,
    pub board: String,
    pub bios_version: String,
}

#[derive(Debug, Clone, Default)]
pub struct CpuIdentity {
    pub model: String,
    pub logical_cpus: usize,
    pub physical_packages: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryIdentity {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct GpuIdentity {
    pub card: String,
    pub driver: String,
    pub pci_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct StorageDevice {
    pub name: String,
    pub model: String,
    pub vendor: String,
    pub size_bytes: u64,
    pub rotational: Option<bool>,
    pub removable: bool,
    pub transport: String,
}

#[derive(Debug, Clone, Default)]
pub struct TemperatureSensor {
    pub source: String,
    pub label: String,
    pub celsius: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageHealthReading {
    pub device: String,
    pub protocol: String,
    pub smart_supported: Option<bool>,
    pub smart_enabled: Option<bool>,
    pub passed: Option<bool>,
    pub temperature_c: Option<f64>,
    pub power_on_hours: Option<u64>,
    pub percentage_used: Option<u64>,
    pub critical_warning: Option<u64>,
    pub media_errors: Option<u64>,
    pub unsafe_shutdowns: Option<u64>,
    pub data_units_read: Option<u64>,
    pub data_units_written: Option<u64>,
    pub reallocated_sectors: Option<u64>,
    pub pending_sectors: Option<u64>,
    pub offline_uncorrectable: Option<u64>,
    pub error_log_count: Option<u64>,
    pub source: String,
    pub note: Option<String>,
}

pub fn collect_report() -> HardwareReport {
    HardwareReport {
        system: collect_system_identity(),
        cpu: collect_cpu_identity(),
        memory: collect_memory_identity(),
        gpus: collect_gpus(),
        storage: collect_storage_devices(),
        sensors: collect_temperature_sensors(),
    }
}

pub fn collect_storage_devices() -> Vec<StorageDevice> {
    let mut devices = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return devices;
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if is_virtual_block_name(&name) {
            continue;
        }

        let root = entry.path();
        if !root.join("device").exists() && !name.starts_with("nvme") && !name.starts_with("mmcblk")
        {
            continue;
        }

        let sectors = read_u64(root.join("size")).unwrap_or(0);
        let rotational = read_u64(root.join("queue/rotational")).map(|value| value != 0);
        let removable = read_u64(root.join("removable")).unwrap_or(0) != 0;
        let model = read_trimmed(root.join("device/model")).unwrap_or_else(|| name.clone());
        let vendor = read_trimmed(root.join("device/vendor")).unwrap_or_default();
        let transport = infer_transport(&root, &name);

        devices.push(StorageDevice {
            name,
            model,
            vendor,
            size_bytes: sectors.saturating_mul(512),
            rotational,
            removable,
            transport,
        });
    }

    devices.sort_by(|a, b| a.name.cmp(&b.name));
    devices
}

pub fn smartctl_command_error(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    let exit_status = value
        .pointer("/smartctl/exit_status")
        .and_then(number_as_u64)
        .unwrap_or(0);
    if exit_status & 0x07 == 0 {
        return None;
    }

    let message = value
        .pointer("/smartctl/messages")
        .and_then(Value::as_array)
        .and_then(|messages| {
            messages.iter().find_map(|item| {
                item.get("string")
                    .and_then(Value::as_str)
                    .filter(|text| !text.trim().is_empty())
                    .map(str::to_string)
            })
        })
        .unwrap_or_else(|| {
            format!(
                "smartctl command failed with status bits 0x{:02x}",
                exit_status & 0x07
            )
        });
    Some(message)
}

pub fn parse_smartctl_json(device: &str, text: &str) -> Result<StorageHealthReading, String> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| format!("smartctl returned invalid JSON: {error}"))?;

    let protocol = value
        .pointer("/device/protocol")
        .and_then(Value::as_str)
        .unwrap_or("Unknown")
        .to_string();

    let smart_supported = value
        .pointer("/smart_support/available")
        .and_then(Value::as_bool);
    let smart_enabled = value
        .pointer("/smart_support/enabled")
        .and_then(Value::as_bool);
    let passed = value
        .pointer("/smart_status/passed")
        .and_then(Value::as_bool);
    let temperature_c = value
        .pointer("/temperature/current")
        .and_then(number_as_f64)
        .or_else(|| {
            value
                .pointer("/nvme_smart_health_information_log/temperature")
                .and_then(number_as_f64)
        });
    let power_on_hours = value
        .pointer("/power_on_time/hours")
        .and_then(number_as_u64)
        .or_else(|| {
            value
                .pointer("/nvme_smart_health_information_log/power_on_hours")
                .and_then(number_as_u64)
        });

    let nvme = value.pointer("/nvme_smart_health_information_log");
    let percentage_used = nvme
        .and_then(|item| item.get("percentage_used"))
        .and_then(number_as_u64);
    let critical_warning = nvme
        .and_then(|item| item.get("critical_warning"))
        .and_then(number_as_u64);
    let media_errors = nvme
        .and_then(|item| item.get("media_errors"))
        .and_then(number_as_u64);
    let unsafe_shutdowns = nvme
        .and_then(|item| item.get("unsafe_shutdowns"))
        .and_then(number_as_u64);
    let data_units_read = nvme
        .and_then(|item| item.get("data_units_read"))
        .and_then(number_as_u64);
    let data_units_written = nvme
        .and_then(|item| item.get("data_units_written"))
        .and_then(number_as_u64);

    let attributes = value
        .pointer("/ata_smart_attributes/table")
        .and_then(Value::as_array)
        .map(Vec::as_slice);
    let reallocated_sectors = ata_raw_attribute(attributes, 5);
    let pending_sectors = ata_raw_attribute(attributes, 197);
    let offline_uncorrectable = ata_raw_attribute(attributes, 198);
    let error_log_count = value
        .pointer("/ata_smart_error_log/summary/count")
        .and_then(number_as_u64);

    let standby_note = value
        .pointer("/smartctl/messages")
        .and_then(Value::as_array)
        .and_then(|messages| {
            messages.iter().find_map(|item| {
                let text = item.get("string").and_then(Value::as_str)?;
                let lower = text.to_ascii_lowercase();
                if lower.contains("standby") || lower.contains("sleep") {
                    Some("Drive is in a low-power state; LinuxCare did not wake it for this health check.".to_string())
                } else {
                    None
                }
            })
        });

    Ok(StorageHealthReading {
        device: device.to_string(),
        protocol,
        smart_supported,
        smart_enabled,
        passed,
        temperature_c,
        power_on_hours,
        percentage_used,
        critical_warning,
        media_errors,
        unsafe_shutdowns,
        data_units_read,
        data_units_written,
        reallocated_sectors,
        pending_sectors,
        offline_uncorrectable,
        error_log_count,
        source: "smartctl JSON".to_string(),
        note: standby_note,
    })
}

fn collect_system_identity() -> SystemIdentity {
    let vendor = read_trimmed("/sys/class/dmi/id/sys_vendor").unwrap_or_else(|| "Unknown".into());
    let product =
        read_trimmed("/sys/class/dmi/id/product_name").unwrap_or_else(|| "Unknown".into());
    let board_vendor = read_trimmed("/sys/class/dmi/id/board_vendor").unwrap_or_default();
    let board_name = read_trimmed("/sys/class/dmi/id/board_name").unwrap_or_default();
    let board = match (board_vendor.is_empty(), board_name.is_empty()) {
        (false, false) => format!("{board_vendor} {board_name}"),
        (false, true) => board_vendor,
        (true, false) => board_name,
        (true, true) => "Unknown".into(),
    };
    let bios_version =
        read_trimmed("/sys/class/dmi/id/bios_version").unwrap_or_else(|| "Unknown".into());

    SystemIdentity {
        vendor,
        product,
        board,
        bios_version,
    }
}

fn collect_cpu_identity() -> CpuIdentity {
    let content = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let model = content
        .lines()
        .find_map(|line| {
            line.strip_prefix("model name\t:")
                .or_else(|| line.strip_prefix("Model\t\t:"))
                .map(str::trim)
        })
        .filter(|value| !value.is_empty())
        .unwrap_or("Unknown CPU")
        .to_string();

    let logical_cpus = content
        .lines()
        .filter(|line| line.starts_with("processor\t:"))
        .count()
        .max(1);

    let packages: HashSet<String> = content
        .lines()
        .filter_map(|line| line.strip_prefix("physical id\t:").map(str::trim))
        .map(str::to_string)
        .collect();

    CpuIdentity {
        model,
        logical_cpus,
        physical_packages: (!packages.is_empty()).then_some(packages.len()),
    }
}

fn collect_memory_identity() -> MemoryIdentity {
    let content = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total_kib = 0u64;
    let mut available_kib = 0u64;
    for line in content.lines() {
        if let Some(value) = meminfo_kib(line, "MemTotal:") {
            total_kib = value;
        } else if let Some(value) = meminfo_kib(line, "MemAvailable:") {
            available_kib = value;
        }
    }
    MemoryIdentity {
        total_bytes: total_kib.saturating_mul(1024),
        available_bytes: available_kib.saturating_mul(1024),
    }
}

fn collect_gpus() -> Vec<GpuIdentity> {
    let mut gpus = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return gpus;
    };

    for entry in entries.flatten() {
        let card = entry.file_name().to_string_lossy().to_string();
        if !card.starts_with("card") || card.contains('-') {
            continue;
        }
        let device = entry.path().join("device");
        if !device.exists() {
            continue;
        }
        let driver = fs::read_link(device.join("driver"))
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "Unknown".into());
        let vendor = read_trimmed(device.join("vendor")).unwrap_or_default();
        let id = read_trimmed(device.join("device")).unwrap_or_default();
        let pci_id = if vendor.is_empty() && id.is_empty() {
            "Unknown".into()
        } else {
            format!(
                "{}:{}",
                vendor.trim_start_matches("0x"),
                id.trim_start_matches("0x")
            )
        };
        gpus.push(GpuIdentity {
            card,
            driver,
            pci_id,
        });
    }
    gpus.sort_by(|a, b| a.card.cmp(&b.card));
    gpus
}

fn collect_temperature_sensors() -> Vec<TemperatureSensor> {
    let mut sensors = Vec::new();
    let Ok(hwmons) = fs::read_dir("/sys/class/hwmon") else {
        return sensors;
    };

    for hwmon in hwmons.flatten() {
        let root = hwmon.path();
        let source = read_trimmed(root.join("name")).unwrap_or_else(|| "hwmon".into());
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let filename = entry.file_name().to_string_lossy().to_string();
            let Some(index) = filename
                .strip_prefix("temp")
                .and_then(|value| value.strip_suffix("_input"))
            else {
                continue;
            };
            let Some(raw) = read_i64(entry.path()) else {
                continue;
            };
            let celsius = raw as f64 / 1000.0;
            if !(-20.0..=150.0).contains(&celsius) {
                continue;
            }
            let label = read_trimmed(root.join(format!("temp{index}_label")))
                .unwrap_or_else(|| format!("Sensor {index}"));
            sensors.push(TemperatureSensor {
                source: source.clone(),
                label,
                celsius,
            });
            if sensors.len() >= 12 {
                return sensors;
            }
        }
    }
    sensors
}

fn is_virtual_block_name(name: &str) -> bool {
    ["loop", "ram", "zram", "dm-", "md"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn infer_transport(root: &Path, name: &str) -> String {
    if name.starts_with("nvme") {
        return "NVMe".into();
    }
    if name.starts_with("mmcblk") {
        return "MMC/eMMC".into();
    }
    let canonical = fs::canonicalize(root.join("device"))
        .unwrap_or_else(|_| PathBuf::new())
        .to_string_lossy()
        .to_ascii_lowercase();
    if canonical.contains("usb") {
        "USB".into()
    } else if canonical.contains("ata") {
        "ATA/SATA".into()
    } else if canonical.contains("virtio") {
        "VirtIO".into()
    } else if canonical.contains("scsi") {
        "SCSI/SAS".into()
    } else {
        "Block device".into()
    }
}

fn ata_raw_attribute(attributes: Option<&[Value]>, id: u64) -> Option<u64> {
    attributes?.iter().find_map(|attribute| {
        if attribute.get("id").and_then(number_as_u64) != Some(id) {
            return None;
        }
        attribute
            .pointer("/raw/value")
            .and_then(number_as_u64)
            .or_else(|| attribute.get("value").and_then(number_as_u64))
    })
}

fn number_as_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|number| number.parse::<u64>().ok()))
}

fn number_as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|number| number.parse::<f64>().ok()))
}

fn meminfo_kib(line: &str, prefix: &str) -> Option<u64> {
    let rest = line.strip_prefix(prefix)?;
    rest.split_whitespace().next()?.parse().ok()
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().trim_matches('\0').to_string())
        .filter(|value| !value.is_empty())
}

fn read_u64(path: impl AsRef<Path>) -> Option<u64> {
    read_trimmed(path)?.parse().ok()
}

fn read_i64(path: impl AsRef<Path>) -> Option<i64> {
    read_trimmed(path)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nvme_smart_json_without_serial_data() {
        let json = r#"{
          "device": {"protocol": "NVMe"},
          "smart_support": {"available": true, "enabled": true},
          "smart_status": {"passed": true},
          "temperature": {"current": 42},
          "power_on_time": {"hours": 1234},
          "nvme_smart_health_information_log": {
            "critical_warning": 0,
            "percentage_used": 7,
            "unsafe_shutdowns": 4,
            "media_errors": 0,
            "data_units_read": 111,
            "data_units_written": 222
          },
          "serial_number": "DO-NOT-KEEP"
        }"#;
        let reading = parse_smartctl_json("nvme0n1", json).expect("valid NVMe JSON");
        assert_eq!(reading.protocol, "NVMe");
        assert_eq!(reading.passed, Some(true));
        assert_eq!(reading.percentage_used, Some(7));
        assert_eq!(reading.media_errors, Some(0));
        let serialized = serde_json::to_string(&reading).expect("serialize reading");
        assert!(!serialized.contains("DO-NOT-KEEP"));
    }

    #[test]
    fn separates_smartctl_command_errors_from_health_status_bits() {
        let command_error =
            r#"{"smartctl":{"exit_status":2,"messages":[{"string":"Device open failed"}]}}"#;
        assert_eq!(
            smartctl_command_error(command_error).as_deref(),
            Some("Device open failed")
        );

        let health_only = r#"{"smartctl":{"exit_status":8}}"#;
        assert!(smartctl_command_error(health_only).is_none());
    }

    #[test]
    fn parses_ata_failure_counters() {
        let json = r#"{
          "device": {"protocol": "ATA"},
          "smart_status": {"passed": true},
          "ata_smart_attributes": {"table": [
            {"id": 5, "raw": {"value": 2}},
            {"id": 197, "raw": {"value": 1}},
            {"id": 198, "raw": {"value": 0}}
          ]},
          "ata_smart_error_log": {"summary": {"count": 3}}
        }"#;
        let reading = parse_smartctl_json("sda", json).expect("valid ATA JSON");
        assert_eq!(reading.reallocated_sectors, Some(2));
        assert_eq!(reading.pending_sectors, Some(1));
        assert_eq!(reading.offline_uncorrectable, Some(0));
        assert_eq!(reading.error_log_count, Some(3));
    }
}
