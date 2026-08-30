use crate::{
    battery, boot, filesystem_doctor, network_doctor, package_doctor, quarantine, service_doctor,
    storage_runway, thermal_doctor, timeline,
};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecommendationPriority {
    Critical,
    High,
    Medium,
    Low,
}

impl RecommendationPriority {
    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "CRITICAL",
            Self::High => "HIGH",
            Self::Medium => "MEDIUM",
            Self::Low => "LOW",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::Critical => "risk-dangerous",
            Self::High => "risk-advanced",
            Self::Medium => "risk-review",
            Self::Low => "risk-safe",
        }
    }

    fn sort_key(self) -> u8 {
        match self {
            Self::Critical => 0,
            Self::High => 1,
            Self::Medium => 2,
            Self::Low => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Recommendation {
    pub priority: RecommendationPriority,
    pub area: String,
    pub title: String,
    pub why: String,
    pub suggested_action: String,
    pub reversible: bool,
    pub requires_privilege: bool,
}

#[derive(Debug, Clone)]
pub struct MaintenanceIntelligenceReport {
    pub recommendations: Vec<Recommendation>,
    pub summary: String,
    pub checked_areas: usize,
    pub recent_timeline_events: usize,
    pub quarantine_bytes: u64,
}

pub fn collect_report(home: &Path) -> MaintenanceIntelligenceReport {
    let services = service_doctor::collect_report();
    let packages = package_doctor::collect_report();
    let network = network_doctor::collect_report();
    let filesystems = filesystem_doctor::collect_report();
    let thermal = thermal_doctor::collect_report();
    let batteries = battery::collect_report_with_history(home);
    let boot = boot::collect_report();
    let storage = storage_runway::collect_report(home);
    let quarantine_bytes = quarantine::total_protected_bytes(home);
    let recent_timeline_events = timeline::load(home)
        .map(|events| events.len().min(20))
        .unwrap_or(0);

    let mut recommendations = Vec::new();
    storage_recommendations(&storage, &mut recommendations);
    service_recommendations(&services, &mut recommendations);
    package_recommendations(&packages, &mut recommendations);
    network_recommendations(&network, &mut recommendations);
    filesystem_recommendations(&filesystems, &mut recommendations);
    thermal_recommendations(&thermal, &mut recommendations);
    battery_recommendations(&batteries, &mut recommendations);
    boot_recommendations(&boot, &mut recommendations);

    if quarantine_bytes >= 1024 * 1024 * 1024 {
        recommendations.push(Recommendation {
            priority: RecommendationPriority::Low,
            area: "Safety".to_string(),
            title: "Undo-protected cleanup data is using disk space".to_string(),
            why: format!(
                "Safety Quarantine currently protects {}. This is intentionally not counted as freed space until purge.",
                crate::format::bytes(quarantine_bytes)
            ),
            suggested_action: "Open Safety & Timeline. Keep the protected data while you may need Undo; purge only after you are satisfied the cleanup was safe.".to_string(),
            reversible: false,
            requires_privilege: false,
        });
    }

    recommendations.sort_by_key(|recommendation| recommendation.priority.sort_key());
    recommendations.truncate(14);

    let summary = if recommendations.is_empty() {
        "No actionable maintenance recommendation crossed LinuxCare's conservative thresholds. Unknown checks are not treated as healthy or unhealthy.".to_string()
    } else {
        let critical = recommendations
            .iter()
            .filter(|item| item.priority == RecommendationPriority::Critical)
            .count();
        let high = recommendations
            .iter()
            .filter(|item| item.priority == RecommendationPriority::High)
            .count();
        format!(
            "{} recommendation(s) are currently actionable: {critical} critical and {high} high priority. LinuxCare explains why before suggesting an action.",
            recommendations.len()
        )
    };

    MaintenanceIntelligenceReport {
        recommendations,
        summary,
        checked_areas: 8,
        recent_timeline_events,
        quarantine_bytes,
    }
}

fn storage_recommendations(
    storage: &storage_runway::StorageRunwayReport,
    out: &mut Vec<Recommendation>,
) {
    if storage.used_percent >= 95.0 {
        out.push(Recommendation {
            priority: RecommendationPriority::Critical,
            area: "Storage".to_string(),
            title: "Root filesystem is critically full".to_string(),
            why: format!("Root storage is {:.0}% used.", storage.used_percent),
            suggested_action: "Review Storage Analyzer and Smart Cleaner now. Prefer safe user-space cleanup before privileged package/log cleanup.".to_string(),
            reversible: true,
            requires_privilege: false,
        });
        return;
    }
    if storage.used_percent >= 90.0 {
        out.push(Recommendation {
            priority: RecommendationPriority::High,
            area: "Storage".to_string(),
            title: "Root filesystem has high capacity pressure".to_string(),
            why: format!("Root storage is {:.0}% used.", storage.used_percent),
            suggested_action: "Review the largest storage categories and reclaim space before updates or applications encounter ENOSPC failures.".to_string(),
            reversible: true,
            requires_privilege: false,
        });
    }
    if let Some(days) = storage
        .targets
        .iter()
        .find(|target| target.percent == 90)
        .and_then(|target| target.days)
    {
        if days <= 14.0 && storage.used_percent < 90.0 {
            out.push(Recommendation {
                priority: RecommendationPriority::High,
                area: "Storage".to_string(),
                title: "Storage trend may reach 90% soon".to_string(),
                why: format!(
                    "The current runway trend estimates 90% usage in {}.",
                    storage_runway::format_days(Some(days))
                ),
                suggested_action: "Use What Changed and Storage Analyzer to identify the growth source instead of repeatedly deleting random cache.".to_string(),
                reversible: true,
                requires_privilege: false,
            });
        }
    }
}

fn service_recommendations(
    services: &service_doctor::ServiceDoctorReport,
    out: &mut Vec<Recommendation>,
) {
    if !services.restarting_system.is_empty() || !services.restarting_user.is_empty() {
        out.push(Recommendation {
            priority: RecommendationPriority::Critical,
            area: "Services".to_string(),
            title: "A service may be stuck in a restart loop".to_string(),
            why: format!(
                "{} system and {} user service unit(s) currently show auto-restart/activating state.",
                services.restarting_system.len(),
                services.restarting_user.len()
            ),
            suggested_action: "Inspect the affected unit status and journal first. Do not disable an unfamiliar dependency merely to hide the failure.".to_string(),
            reversible: true,
            requires_privilege: true,
        });
    }
    let failed = services.failed_system.len() + services.failed_user.len();
    if failed > 0 {
        out.push(Recommendation {
            priority: RecommendationPriority::High,
            area: "Services".to_string(),
            title: "Failed service units need diagnosis".to_string(),
            why: format!("{failed} failed system/user service unit(s) are visible to systemd."),
            suggested_action: "Open Service Doctor and inspect the service journal and dependency chain before restarting or changing startup policy.".to_string(),
            reversible: true,
            requires_privilege: !services.failed_system.is_empty(),
        });
    }
}

fn package_recommendations(
    packages: &package_doctor::PackageDoctorReport,
    out: &mut Vec<Recommendation>,
) {
    if !packages.duplicate_source_entries.is_empty() {
        out.push(Recommendation {
            priority: RecommendationPriority::Medium,
            area: "Packages".to_string(),
            title: "Duplicate APT source definitions detected".to_string(),
            why: format!(
                "{} exact duplicate active repository definition(s) were found.",
                packages.duplicate_source_entries.len()
            ),
            suggested_action: "Review source files and remove only confirmed duplicates. LinuxCare does not edit repository configuration automatically.".to_string(),
            reversible: true,
            requires_privilege: true,
        });
    }
    if let Some(days) = packages.apt_metadata_age_days {
        if days > 14 {
            out.push(Recommendation {
                priority: RecommendationPriority::Low,
                area: "Packages".to_string(),
                title: "APT metadata may be stale".to_string(),
                why: format!("The newest cached repository metadata is about {days} day(s) old."),
                suggested_action: "Refresh package metadata when you are ready. LinuxCare does not perform networked apt update in the background.".to_string(),
                reversible: false,
                requires_privilege: true,
            });
        }
    }
    if packages.signals.iter().any(|signal| {
        signal.state == package_doctor::DiagnosticState::Review
            && signal.title.contains("dependency")
    }) {
        out.push(Recommendation {
            priority: RecommendationPriority::High,
            area: "Packages".to_string(),
            title: "APT dependency state needs repair review".to_string(),
            why: "apt-get check reported package dependency state that is not clean.".to_string(),
            suggested_action: "Review the exact APT error before attempting any fix-broken operation. LinuxCare does not execute repair commands automatically.".to_string(),
            reversible: false,
            requires_privilege: true,
        });
    }
}

fn network_recommendations(
    network: &network_doctor::NetworkDoctorReport,
    out: &mut Vec<Recommendation>,
) {
    match network.connectivity.as_deref() {
        Some("none") => out.push(Recommendation {
            priority: RecommendationPriority::High,
            area: "Network".to_string(),
            title: "NetworkManager reports no connectivity".to_string(),
            why: "The local NetworkManager connectivity state is 'none'.".to_string(),
            suggested_action: "Check link state, default route and DNS configuration in Network Doctor before changing firewall rules.".to_string(),
            reversible: true,
            requires_privilege: false,
        }),
        Some("limited" | "portal") => out.push(Recommendation {
            priority: RecommendationPriority::Medium,
            area: "Network".to_string(),
            title: "Connectivity is not fully confirmed".to_string(),
            why: format!(
                "NetworkManager reports {}.",
                network_doctor::connectivity_label(network.connectivity.as_deref())
            ),
            suggested_action: "Review the default route and DNS state. Run the explicit Active Probe only if you want LinuxCare to make a DNS query and ping the local gateway.".to_string(),
            reversible: true,
            requires_privilege: false,
        }),
        _ => {}
    }
    if network.default_routes.is_empty() {
        out.push(Recommendation {
            priority: RecommendationPriority::High,
            area: "Network".to_string(),
            title: "No default route detected".to_string(),
            why: "Neither IPv4 nor IPv6 routing exposes a default route.".to_string(),
            suggested_action:
                "Check the active connection, DHCP/static route configuration, or VPN policy."
                    .to_string(),
            reversible: true,
            requires_privilege: false,
        });
    }
    if network
        .interface_health
        .iter()
        .any(|interface| interface.error_total() > 100)
    {
        out.push(Recommendation {
            priority: RecommendationPriority::Medium,
            area: "Network".to_string(),
            title: "Network interface error/drop counters are elevated".to_string(),
            why: "At least one active/relevant interface has more than 100 cumulative RX/TX errors or drops.".to_string(),
            suggested_action: "Compare counters over time before blaming hardware. Rising counts can indicate Wi-Fi quality, driver, duplex, queue, or MTU problems.".to_string(),
            reversible: true,
            requires_privilege: false,
        });
    }
}

fn filesystem_recommendations(
    filesystems: &filesystem_doctor::FilesystemDoctorReport,
    out: &mut Vec<Recommendation>,
) {
    if !filesystems.kernel_error_lines.is_empty() {
        out.push(Recommendation {
            priority: RecommendationPriority::Critical,
            area: "Filesystem".to_string(),
            title: "Filesystem-related kernel errors were detected this boot".to_string(),
            why: format!(
                "{} matching error line(s) were found in the current boot kernel journal.",
                filesystems.kernel_error_lines.len()
            ),
            suggested_action: "Back up important data before attempting repair. Identify the filesystem/device first; do not run fsck on a mounted filesystem blindly.".to_string(),
            reversible: false,
            requires_privilege: true,
        });
    }
    if let Some(mount) = filesystems
        .mounts
        .iter()
        .find(|mount| mount.state == filesystem_doctor::FilesystemState::Critical)
    {
        out.push(Recommendation {
            priority: RecommendationPriority::High,
            area: "Filesystem".to_string(),
            title: format!("Filesystem pressure on {}", mount.mount_point.display()),
            why: mount.detail.clone(),
            suggested_action: "Reclaim capacity or inodes first. If the filesystem unexpectedly became read-only, inspect kernel/storage errors before remounting it read-write.".to_string(),
            reversible: true,
            requires_privilege: false,
        });
    }
}

fn thermal_recommendations(
    thermal: &thermal_doctor::ThermalDoctorReport,
    out: &mut Vec<Recommendation>,
) {
    if thermal
        .sensors
        .iter()
        .any(|sensor| sensor.state == thermal_doctor::ThermalState::Critical)
    {
        out.push(Recommendation {
            priority: RecommendationPriority::Critical,
            area: "Thermal".to_string(),
            title: "Critical thermal pressure is visible".to_string(),
            why: format!(
                "The hottest exposed sensor is {:.1}°C.",
                thermal.max_temperature_c.unwrap_or_default()
            ),
            suggested_action: "Reduce sustained load and verify airflow/cooling. If temperature remains high at idle, inspect fans, dust, thermal interface and firmware.".to_string(),
            reversible: true,
            requires_privilege: false,
        });
    } else if thermal.cpu.throttle_events.unwrap_or(0) > 0 {
        out.push(Recommendation {
            priority: RecommendationPriority::Medium,
            area: "Thermal".to_string(),
            title: "CPU throttle events have occurred".to_string(),
            why: format!(
                "The available kernel counters expose {} throttle event(s) since boot.",
                thermal.cpu.throttle_events.unwrap_or(0)
            ),
            suggested_action: "Compare temperature and frequency under load. A historical throttle count is evidence to investigate, not proof of a current cooling failure.".to_string(),
            reversible: true,
            requires_privilege: false,
        });
    }
}

fn battery_recommendations(batteries: &battery::BatteryLabReport, out: &mut Vec<Recommendation>) {
    for battery in &batteries.batteries {
        if let Some(health) = battery.health_percent {
            if health < 70.0 {
                out.push(Recommendation {
                    priority: RecommendationPriority::Medium,
                    area: "Battery".to_string(),
                    title: format!("{} has high capacity wear", battery.display_name()),
                    why: format!("Current full capacity is approximately {health:.0}% of design capacity."),
                    suggested_action: "Treat this as a capacity-wear estimate. If runtime is poor, consider battery replacement; LinuxCare does not alter firmware charge thresholds automatically.".to_string(),
                    reversible: false,
                    requires_privilege: false,
                });
            }
        }
    }
    for trend in &batteries.trends {
        if trend.span_hours >= 72.0
            && trend
                .health_delta_percent
                .is_some_and(|delta| delta <= -3.0)
        {
            out.push(Recommendation {
                priority: RecommendationPriority::Low,
                area: "Battery".to_string(),
                title: format!("{} health estimate changed noticeably", trend.name),
                why: format!(
                    "The recorded full/design capacity ratio changed by {:.1} percentage points across {:.0} hours. Battery gauges can recalibrate, so this is not treated as confirmed physical wear by itself.",
                    trend.health_delta_percent.unwrap_or_default(),
                    trend.span_hours
                ),
                suggested_action: "Keep collecting samples and compare runtime. Avoid making a replacement decision from a single capacity recalibration.".to_string(),
                reversible: false,
                requires_privilege: false,
            });
        }
    }
}

fn boot_recommendations(boot: &boot::BootReport, out: &mut Vec<Recommendation>) {
    if let Some(total) = boot.total_secs {
        if total >= 60.0 {
            out.push(Recommendation {
                priority: RecommendationPriority::Medium,
                area: "Boot".to_string(),
                title: "Boot time is materially high".to_string(),
                why: format!("systemd-analyze reports a total boot time of {total:.1}s."),
                suggested_action: "Use Boot Doctor to inspect blame and critical-chain. Do not disable a service solely because it appears near the top of blame.".to_string(),
                reversible: true,
                requires_privilege: false,
            });
        }
    }
}
