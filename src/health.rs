use crate::{format, model::CleanupCandidate, system_info};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const SYSTEM_SNAPSHOT_INTERVAL_SECS: u64 = 6 * 60 * 60;
const MAX_SYSTEM_SNAPSHOTS: usize = 120;
const MAX_SCAN_SNAPSHOTS: usize = 60;
const WEEK_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalState {
    Good,
    Attention,
    Critical,
    Neutral,
    Unknown,
}

impl SignalState {
    pub fn css_class(self) -> &'static str {
        match self {
            Self::Good => "risk-safe",
            Self::Attention => "risk-review",
            Self::Critical => "risk-dangerous",
            Self::Neutral | Self::Unknown => "health-neutral",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Good => "GOOD",
            Self::Attention => "ATTENTION",
            Self::Critical => "CRITICAL",
            Self::Neutral => "INFO",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HealthSignal {
    pub title: String,
    pub detail: String,
    pub state: SignalState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeTone {
    Positive,
    Attention,
    Neutral,
}

impl ChangeTone {
    pub fn css_class(self) -> &'static str {
        match self {
            Self::Positive => "change-positive",
            Self::Attention => "change-attention",
            Self::Neutral => "change-neutral",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemChange {
    pub title: String,
    pub detail: String,
    pub tone: ChangeTone,
}

#[derive(Debug, Clone)]
pub struct HealthReport {
    pub score: u8,
    pub grade: String,
    pub summary: String,
    pub domains: Vec<HealthDomain>,
    pub signals: Vec<HealthSignal>,
    pub changes: Vec<SystemChange>,
    pub baseline_label: String,
    pub scan_baseline_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HealthDomain {
    pub name: String,
    pub score: Option<u8>,
    pub detail: String,
    pub state: SignalState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SystemSnapshot {
    timestamp_unix: u64,
    kernel_version: String,
    disk_root_used_bytes: u64,
    disk_root_total_bytes: u64,
    disk_home_used_bytes: u64,
    disk_home_total_bytes: u64,
    ram_used_bytes: u64,
    ram_total_bytes: u64,
    failed_system_units: Option<u32>,
    failed_user_units: Option<u32>,
    firewall_active: Option<bool>,
    network_bound_ports: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScanSnapshot {
    timestamp_unix: u64,
    total_reclaimable_bytes: u64,
    categories: BTreeMap<String, u64>,
}

pub fn collect_report(home: &Path) -> HealthReport {
    let stats = system_info::collect_system_stats();
    let current = SystemSnapshot {
        timestamp_unix: now_unix(),
        kernel_version: stats.kernel_version.clone(),
        disk_root_used_bytes: stats.disk_root_used_bytes,
        disk_root_total_bytes: stats.disk_root_total_bytes,
        disk_home_used_bytes: stats.disk_home_used_bytes,
        disk_home_total_bytes: stats.disk_home_total_bytes,
        ram_used_bytes: stats.ram_used_bytes,
        ram_total_bytes: stats.ram_total_bytes,
        failed_system_units: failed_units(false),
        failed_user_units: failed_units(true),
        firewall_active: firewall_state(),
        network_bound_ports: network_bound_port_count(),
    };

    let (_, signals) = score_snapshot(&current);
    let domains = collect_domain_scores(&current);
    let score = weighted_overall_score(&domains).unwrap_or(100);
    let grade = grade_for(score).to_string();
    let summary = format!(
        "{} Known domain scores are combined conservatively; unavailable domains are excluded rather than treated as failures.",
        summary_for(score)
    );

    let mut system_history: Vec<SystemSnapshot> =
        load_json(&system_snapshot_path(home)).unwrap_or_default();
    let baseline = select_week_baseline(&system_history, current.timestamp_unix).cloned();
    let baseline_label = baseline
        .as_ref()
        .map(|b| age_label(current.timestamp_unix, b.timestamp_unix))
        .unwrap_or_else(|| "Baseline created now".to_string());

    let mut changes = baseline
        .as_ref()
        .map(|b| compare_system(b, &current))
        .unwrap_or_default();

    maybe_record_system_snapshot(home, &mut system_history, current);

    let scan_history: Vec<ScanSnapshot> = load_json(&scan_snapshot_path(home)).unwrap_or_default();
    let scan_baseline_label = if scan_history.len() >= 2 {
        let previous = &scan_history[scan_history.len() - 2];
        let latest = &scan_history[scan_history.len() - 1];
        changes.extend(compare_scans(previous, latest));
        Some(format!(
            "Smart Scan: {}",
            age_label(latest.timestamp_unix, previous.timestamp_unix)
        ))
    } else if scan_history.len() == 1 {
        Some("Smart Scan baseline captured".to_string())
    } else {
        None
    };

    if changes.is_empty() && baseline.is_some() {
        changes.push(SystemChange {
            title: "No meaningful drift detected".to_string(),
            detail: "Disk pressure, kernel, failed services, firewall state, and network listeners are materially unchanged from the baseline.".to_string(),
            tone: ChangeTone::Positive,
        });
    }

    changes.truncate(8);

    HealthReport {
        score,
        grade,
        summary,
        domains,
        signals,
        changes,
        baseline_label,
        scan_baseline_label,
    }
}

pub fn record_scan_snapshot(home: &Path, candidates: &[CleanupCandidate]) -> io::Result<()> {
    let mut categories = BTreeMap::<String, u64>::new();
    let mut total_reclaimable_bytes = 0u64;

    for candidate in candidates {
        let entry = categories
            .entry(candidate.category.label().to_string())
            .or_insert(0);
        *entry = (*entry).saturating_add(candidate.size_bytes);
        if candidate.reclaimable_now() {
            total_reclaimable_bytes = total_reclaimable_bytes.saturating_add(candidate.size_bytes);
        }
    }

    let snapshot = ScanSnapshot {
        timestamp_unix: now_unix(),
        total_reclaimable_bytes,
        categories,
    };

    let path = scan_snapshot_path(home);
    let mut history: Vec<ScanSnapshot> = load_json(&path).unwrap_or_default();
    history.push(snapshot);
    if history.len() > MAX_SCAN_SNAPSHOTS {
        let drain = history.len() - MAX_SCAN_SNAPSHOTS;
        history.drain(0..drain);
    }
    save_json_atomic(&path, &history)
}

fn collect_domain_scores(snapshot: &SystemSnapshot) -> Vec<HealthDomain> {
    let disk_pct = ratio(
        snapshot.disk_root_used_bytes,
        snapshot.disk_root_total_bytes,
    ) * 100.0;
    let storage_score = if snapshot.disk_root_total_bytes == 0 {
        None
    } else if disk_pct >= 95.0 {
        Some(25)
    } else if disk_pct >= 90.0 {
        Some(50)
    } else if disk_pct >= 80.0 {
        Some(78)
    } else {
        Some(100)
    };

    let ram_pct = ratio(snapshot.ram_used_bytes, snapshot.ram_total_bytes) * 100.0;
    let performance_score = if snapshot.ram_total_bytes == 0 {
        None
    } else if ram_pct >= 95.0 {
        Some(45)
    } else if ram_pct >= 90.0 {
        Some(65)
    } else if ram_pct >= 80.0 {
        Some(82)
    } else {
        Some(100)
    };

    let failed_total = match (snapshot.failed_system_units, snapshot.failed_user_units) {
        (None, None) => None,
        (system, user) => Some(system.unwrap_or(0).saturating_add(user.unwrap_or(0))),
    };
    let reliability_score =
        failed_total.map(|count| 100u8.saturating_sub((count.min(5) * 16) as u8));
    let security_score = snapshot
        .firewall_active
        .map(|active| if active { 100 } else { 72 });

    let network_score = quick_network_score();
    let package_score = quick_package_score();
    let thermal = crate::thermal_doctor::collect_report();
    let thermal_score = thermal.max_temperature_c.map(|temperature| {
        if temperature >= 95.0 {
            30
        } else if temperature >= 88.0 {
            55
        } else if temperature >= 82.0 {
            75
        } else if temperature >= 75.0 {
            90
        } else {
            100
        }
    });
    let battery = crate::battery::collect_report();
    let battery_health = battery
        .batteries
        .iter()
        .filter_map(|device| device.health_percent)
        .reduce(f64::min);
    let battery_score = battery_health.map(|health| {
        if health >= 90.0 {
            100
        } else if health >= 80.0 {
            90
        } else if health >= 70.0 {
            75
        } else {
            55
        }
    });

    vec![
        domain(
            "Storage",
            storage_score,
            format!("Root filesystem {disk_pct:.0}% used"),
        ),
        domain(
            "Performance",
            performance_score,
            format!("RAM non-available {ram_pct:.0}%"),
        ),
        domain(
            "Reliability",
            reliability_score,
            failed_total
                .map(|count| format!("{count} failed system/user service unit(s)"))
                .unwrap_or_else(|| "systemd failure state unavailable".to_string()),
        ),
        domain(
            "Security",
            security_score,
            match snapshot.firewall_active {
                Some(true) => "UFW active; other firewall layers are not inferred".to_string(),
                Some(false) => {
                    "UFW inactive; nftables/firewalld/upstream policy may still exist".to_string()
                }
                None => "Firewall state unavailable".to_string(),
            },
        ),
        domain("Network", network_score, quick_network_detail()),
        domain("Packages", package_score, quick_package_detail()),
        domain(
            "Thermal",
            thermal_score,
            thermal
                .max_temperature_c
                .map(|value| format!("Hottest exposed sensor {value:.1}°C"))
                .unwrap_or_else(|| "No usable temperature sensor exposed".to_string()),
        ),
        domain(
            "Battery",
            battery_score,
            battery_health
                .map(|value| format!("Lowest reported full/design capacity ratio {value:.0}%"))
                .unwrap_or_else(|| "No battery health ratio available".to_string()),
        ),
    ]
}

fn domain(name: &str, score: Option<u8>, detail: String) -> HealthDomain {
    let score = score.map(|value| value.min(100));
    let state = match score {
        Some(90..) => SignalState::Good,
        Some(70..=89) => SignalState::Attention,
        Some(..=69) => SignalState::Critical,
        None => SignalState::Unknown,
    };
    HealthDomain {
        name: name.to_string(),
        score,
        detail,
        state,
    }
}

fn weighted_overall_score(domains: &[HealthDomain]) -> Option<u8> {
    let weights = [
        ("Storage", 20u32),
        ("Performance", 15),
        ("Reliability", 15),
        ("Security", 10),
        ("Network", 10),
        ("Packages", 15),
        ("Thermal", 10),
        ("Battery", 5),
    ];
    let mut weighted = 0u32;
    let mut total_weight = 0u32;
    for domain in domains {
        let Some(score) = domain.score else {
            continue;
        };
        let weight = weights
            .iter()
            .find(|(name, _)| *name == domain.name.as_str())
            .map(|(_, weight)| *weight)
            .unwrap_or(1);
        weighted = weighted.saturating_add(score as u32 * weight);
        total_weight = total_weight.saturating_add(weight);
    }
    (total_weight > 0).then_some((weighted / total_weight).min(100) as u8)
}

fn quick_network_score() -> Option<u8> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "CONNECTIVITY", "general"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    match String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_lowercase()
        .as_str()
    {
        "full" => Some(100),
        "limited" => Some(72),
        "portal" => Some(65),
        "none" => Some(35),
        _ => None,
    }
}

fn quick_network_detail() -> String {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "CONNECTIVITY", "general"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            format!("NetworkManager connectivity: {value}")
        }
        _ => "NetworkManager connectivity unavailable".to_string(),
    }
}

fn quick_package_score() -> Option<u8> {
    let output = Command::new("/usr/bin/dpkg").arg("--audit").output().ok()?;
    if !output.status.success() {
        return Some(60);
    }
    if output.stdout.is_empty() {
        Some(100)
    } else {
        Some(65)
    }
}

fn quick_package_detail() -> String {
    let output = Command::new("/usr/bin/dpkg").arg("--audit").output();
    match output {
        Ok(output) if output.status.success() && output.stdout.is_empty() => {
            "dpkg --audit reports no incomplete package state".to_string()
        }
        Ok(output) if output.status.success() => {
            "dpkg --audit reports package state that needs review".to_string()
        }
        Ok(_) => "dpkg --audit returned a non-success status".to_string(),
        Err(_) => "dpkg audit unavailable".to_string(),
    }
}

fn score_snapshot(snapshot: &SystemSnapshot) -> (u8, Vec<HealthSignal>) {
    let mut penalty = 0u16;
    let mut signals = Vec::new();

    let disk_pct = ratio(
        snapshot.disk_root_used_bytes,
        snapshot.disk_root_total_bytes,
    );
    let (disk_state, disk_penalty) = if disk_pct >= 0.95 {
        (SignalState::Critical, 30)
    } else if disk_pct >= 0.90 {
        (SignalState::Critical, 22)
    } else if disk_pct >= 0.80 {
        (SignalState::Attention, 10)
    } else {
        (SignalState::Good, 0)
    };
    penalty += disk_penalty;
    signals.push(HealthSignal {
        title: "System storage".to_string(),
        detail: format!(
            "{:.0}% used • {} free",
            disk_pct * 100.0,
            format::bytes(
                snapshot
                    .disk_root_total_bytes
                    .saturating_sub(snapshot.disk_root_used_bytes)
            )
        ),
        state: disk_state,
    });

    let ram_pct = ratio(snapshot.ram_used_bytes, snapshot.ram_total_bytes);
    let (ram_state, ram_penalty) = if ram_pct >= 0.95 {
        (SignalState::Critical, 12)
    } else if ram_pct >= 0.90 {
        (SignalState::Attention, 8)
    } else if ram_pct >= 0.80 {
        (SignalState::Attention, 4)
    } else {
        (SignalState::Good, 0)
    };
    penalty += ram_penalty;
    signals.push(HealthSignal {
        title: "Memory pressure".to_string(),
        detail: format!("{:.0}% of RAM is currently non-available", ram_pct * 100.0),
        state: ram_state,
    });

    push_failed_signal(
        &mut signals,
        &mut penalty,
        "System services",
        snapshot.failed_system_units,
        5,
    );
    push_failed_signal(
        &mut signals,
        &mut penalty,
        "User services",
        snapshot.failed_user_units,
        3,
    );

    match snapshot.firewall_active {
        Some(true) => signals.push(HealthSignal {
            title: "Firewall".to_string(),
            detail: "UFW reports an active firewall policy".to_string(),
            state: SignalState::Good,
        }),
        Some(false) => {
            penalty += 3;
            signals.push(HealthSignal {
                title: "Firewall".to_string(),
                detail: "UFW reports an inactive policy; this does not prove nftables, firewalld, or upstream policy is absent".to_string(),
                state: SignalState::Attention,
            });
        }
        None => signals.push(HealthSignal {
            title: "Firewall".to_string(),
            detail: "UFW state is unavailable; LinuxCare does not guess".to_string(),
            state: SignalState::Unknown,
        }),
    }

    signals.push(HealthSignal {
        title: "Network listeners".to_string(),
        detail: snapshot
            .network_bound_ports
            .map(|count| {
                format!(
                    "{count} listening socket(s) are bound beyond loopback; reachability still depends on firewall/router rules"
                )
            })
            .unwrap_or_else(|| "Listening socket data is unavailable".to_string()),
        state: SignalState::Neutral,
    });

    let score = 100u16.saturating_sub(penalty.min(100)) as u8;
    (score, signals)
}

fn push_failed_signal(
    signals: &mut Vec<HealthSignal>,
    penalty: &mut u16,
    title: &str,
    count: Option<u32>,
    max_penalty_per_unit: u16,
) {
    match count {
        Some(0) => signals.push(HealthSignal {
            title: title.to_string(),
            detail: "No failed service units detected".to_string(),
            state: SignalState::Good,
        }),
        Some(count) => {
            let p = (count as u16).saturating_mul(max_penalty_per_unit).min(20);
            *penalty = (*penalty).saturating_add(p);
            signals.push(HealthSignal {
                title: title.to_string(),
                detail: format!("{count} failed service unit(s) detected"),
                state: if count >= 3 {
                    SignalState::Critical
                } else {
                    SignalState::Attention
                },
            });
        }
        None => signals.push(HealthSignal {
            title: title.to_string(),
            detail: "systemd state is unavailable".to_string(),
            state: SignalState::Unknown,
        }),
    }
}

fn compare_system(previous: &SystemSnapshot, current: &SystemSnapshot) -> Vec<SystemChange> {
    let mut changes = Vec::new();
    let root_delta = signed_delta(current.disk_root_used_bytes, previous.disk_root_used_bytes);
    if root_delta.unsigned_abs() >= 64 * 1024 * 1024 {
        changes.push(SystemChange {
            title: if root_delta > 0 {
                "System disk usage increased".to_string()
            } else {
                "System disk usage decreased".to_string()
            },
            detail: format_delta_bytes(root_delta),
            tone: if root_delta > 0 {
                ChangeTone::Attention
            } else {
                ChangeTone::Positive
            },
        });
    }

    let home_is_probably_same_fs = current.disk_home_total_bytes == current.disk_root_total_bytes
        && previous.disk_home_total_bytes == previous.disk_root_total_bytes;
    if !home_is_probably_same_fs {
        let home_delta = signed_delta(current.disk_home_used_bytes, previous.disk_home_used_bytes);
        if home_delta.unsigned_abs() >= 64 * 1024 * 1024 {
            changes.push(SystemChange {
                title: if home_delta > 0 {
                    "Home storage grew".to_string()
                } else {
                    "Home storage shrank".to_string()
                },
                detail: format_delta_bytes(home_delta),
                tone: if home_delta > 0 {
                    ChangeTone::Attention
                } else {
                    ChangeTone::Positive
                },
            });
        }
    }

    if previous.kernel_version != current.kernel_version {
        changes.push(SystemChange {
            title: "Kernel changed".to_string(),
            detail: format!("{} → {}", previous.kernel_version, current.kernel_version),
            tone: ChangeTone::Neutral,
        });
    }

    compare_optional_count(
        &mut changes,
        "Failed system services",
        previous.failed_system_units,
        current.failed_system_units,
    );
    compare_optional_count(
        &mut changes,
        "Failed user services",
        previous.failed_user_units,
        current.failed_user_units,
    );
    compare_optional_count(
        &mut changes,
        "Network-bound listeners",
        previous.network_bound_ports,
        current.network_bound_ports,
    );

    if let (Some(before), Some(after)) = (previous.firewall_active, current.firewall_active) {
        if before != after {
            changes.push(SystemChange {
                title: "Firewall state changed".to_string(),
                detail: if after {
                    "UFW is now active".to_string()
                } else {
                    "UFW is now inactive".to_string()
                },
                tone: if after {
                    ChangeTone::Positive
                } else {
                    ChangeTone::Attention
                },
            });
        }
    }

    changes
}

fn compare_scans(previous: &ScanSnapshot, current: &ScanSnapshot) -> Vec<SystemChange> {
    let mut changes = Vec::new();
    let total_delta = signed_delta(
        current.total_reclaimable_bytes,
        previous.total_reclaimable_bytes,
    );
    if total_delta.unsigned_abs() >= 32 * 1024 * 1024 {
        changes.push(SystemChange {
            title: if total_delta > 0 {
                "Safe reclaimable space grew".to_string()
            } else {
                "Safe reclaimable space fell".to_string()
            },
            detail: format_delta_bytes(total_delta),
            tone: if total_delta > 0 {
                ChangeTone::Attention
            } else {
                ChangeTone::Positive
            },
        });
    }

    for (category, current_bytes) in &current.categories {
        let previous_bytes = previous.categories.get(category).copied().unwrap_or(0);
        let delta = signed_delta(*current_bytes, previous_bytes);
        if delta.unsigned_abs() < 64 * 1024 * 1024 {
            continue;
        }
        changes.push(SystemChange {
            title: format!("{category} changed"),
            detail: format_delta_bytes(delta),
            tone: if delta > 0 {
                ChangeTone::Attention
            } else {
                ChangeTone::Positive
            },
        });
    }

    changes.sort_by_key(|change| match change.tone {
        ChangeTone::Attention => 0,
        ChangeTone::Positive => 1,
        ChangeTone::Neutral => 2,
    });
    changes
}

fn compare_optional_count(
    changes: &mut Vec<SystemChange>,
    title: &str,
    before: Option<u32>,
    after: Option<u32>,
) {
    let (Some(before), Some(after)) = (before, after) else {
        return;
    };
    if before == after {
        return;
    }
    changes.push(SystemChange {
        title: title.to_string(),
        detail: format!("{before} → {after}"),
        tone: if after > before {
            ChangeTone::Attention
        } else {
            ChangeTone::Positive
        },
    });
}

fn maybe_record_system_snapshot(
    home: &Path,
    history: &mut Vec<SystemSnapshot>,
    current: SystemSnapshot,
) {
    let should_record = history
        .last()
        .map(|last| {
            current.timestamp_unix.saturating_sub(last.timestamp_unix)
                >= SYSTEM_SNAPSHOT_INTERVAL_SECS
        })
        .unwrap_or(true);

    if !should_record {
        return;
    }

    history.push(current);
    if history.len() > MAX_SYSTEM_SNAPSHOTS {
        let drain = history.len() - MAX_SYSTEM_SNAPSHOTS;
        history.drain(0..drain);
    }
    let _ = save_json_atomic(&system_snapshot_path(home), history);
}

fn select_week_baseline(history: &[SystemSnapshot], now: u64) -> Option<&SystemSnapshot> {
    if history.is_empty() {
        return None;
    }

    let target = now.saturating_sub(WEEK_SECS);
    history
        .iter()
        .filter(|snapshot| snapshot.timestamp_unix <= target)
        .max_by_key(|snapshot| snapshot.timestamp_unix)
        .or_else(|| history.first())
}

fn failed_units(user_scope: bool) -> Option<u32> {
    let mut command = Command::new("systemctl");
    if user_scope {
        command.arg("--user");
    }
    let output = command
        .args(["--failed", "--type=service", "--no-legend", "--no-pager"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.lines().filter(|line| !line.trim().is_empty()).count() as u32)
}

fn firewall_state() -> Option<bool> {
    let output = Command::new("ufw").arg("status").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
    if text.contains("status: active") {
        Some(true)
    } else if text.contains("status: inactive") {
        Some(false)
    } else {
        None
    }
}

fn network_bound_port_count() -> Option<u32> {
    let output = Command::new("ss").args(["-tulnH"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let count = text
        .lines()
        .filter_map(|line| line.split_whitespace().nth(4))
        .filter(|addr| !is_loopback_bind(addr))
        .count();
    Some(count as u32)
}

fn is_loopback_bind(addr: &str) -> bool {
    addr.starts_with("127.")
        || addr.starts_with("[::1]")
        || addr.starts_with("::1:")
        || addr.starts_with("localhost:")
}

fn ratio(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64
    }
}

fn signed_delta(current: u64, previous: u64) -> i128 {
    current as i128 - previous as i128
}

fn format_delta_bytes(delta: i128) -> String {
    let magnitude = delta.unsigned_abs().min(u64::MAX as u128) as u64;
    if delta >= 0 {
        format!("+{}", format::bytes(magnitude))
    } else {
        format!("−{}", format::bytes(magnitude))
    }
}

fn grade_for(score: u8) -> &'static str {
    match score {
        90..=100 => "Excellent",
        75..=89 => "Good",
        55..=74 => "Needs attention",
        _ => "Critical",
    }
}

fn summary_for(score: u8) -> &'static str {
    match score {
        90..=100 => "No major health pressure detected from the signals LinuxCare can verify.",
        75..=89 => "The system is generally healthy, with a few items worth reviewing.",
        55..=74 => "Multiple verified signals need attention before they become disruptive.",
        _ => "LinuxCare detected significant pressure or failures that should be reviewed soon.",
    }
}

fn age_label(now: u64, then: u64) -> String {
    let age = now.saturating_sub(then);
    if age >= 6 * 24 * 60 * 60 {
        let days = (age as f64 / 86_400.0).round() as u64;
        format!("Compared with ~{days} days ago")
    } else if age >= 60 * 60 {
        let hours = (age as f64 / 3_600.0).round() as u64;
        format!("Compared with ~{hours} hours ago")
    } else {
        let minutes = (age / 60).max(1);
        format!("Compared with ~{minutes} minutes ago")
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn state_root(home: &Path) -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"))
        .join("linuxcare")
}

fn system_snapshot_path(home: &Path) -> PathBuf {
    state_root(home).join("health-snapshots.json")
}

fn scan_snapshot_path(home: &Path) -> PathBuf {
    state_root(home).join("scan-snapshots.json")
}

fn load_json<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    let data = fs::read(path)?;
    serde_json::from_slice(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn save_json_atomic<T: Serialize + ?Sized>(path: &Path, value: &T) -> io::Result<()> {
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
    let tmp = dir.join(format!(".linuxcare-{}-{nonce}.tmp", std::process::id()));
    let payload = serde_json::to_vec_pretty(value)
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

    fn snapshot() -> SystemSnapshot {
        SystemSnapshot {
            timestamp_unix: 100,
            kernel_version: "6.0-test".to_string(),
            disk_root_used_bytes: 50,
            disk_root_total_bytes: 100,
            disk_home_used_bytes: 50,
            disk_home_total_bytes: 100,
            ram_used_bytes: 50,
            ram_total_bytes: 100,
            failed_system_units: Some(0),
            failed_user_units: Some(0),
            firewall_active: Some(true),
            network_bound_ports: Some(2),
        }
    }

    #[test]
    fn healthy_snapshot_scores_high() {
        let (score, _) = score_snapshot(&snapshot());
        assert_eq!(score, 100);
    }

    #[test]
    fn pressure_reduces_score() {
        let mut s = snapshot();
        s.disk_root_used_bytes = 96;
        s.ram_used_bytes = 96;
        s.failed_system_units = Some(2);
        s.firewall_active = Some(false);
        let (score, _) = score_snapshot(&s);
        assert!(score < 60);
    }

    #[test]
    fn kernel_change_is_reported() {
        let before = snapshot();
        let mut after = before.clone();
        after.kernel_version = "6.1-test".to_string();
        let changes = compare_system(&before, &after);
        assert!(changes
            .iter()
            .any(|change| change.title == "Kernel changed"));
    }
}
