use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const PERSIST_INTERVAL_SECS: u64 = 15;
const HISTORY_WINDOW_SECS: u64 = 6 * 60 * 60;
const MAX_HISTORY_SAMPLES: usize = 1440;
const MAX_PERSISTED_PROCESSES: usize = 24;
const MIN_ANOMALY_SAMPLES: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetric {
    pub pid: u32,
    pub start_ticks: u64,
    pub name: String,
    pub owned_by_user: bool,
    pub cpu_pct: f64,
    pub ram_bytes: u64,
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
    pub network_bytes_per_sec: Option<u64>,
}

impl ProcessMetric {
    fn key(&self) -> ProcessKey {
        ProcessKey {
            pid: self.pid,
            start_ticks: self.start_ticks,
        }
    }

    pub fn io_bytes_per_sec(&self) -> u64 {
        self.read_bytes_per_sec
            .saturating_add(self.write_bytes_per_sec)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetric {
    pub cpu_pct: f64,
    pub ram_pct: f64,
    pub network_rx_bytes_per_sec: u64,
    pub network_tx_bytes_per_sec: u64,
    pub disk_read_bytes_per_sec: u64,
    pub disk_write_bytes_per_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub timestamp_unix: u64,
    pub system: SystemMetric,
    pub processes: Vec<ProcessMetric>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnomalySeverity {
    Attention,
    Critical,
}

impl AnomalySeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Attention => "ATTENTION",
            Self::Critical => "CRITICAL",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::Attention => "risk-review",
            Self::Critical => "risk-dangerous",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessAnomaly {
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub title: String,
    pub detail: String,
    pub severity: AnomalySeverity,
    pub first_observed_unix: u64,
}

#[derive(Debug, Clone, Default)]
pub struct HistoricalVitalsSummary {
    pub sample_count: usize,
    pub window_secs: u64,
    pub cpu_avg_pct: f64,
    pub cpu_peak_pct: f64,
    pub ram_avg_pct: f64,
    pub ram_peak_pct: f64,
    pub network_avg_bytes_per_sec: u64,
    pub network_peak_bytes_per_sec: u64,
    pub disk_io_avg_bytes_per_sec: u64,
    pub disk_io_peak_bytes_per_sec: u64,
}

#[derive(Debug, Clone)]
pub struct ProcessIntelligenceReport {
    pub current: TelemetrySnapshot,
    pub history_summary: HistoricalVitalsSummary,
    pub anomalies: Vec<ProcessAnomaly>,
    pub baseline_ready: bool,
    pub baseline_detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ProcessKey {
    pid: u32,
    start_ticks: u64,
}

#[derive(Debug, Clone, Copy)]
struct RawProcess {
    ticks: u64,
    read_bytes: u64,
    write_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
struct CpuCounters {
    total: u64,
    idle: u64,
    logical_cpus: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct ByteCounters {
    first: u64,
    second: u64,
}

pub struct ProcessSampler {
    home: PathBuf,
    previous_processes: HashMap<ProcessKey, RawProcess>,
    previous_cpu: Option<CpuCounters>,
    previous_network: Option<ByteCounters>,
    previous_disk: Option<ByteCounters>,
    previous_instant: Option<Instant>,
    last_persist_unix: u64,
    history: Vec<TelemetrySnapshot>,
    uid: u32,
}

impl ProcessSampler {
    pub fn new(home: PathBuf) -> Self {
        let now = now_unix();
        let mut history = load_history(&home).unwrap_or_default();
        trim_history(&mut history, now);
        let last_persist_unix = history
            .last()
            .map(|sample| sample.timestamp_unix)
            .unwrap_or(0);
        Self {
            home,
            previous_processes: HashMap::new(),
            previous_cpu: None,
            previous_network: None,
            previous_disk: None,
            previous_instant: None,
            last_persist_unix,
            history,
            uid: unsafe { libc::getuid() },
        }
    }

    pub fn sample_report(&mut self) -> ProcessIntelligenceReport {
        let now = now_unix();
        let now_instant = Instant::now();
        let elapsed_secs = self
            .previous_instant
            .map(|previous| now_instant.duration_since(previous).as_secs_f64())
            .unwrap_or(0.0);

        let cpu = read_cpu_counters().unwrap_or(CpuCounters {
            total: 0,
            idle: 0,
            logical_cpus: 1,
        });
        let cpu_pct = system_cpu_pct(self.previous_cpu, cpu);
        let total_tick_delta = self
            .previous_cpu
            .map(|previous| cpu.total.saturating_sub(previous.total))
            .unwrap_or(0);

        let current_raw = read_process_inventory(self.uid);
        let mut processes = Vec::with_capacity(current_raw.len());
        for (key, raw) in &current_raw {
            let previous = self.previous_processes.get(key);
            let cpu_pct = if total_tick_delta > 0 {
                previous
                    .map(|previous| {
                        let process_delta = raw.raw.ticks.saturating_sub(previous.ticks);
                        process_delta as f64 / total_tick_delta as f64
                            * cpu.logical_cpus.max(1) as f64
                            * 100.0
                    })
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            let read_rate = rate(
                previous.map(|previous| previous.read_bytes),
                raw.raw.read_bytes,
                elapsed_secs,
            );
            let write_rate = rate(
                previous.map(|previous| previous.write_bytes),
                raw.raw.write_bytes,
                elapsed_secs,
            );
            processes.push(ProcessMetric {
                pid: key.pid,
                start_ticks: key.start_ticks,
                name: raw.name.clone(),
                owned_by_user: raw.owned_by_user,
                cpu_pct: cpu_pct.clamp(0.0, cpu.logical_cpus.max(1) as f64 * 100.0),
                ram_bytes: raw.ram_bytes,
                read_bytes_per_sec: read_rate,
                write_bytes_per_sec: write_rate,
                network_bytes_per_sec: None,
            });
        }

        processes.sort_by(|a, b| {
            process_weight(b)
                .partial_cmp(&process_weight(a))
                .unwrap_or(Ordering::Equal)
        });
        processes.truncate(40);

        let memory = read_memory_pct();
        let network = read_network_counters().unwrap_or_default();
        let disk = read_disk_counters().unwrap_or_default();
        let system = SystemMetric {
            cpu_pct,
            ram_pct: memory,
            network_rx_bytes_per_sec: rate(
                self.previous_network.map(|previous| previous.first),
                network.first,
                elapsed_secs,
            ),
            network_tx_bytes_per_sec: rate(
                self.previous_network.map(|previous| previous.second),
                network.second,
                elapsed_secs,
            ),
            disk_read_bytes_per_sec: rate(
                self.previous_disk.map(|previous| previous.first),
                disk.first,
                elapsed_secs,
            ),
            disk_write_bytes_per_sec: rate(
                self.previous_disk.map(|previous| previous.second),
                disk.second,
                elapsed_secs,
            ),
        };

        let current = TelemetrySnapshot {
            timestamp_unix: now,
            system,
            processes,
        };

        let mut analysis_history = self.history.clone();
        analysis_history.push(current.clone());
        trim_history(&mut analysis_history, now);
        let history_summary = summarize_history(&analysis_history, now, 30 * 60);
        let baseline_ready = baseline_ready(&analysis_history, now);
        let anomalies = if baseline_ready {
            detect_anomalies(&analysis_history, &current)
        } else {
            Vec::new()
        };
        let baseline_detail = baseline_description(&analysis_history, now);

        if now.saturating_sub(self.last_persist_unix) >= PERSIST_INTERVAL_SECS {
            let mut persisted = current.clone();
            persisted.processes.truncate(MAX_PERSISTED_PROCESSES);
            self.history.push(persisted);
            trim_history(&mut self.history, now);
            let _ = save_history(&self.home, &self.history);
            self.last_persist_unix = now;
        }

        self.previous_processes = current_raw
            .into_iter()
            .map(|(key, raw)| (key, raw.raw))
            .collect();
        self.previous_cpu = Some(cpu);
        self.previous_network = Some(network);
        self.previous_disk = Some(disk);
        self.previous_instant = Some(now_instant);

        ProcessIntelligenceReport {
            current,
            history_summary,
            anomalies,
            baseline_ready,
            baseline_detail,
        }
    }
}

#[derive(Debug)]
struct InventoryProcess {
    name: String,
    owned_by_user: bool,
    ram_bytes: u64,
    raw: RawProcess,
}

fn read_process_inventory(uid: u32) -> HashMap<ProcessKey, InventoryProcess> {
    let mut result = HashMap::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return result;
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(pid_text) = file_name.to_str() else {
            continue;
        };
        let Ok(pid) = pid_text.parse::<u32>() else {
            continue;
        };
        if pid <= 1 {
            continue;
        }

        let proc_path = entry.path();
        let Some((ticks, start_ticks, stat_name)) = read_process_stat(&proc_path) else {
            continue;
        };
        let (name, process_uid, ram_bytes) =
            read_process_status(&proc_path).unwrap_or_else(|| (stat_name.clone(), None, 0));
        let io = read_process_io(&proc_path).unwrap_or_default();
        result.insert(
            ProcessKey { pid, start_ticks },
            InventoryProcess {
                name: if name.is_empty() { stat_name } else { name },
                owned_by_user: process_uid == Some(uid),
                ram_bytes,
                raw: RawProcess {
                    ticks,
                    read_bytes: io.first,
                    write_bytes: io.second,
                },
            },
        );
    }
    result
}

fn read_process_stat(proc_path: &Path) -> Option<(u64, u64, String)> {
    let content = fs::read_to_string(proc_path.join("stat")).ok()?;
    let open = content.find('(')?;
    let close = content.rfind(')')?;
    if close <= open {
        return None;
    }
    let name = content[open + 1..close].to_string();
    let fields: Vec<&str> = content[close + 1..].split_whitespace().collect();
    let utime = fields.get(11)?.parse::<u64>().ok()?;
    let stime = fields.get(12)?.parse::<u64>().ok()?;
    let start_ticks = fields.get(19)?.parse::<u64>().ok()?;
    Some((utime.saturating_add(stime), start_ticks, name))
}

fn read_process_status(proc_path: &Path) -> Option<(String, Option<u32>, u64)> {
    let content = fs::read_to_string(proc_path.join("status")).ok()?;
    let mut name = String::new();
    let mut uid = None;
    let mut ram_bytes = 0u64;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("Name:") {
            name = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("Uid:") {
            uid = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u32>().ok());
        } else if let Some(value) = line.strip_prefix("VmRSS:") {
            ram_bytes = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0)
                .saturating_mul(1024);
        }
    }
    Some((name, uid, ram_bytes))
}

fn read_process_io(proc_path: &Path) -> Option<ByteCounters> {
    let content = fs::read_to_string(proc_path.join("io")).ok()?;
    let mut read_bytes = 0u64;
    let mut write_bytes = 0u64;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("read_bytes:") {
            read_bytes = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = line.strip_prefix("write_bytes:") {
            write_bytes = value.trim().parse().unwrap_or(0);
        }
    }
    Some(ByteCounters {
        first: read_bytes,
        second: write_bytes,
    })
}

fn read_cpu_counters() -> Option<CpuCounters> {
    let content = fs::read_to_string("/proc/stat").ok()?;
    let mut total = 0u64;
    let mut idle = 0u64;
    let mut logical_cpus = 0u64;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("cpu ") {
            let values: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|value| value.parse::<u64>().ok())
                .collect();
            total = values.iter().copied().sum();
            idle = values
                .get(3)
                .copied()
                .unwrap_or(0)
                .saturating_add(values.get(4).copied().unwrap_or(0));
        } else if line.starts_with("cpu")
            && line
                .as_bytes()
                .get(3)
                .map(|byte| byte.is_ascii_digit())
                .unwrap_or(false)
        {
            logical_cpus = logical_cpus.saturating_add(1);
        }
    }
    Some(CpuCounters {
        total,
        idle,
        logical_cpus: logical_cpus.max(1),
    })
}

fn system_cpu_pct(previous: Option<CpuCounters>, current: CpuCounters) -> f64 {
    let Some(previous) = previous else {
        return 0.0;
    };
    let total = current.total.saturating_sub(previous.total);
    let idle = current.idle.saturating_sub(previous.idle);
    if total == 0 {
        0.0
    } else {
        ((total.saturating_sub(idle)) as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    }
}

fn read_memory_pct() -> f64 {
    let Ok(content) = fs::read_to_string("/proc/meminfo") else {
        return 0.0;
    };
    let mut total = 0u64;
    let mut available = 0u64;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("MemTotal:") {
            total = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
        } else if let Some(value) = line.strip_prefix("MemAvailable:") {
            available = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
        }
    }
    if total == 0 {
        0.0
    } else {
        (total.saturating_sub(available)) as f64 / total as f64 * 100.0
    }
}

fn read_network_counters() -> Option<ByteCounters> {
    let content = fs::read_to_string("/proc/net/dev").ok()?;
    let preferred_iface = default_route_interface();
    let mut fallback_rx = 0u64;
    let mut fallback_tx = 0u64;
    for line in content.lines().skip(2) {
        let Some((iface, values)) = line.split_once(':') else {
            continue;
        };
        let iface = iface.trim();
        if iface == "lo" {
            continue;
        }
        let fields: Vec<&str> = values.split_whitespace().collect();
        let rx = fields.first().and_then(|v| v.parse().ok()).unwrap_or(0);
        let tx = fields.get(8).and_then(|v| v.parse().ok()).unwrap_or(0);
        if preferred_iface.as_deref() == Some(iface) {
            return Some(ByteCounters {
                first: rx,
                second: tx,
            });
        }
        fallback_rx = fallback_rx.saturating_add(rx);
        fallback_tx = fallback_tx.saturating_add(tx);
    }
    Some(ByteCounters {
        first: fallback_rx,
        second: fallback_tx,
    })
}

fn default_route_interface() -> Option<String> {
    let content = fs::read_to_string("/proc/net/route").ok()?;
    content.lines().skip(1).find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.get(1).copied() == Some("00000000") {
            fields.first().map(|iface| (*iface).to_string())
        } else {
            None
        }
    })
}

fn read_disk_counters() -> Option<ByteCounters> {
    let content = fs::read_to_string("/proc/diskstats").ok()?;
    if let Some((root_major, root_minor)) = root_block_major_minor() {
        for line in content.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 10 {
                continue;
            }
            let major = fields.first().and_then(|value| value.parse::<u64>().ok());
            let minor = fields.get(1).and_then(|value| value.parse::<u64>().ok());
            if major == Some(root_major) && minor == Some(root_minor) {
                let sectors_read = fields
                    .get(5)
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
                let sectors_written = fields
                    .get(9)
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
                return Some(ByteCounters {
                    first: sectors_read.saturating_mul(512),
                    second: sectors_written.saturating_mul(512),
                });
            }
        }
    }

    let mut read_bytes = 0u64;
    let mut write_bytes = 0u64;
    for line in content.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }
        let name = fields[2];
        if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("zram") {
            continue;
        }
        if !Path::new("/sys/block").join(name).exists() {
            continue;
        }
        let sectors_read = fields
            .get(5)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let sectors_written = fields
            .get(9)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        read_bytes = read_bytes.saturating_add(sectors_read.saturating_mul(512));
        write_bytes = write_bytes.saturating_add(sectors_written.saturating_mul(512));
    }
    Some(ByteCounters {
        first: read_bytes,
        second: write_bytes,
    })
}

fn root_block_major_minor() -> Option<(u64, u64)> {
    let content = fs::read_to_string("/proc/self/mountinfo").ok()?;
    for line in content.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.get(4).copied() != Some("/") {
            continue;
        }
        let (major, minor) = fields.get(2)?.split_once(':')?;
        return Some((major.parse().ok()?, minor.parse().ok()?));
    }
    None
}

fn process_weight(process: &ProcessMetric) -> f64 {
    process.cpu_pct * 12.0
        + process.ram_bytes as f64 / (128.0 * 1024.0 * 1024.0)
        + process.io_bytes_per_sec() as f64 / (1024.0 * 1024.0)
}

fn rate(previous: Option<u64>, current: u64, elapsed_secs: f64) -> u64 {
    if elapsed_secs <= 0.0 || !elapsed_secs.is_finite() {
        return 0;
    }
    previous
        .map(|previous| current.saturating_sub(previous) as f64 / elapsed_secs)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0)
        .round()
        .clamp(0.0, u64::MAX as f64) as u64
}

fn baseline_ready(history: &[TelemetrySnapshot], now: u64) -> bool {
    if history.len() < MIN_ANOMALY_SAMPLES {
        return false;
    }
    let oldest = history
        .get(history.len() - MIN_ANOMALY_SAMPLES)
        .map(|sample| sample.timestamp_unix)
        .unwrap_or(now);
    now.saturating_sub(oldest) >= 60
}

fn baseline_description(history: &[TelemetrySnapshot], now: u64) -> String {
    if baseline_ready(history, now) {
        let oldest = history
            .first()
            .map(|sample| sample.timestamp_unix)
            .unwrap_or(now);
        format!(
            "{} samples • {} of local history",
            history.len(),
            age_short(now.saturating_sub(oldest))
        )
    } else {
        format!(
            "Learning baseline • {}/{} minimum samples collected",
            history.len().min(MIN_ANOMALY_SAMPLES),
            MIN_ANOMALY_SAMPLES
        )
    }
}

fn summarize_history(
    history: &[TelemetrySnapshot],
    now: u64,
    window_secs: u64,
) -> HistoricalVitalsSummary {
    let samples: Vec<&TelemetrySnapshot> = history
        .iter()
        .filter(|sample| now.saturating_sub(sample.timestamp_unix) <= window_secs)
        .collect();
    if samples.is_empty() {
        return HistoricalVitalsSummary::default();
    }

    let mut cpu_sum = 0.0;
    let mut cpu_peak: f64 = 0.0;
    let mut ram_sum = 0.0;
    let mut ram_peak: f64 = 0.0;
    let mut network_sum = 0u128;
    let mut network_peak = 0u64;
    let mut disk_sum = 0u128;
    let mut disk_peak = 0u64;
    for sample in &samples {
        cpu_sum += sample.system.cpu_pct;
        cpu_peak = cpu_peak.max(sample.system.cpu_pct);
        ram_sum += sample.system.ram_pct;
        ram_peak = ram_peak.max(sample.system.ram_pct);
        let network = sample
            .system
            .network_rx_bytes_per_sec
            .saturating_add(sample.system.network_tx_bytes_per_sec);
        network_sum = network_sum.saturating_add(network as u128);
        network_peak = network_peak.max(network);
        let disk = sample
            .system
            .disk_read_bytes_per_sec
            .saturating_add(sample.system.disk_write_bytes_per_sec);
        disk_sum = disk_sum.saturating_add(disk as u128);
        disk_peak = disk_peak.max(disk);
    }
    let count = samples.len();
    let oldest = samples
        .first()
        .map(|sample| sample.timestamp_unix)
        .unwrap_or(now);
    HistoricalVitalsSummary {
        sample_count: count,
        window_secs: now.saturating_sub(oldest).min(window_secs),
        cpu_avg_pct: cpu_sum / count as f64,
        cpu_peak_pct: cpu_peak,
        ram_avg_pct: ram_sum / count as f64,
        ram_peak_pct: ram_peak,
        network_avg_bytes_per_sec: (network_sum / count as u128) as u64,
        network_peak_bytes_per_sec: network_peak,
        disk_io_avg_bytes_per_sec: (disk_sum / count as u128) as u64,
        disk_io_peak_bytes_per_sec: disk_peak,
    }
}

fn detect_anomalies(
    history: &[TelemetrySnapshot],
    current: &TelemetrySnapshot,
) -> Vec<ProcessAnomaly> {
    let baseline: Vec<&TelemetrySnapshot> = history
        .iter()
        .filter(|sample| sample.timestamp_unix < current.timestamp_unix)
        .rev()
        .take(80)
        .collect();
    if baseline.len() < MIN_ANOMALY_SAMPLES {
        return Vec::new();
    }

    let mut anomalies = Vec::new();
    let baseline_cpu = median(
        baseline
            .iter()
            .map(|sample| sample.system.cpu_pct)
            .collect(),
    );
    if current.system.cpu_pct >= 90.0 && current.system.cpu_pct >= baseline_cpu + 20.0 {
        anomalies.push(ProcessAnomaly {
            pid: None,
            process_name: None,
            title: "System CPU spike".to_string(),
            detail: format!(
                "CPU is {:.0}% versus a {:.0}% recent median.",
                current.system.cpu_pct, baseline_cpu
            ),
            severity: AnomalySeverity::Critical,
            first_observed_unix: first_system_crossing(history, current.timestamp_unix, |sample| {
                sample.system.cpu_pct >= 85.0
            }),
        });
    }

    let current_network = current
        .system
        .network_rx_bytes_per_sec
        .saturating_add(current.system.network_tx_bytes_per_sec);
    let baseline_network = median_u64(
        baseline
            .iter()
            .map(|sample| {
                sample
                    .system
                    .network_rx_bytes_per_sec
                    .saturating_add(sample.system.network_tx_bytes_per_sec)
            })
            .collect(),
    );
    if current_network >= 5 * 1024 * 1024
        && current_network > baseline_network.saturating_mul(4).max(2 * 1024 * 1024)
    {
        anomalies.push(ProcessAnomaly {
            pid: None,
            process_name: None,
            title: "Network throughput spike".to_string(),
            detail: format!(
                "System network traffic is {} /s versus a recent median of {} /s. LinuxCare does not attribute network bytes to a PID without eBPF/cgroup instrumentation.",
                crate::format::bytes(current_network),
                crate::format::bytes(baseline_network)
            ),
            severity: AnomalySeverity::Attention,
            first_observed_unix: first_system_crossing(history, current.timestamp_unix, |sample| {
                sample
                    .system
                    .network_rx_bytes_per_sec
                    .saturating_add(sample.system.network_tx_bytes_per_sec)
                    >= 5 * 1024 * 1024
            }),
        });
    }

    let current_disk = current
        .system
        .disk_read_bytes_per_sec
        .saturating_add(current.system.disk_write_bytes_per_sec);
    let baseline_disk = median_u64(
        baseline
            .iter()
            .map(|sample| {
                sample
                    .system
                    .disk_read_bytes_per_sec
                    .saturating_add(sample.system.disk_write_bytes_per_sec)
            })
            .collect(),
    );
    if current_disk >= 30 * 1024 * 1024
        && current_disk > baseline_disk.saturating_mul(4).max(10 * 1024 * 1024)
    {
        anomalies.push(ProcessAnomaly {
            pid: None,
            process_name: None,
            title: "System disk I/O spike".to_string(),
            detail: format!(
                "Block-device I/O is {} /s versus a recent median of {} /s.",
                crate::format::bytes(current_disk),
                crate::format::bytes(baseline_disk)
            ),
            severity: AnomalySeverity::Attention,
            first_observed_unix: first_system_crossing(history, current.timestamp_unix, |sample| {
                sample
                    .system
                    .disk_read_bytes_per_sec
                    .saturating_add(sample.system.disk_write_bytes_per_sec)
                    >= 30 * 1024 * 1024
            }),
        });
    }

    for process in current.processes.iter().take(24) {
        let key = process.key();
        let process_history: Vec<&ProcessMetric> = baseline
            .iter()
            .filter_map(|sample| {
                sample
                    .processes
                    .iter()
                    .find(|candidate| candidate.key() == key)
            })
            .collect();
        if process_history.len() < MIN_ANOMALY_SAMPLES {
            continue;
        }

        let cpu_baseline = median(
            process_history
                .iter()
                .map(|sample| sample.cpu_pct)
                .collect(),
        );
        if process.cpu_pct >= 35.0 && process.cpu_pct >= cpu_baseline + 20.0 {
            anomalies.push(ProcessAnomaly {
                pid: Some(process.pid),
                process_name: Some(process.name.clone()),
                title: format!("{} CPU growth", process.name),
                detail: format!(
                    "PID {} is using {:.0}% CPU versus a {:.0}% recent median.",
                    process.pid, process.cpu_pct, cpu_baseline
                ),
                severity: if process.cpu_pct >= 90.0 {
                    AnomalySeverity::Critical
                } else {
                    AnomalySeverity::Attention
                },
                first_observed_unix: first_process_crossing(
                    history,
                    key,
                    current.timestamp_unix,
                    |sample| sample.cpu_pct >= 30.0,
                ),
            });
        }

        let oldest_ram = process_history
            .last()
            .map(|sample| sample.ram_bytes)
            .unwrap_or(0);
        let ram_growth = process.ram_bytes.saturating_sub(oldest_ram);
        let meaningful_ratio =
            oldest_ram == 0 || process.ram_bytes as f64 / oldest_ram.max(1) as f64 >= 1.25;
        if ram_growth >= 256 * 1024 * 1024 && meaningful_ratio {
            anomalies.push(ProcessAnomaly {
                pid: Some(process.pid),
                process_name: Some(process.name.clone()),
                title: format!("{} memory growth", process.name),
                detail: format!(
                    "PID {} grew by {} during the observed window ({} → {}).",
                    process.pid,
                    crate::format::bytes(ram_growth),
                    crate::format::bytes(oldest_ram),
                    crate::format::bytes(process.ram_bytes)
                ),
                severity: if ram_growth >= 1024 * 1024 * 1024 {
                    AnomalySeverity::Critical
                } else {
                    AnomalySeverity::Attention
                },
                first_observed_unix: history
                    .iter()
                    .find(|snapshot| {
                        snapshot
                            .processes
                            .iter()
                            .any(|candidate| candidate.key() == key)
                    })
                    .map(|snapshot| snapshot.timestamp_unix)
                    .unwrap_or(current.timestamp_unix),
            });
        }

        let io_baseline = median_u64(
            process_history
                .iter()
                .map(|sample| sample.io_bytes_per_sec())
                .collect(),
        );
        let current_io = process.io_bytes_per_sec();
        if current_io >= 20 * 1024 * 1024
            && current_io > io_baseline.saturating_mul(3).max(8 * 1024 * 1024)
        {
            anomalies.push(ProcessAnomaly {
                pid: Some(process.pid),
                process_name: Some(process.name.clone()),
                title: format!("{} disk I/O spike", process.name),
                detail: format!(
                    "PID {} is reading/writing {} /s versus a recent median of {} /s.",
                    process.pid,
                    crate::format::bytes(current_io),
                    crate::format::bytes(io_baseline)
                ),
                severity: AnomalySeverity::Attention,
                first_observed_unix: first_process_crossing(
                    history,
                    key,
                    current.timestamp_unix,
                    |sample| sample.io_bytes_per_sec() >= 16 * 1024 * 1024,
                ),
            });
        }
    }

    let mut seen = HashSet::new();
    anomalies.retain(|anomaly| seen.insert((anomaly.pid, anomaly.title.clone())));
    anomalies.sort_by_key(|anomaly| match anomaly.severity {
        AnomalySeverity::Critical => 0,
        AnomalySeverity::Attention => 1,
    });
    anomalies.truncate(10);
    anomalies
}

fn first_process_crossing(
    history: &[TelemetrySnapshot],
    key: ProcessKey,
    now: u64,
    predicate: impl Fn(&ProcessMetric) -> bool,
) -> u64 {
    let mut first = now;
    for snapshot in history.iter().rev() {
        if now.saturating_sub(snapshot.timestamp_unix) > 30 * 60 {
            break;
        }
        let Some(process) = snapshot
            .processes
            .iter()
            .find(|process| process.key() == key)
        else {
            break;
        };
        if !predicate(process) {
            break;
        }
        first = snapshot.timestamp_unix;
    }
    first
}

fn first_system_crossing(
    history: &[TelemetrySnapshot],
    now: u64,
    predicate: impl Fn(&TelemetrySnapshot) -> bool,
) -> u64 {
    let mut first = now;
    for snapshot in history.iter().rev() {
        if now.saturating_sub(snapshot.timestamp_unix) > 30 * 60 || !predicate(snapshot) {
            break;
        }
        first = snapshot.timestamp_unix;
    }
    first
}

pub fn anomaly_age(now: u64, first_observed_unix: u64) -> String {
    let seconds = now.saturating_sub(first_observed_unix);
    if seconds < 60 {
        "observed now".to_string()
    } else {
        format!("observed for {}", age_short(seconds))
    }
}

fn age_short(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}s", seconds)
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

fn median_u64(mut values: Vec<u64>) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        values[middle - 1].saturating_add(values[middle]) / 2
    } else {
        values[middle]
    }
}

fn trim_history(history: &mut Vec<TelemetrySnapshot>, now: u64) {
    history.retain(|sample| now.saturating_sub(sample.timestamp_unix) <= HISTORY_WINDOW_SECS);
    if history.len() > MAX_HISTORY_SAMPLES {
        history.drain(0..history.len() - MAX_HISTORY_SAMPLES);
    }
}

fn history_path(home: &Path) -> PathBuf {
    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        PathBuf::from(state_home).join("linuxcare/process-intelligence.json")
    } else {
        home.join(".local/state/linuxcare/process-intelligence.json")
    }
}

fn load_history(home: &Path) -> io::Result<Vec<TelemetrySnapshot>> {
    let path = history_path(home);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read(path)?;
    serde_json::from_slice(&data).map_err(io::Error::other)
}

fn save_history(home: &Path, history: &[TelemetrySnapshot]) -> io::Result<()> {
    let path = history_path(home);
    let Some(parent) = path.parent() else {
        return Err(io::Error::other("process history path has no parent"));
    };
    fs::create_dir_all(parent)?;
    set_private_dir(parent)?;
    let data = serde_json::to_vec(history).map_err(io::Error::other)?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(&data)?;
    file.sync_all()?;
    fs::rename(&tmp, &path)?;
    set_private_file(&path)?;
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

#[cfg(unix)]
fn set_private_file(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, cpu: f64, ram: u64, io: u64) -> ProcessMetric {
        ProcessMetric {
            pid,
            start_ticks: 99,
            name: "example".into(),
            owned_by_user: true,
            cpu_pct: cpu,
            ram_bytes: ram,
            read_bytes_per_sec: io,
            write_bytes_per_sec: 0,
            network_bytes_per_sec: None,
        }
    }

    fn snapshot(ts: u64, cpu: f64, process: ProcessMetric) -> TelemetrySnapshot {
        TelemetrySnapshot {
            timestamp_unix: ts,
            system: SystemMetric {
                cpu_pct: cpu,
                ram_pct: 40.0,
                network_rx_bytes_per_sec: 0,
                network_tx_bytes_per_sec: 0,
                disk_read_bytes_per_sec: 0,
                disk_write_bytes_per_sec: 0,
            },
            processes: vec![process],
        }
    }

    #[test]
    fn detects_process_cpu_spike_after_baseline() {
        let mut history = Vec::new();
        for i in 0..6 {
            history.push(snapshot(i * 20, 20.0, process(100, 5.0, 200_000_000, 0)));
        }
        let current = snapshot(140, 30.0, process(100, 80.0, 200_000_000, 0));
        history.push(current.clone());
        let anomalies = detect_anomalies(&history, &current);
        assert!(anomalies
            .iter()
            .any(|anomaly| anomaly.title.contains("CPU growth")));
    }

    #[test]
    fn detects_large_memory_growth() {
        let mut history = Vec::new();
        for i in 0..6 {
            history.push(snapshot(i * 20, 20.0, process(100, 5.0, 200_000_000, 0)));
        }
        let current = snapshot(140, 20.0, process(100, 5.0, 700_000_000, 0));
        history.push(current.clone());
        let anomalies = detect_anomalies(&history, &current);
        assert!(anomalies
            .iter()
            .any(|anomaly| anomaly.title.contains("memory growth")));
    }

    #[test]
    fn baseline_requires_time_and_samples() {
        let history = vec![
            snapshot(0, 10.0, process(1, 1.0, 1, 0)),
            snapshot(15, 10.0, process(1, 1.0, 1, 0)),
            snapshot(30, 10.0, process(1, 1.0, 1, 0)),
            snapshot(45, 10.0, process(1, 1.0, 1, 0)),
            snapshot(60, 10.0, process(1, 1.0, 1, 0)),
        ];
        assert!(baseline_ready(&history, 60));
    }
}
