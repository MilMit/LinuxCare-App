use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Good,
    Review,
    Critical,
    Unknown,
}

impl ServiceState {
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
pub struct ServiceSignal {
    pub title: String,
    pub detail: String,
    pub state: ServiceState,
}

#[derive(Debug, Clone, Default)]
pub struct ServiceDoctorReport {
    pub system_scope_available: bool,
    pub user_scope_available: bool,
    pub failed_system: Vec<String>,
    pub failed_user: Vec<String>,
    pub restarting_system: Vec<String>,
    pub restarting_user: Vec<String>,
    pub enabled_system_count: Option<usize>,
    pub enabled_user_count: Option<usize>,
    pub masked_system_count: Option<usize>,
    pub signals: Vec<ServiceSignal>,
    pub status_label: String,
    pub summary: String,
}

pub fn collect_report() -> ServiceDoctorReport {
    let system_units = collect_unit_states(false);
    let user_units = collect_unit_states(true);
    let system_scope_available = system_units.is_some();
    let user_scope_available = user_units.is_some();
    let failed_system = system_units
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|unit| unit.active == "failed")
        .map(|unit| unit.name.clone())
        .collect::<Vec<_>>();
    let failed_user = user_units
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|unit| unit.active == "failed")
        .map(|unit| unit.name.clone())
        .collect::<Vec<_>>();
    let restarting_system = system_units
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|unit| unit.substate == "auto-restart" || unit.active == "activating")
        .map(|unit| unit.name.clone())
        .collect::<Vec<_>>();
    let restarting_user = user_units
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|unit| unit.substate == "auto-restart" || unit.active == "activating")
        .map(|unit| unit.name.clone())
        .collect::<Vec<_>>();

    let enabled_system_count = count_unit_files(false, "enabled");
    let enabled_user_count = count_unit_files(true, "enabled");
    let masked_system_count = count_unit_files(false, "masked");

    let mut signals = Vec::new();
    push_failure_signal(
        &mut signals,
        "System services",
        system_scope_available,
        &failed_system,
        "No failed system service units were detected.",
    );
    push_failure_signal(
        &mut signals,
        "User services",
        user_scope_available,
        &failed_user,
        "No failed user service units were detected.",
    );
    push_restart_signal(
        &mut signals,
        "System restart loops",
        system_scope_available,
        &restarting_system,
    );
    push_restart_signal(
        &mut signals,
        "User restart loops",
        user_scope_available,
        &restarting_user,
    );

    signals.push(match enabled_system_count {
        Some(count) => ServiceSignal {
            title: "Enabled system services".to_string(),
            detail: format!(
                "{count} system service unit file(s) are enabled. Enabled does not mean unnecessary; LinuxCare does not recommend disabling a service from count alone."
            ),
            state: ServiceState::Good,
        },
        None => ServiceSignal {
            title: "Enabled system services".to_string(),
            detail: "systemctl could not enumerate enabled system service unit files.".to_string(),
            state: ServiceState::Unknown,
        },
    });

    let critical = signals
        .iter()
        .filter(|signal| signal.state == ServiceState::Critical)
        .count();
    let review = signals
        .iter()
        .filter(|signal| signal.state == ServiceState::Review)
        .count();

    let (status_label, summary) = if critical > 0 {
        (
            "Unstable".to_string(),
            "One or more service units appear to be failing or repeatedly restarting. Review the affected unit and its journal before changing startup policy.".to_string(),
        )
    } else if review > 0 {
        (
            "Needs review".to_string(),
            "Service Doctor found state worth reviewing. Diagnosis is read-only and does not disable system services automatically.".to_string(),
        )
    } else if !system_scope_available || !user_scope_available {
        (
            "Partially checked".to_string(),
            "One or more systemd scopes could not be enumerated. Available scopes show no failure/restart-loop signal, but LinuxCare will not label unavailable scopes healthy.".to_string(),
        )
    } else {
        (
            "Healthy".to_string(),
            "No failed or auto-restarting service units were detected in the available systemd state.".to_string(),
        )
    };

    ServiceDoctorReport {
        system_scope_available,
        user_scope_available,
        failed_system,
        failed_user,
        restarting_system,
        restarting_user,
        enabled_system_count,
        enabled_user_count,
        masked_system_count,
        signals,
        status_label,
        summary,
    }
}

#[derive(Debug, Clone)]
struct UnitState {
    name: String,
    active: String,
    substate: String,
}

fn collect_unit_states(user: bool) -> Option<Vec<UnitState>> {
    let mut command = Command::new("systemctl");
    if user {
        command.arg("--user");
    }
    let Ok(output) = command
        .args([
            "list-units",
            "--type=service",
            "--all",
            "--no-legend",
            "--plain",
            "--no-pager",
        ])
        .output()
    else {
        return None;
    };
    if !output.status.success() {
        return None;
    }

    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let parts = line.split_whitespace().collect::<Vec<_>>();
                if parts.len() < 4 {
                    return None;
                }
                Some(UnitState {
                    name: parts[0].to_string(),
                    active: parts[2].to_string(),
                    substate: parts[3].to_string(),
                })
            })
            .collect(),
    )
}

fn count_unit_files(user: bool, state: &str) -> Option<usize> {
    let mut command = Command::new("systemctl");
    if user {
        command.arg("--user");
    }
    let state_arg = format!("--state={state}");
    let output = command
        .args([
            "list-unit-files",
            "--type=service",
            "--no-legend",
            "--no-pager",
            state_arg.as_str(),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count(),
    )
}

fn push_failure_signal(
    signals: &mut Vec<ServiceSignal>,
    title: &str,
    available: bool,
    units: &[String],
    healthy_detail: &str,
) {
    if !available {
        signals.push(ServiceSignal {
            title: title.to_string(),
            detail: "This systemd scope could not be enumerated; LinuxCare does not infer a healthy state.".to_string(),
            state: ServiceState::Unknown,
        });
        return;
    }
    if units.is_empty() {
        signals.push(ServiceSignal {
            title: title.to_string(),
            detail: healthy_detail.to_string(),
            state: ServiceState::Good,
        });
        return;
    }
    signals.push(ServiceSignal {
        title: title.to_string(),
        detail: format!("{} failed unit(s): {}", units.len(), preview(units, 5)),
        state: if units.len() >= 3 {
            ServiceState::Critical
        } else {
            ServiceState::Review
        },
    });
}

fn push_restart_signal(
    signals: &mut Vec<ServiceSignal>,
    title: &str,
    available: bool,
    units: &[String],
) {
    if !available {
        signals.push(ServiceSignal {
            title: title.to_string(),
            detail: "This systemd scope could not be enumerated; restart-loop state is unknown."
                .to_string(),
            state: ServiceState::Unknown,
        });
        return;
    }
    if units.is_empty() {
        signals.push(ServiceSignal {
            title: title.to_string(),
            detail: "No service unit currently reports auto-restart/activating state.".to_string(),
            state: ServiceState::Good,
        });
    } else {
        signals.push(ServiceSignal {
            title: title.to_string(),
            detail: format!(
                "{} unit(s) may be repeatedly restarting: {}",
                units.len(),
                preview(units, 5)
            ),
            state: ServiceState::Critical,
        });
    }
}

fn preview(items: &[String], limit: usize) -> String {
    let mut value = items
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > limit {
        value.push_str(&format!(" +{} more", items.len() - limit));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_limits_output() {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(preview(&items, 2), "a, b +1 more");
    }
}
