use std::process::Command;

#[derive(Debug, Clone)]
pub struct BootUnit {
    pub name: String,
    pub duration_secs: f64,
}

#[derive(Debug, Clone)]
pub struct BootReport {
    pub total_secs: Option<f64>,
    pub firmware_secs: Option<f64>,
    pub loader_secs: Option<f64>,
    pub kernel_secs: Option<f64>,
    pub userspace_secs: Option<f64>,
    pub graphical_secs: Option<f64>,
    pub slow_units: Vec<BootUnit>,
    pub failed_units: Vec<String>,
    pub critical_chain: Vec<String>,
    pub status_label: String,
    pub summary: String,
    pub recommendation: String,
}

pub fn collect_report() -> BootReport {
    let time_text = command_output("systemd-analyze", &["time", "--no-pager"]);
    let blame_text = command_output("systemd-analyze", &["blame", "--no-pager"]);
    let critical_text = command_output("systemd-analyze", &["critical-chain", "--no-pager"]);
    let failed_text = command_output(
        "systemctl",
        &[
            "--failed",
            "--type=service",
            "--no-legend",
            "--plain",
            "--no-pager",
        ],
    );

    let phases = time_text
        .as_deref()
        .map(parse_boot_time)
        .unwrap_or_default();
    let slow_units = blame_text.as_deref().map(parse_blame).unwrap_or_default();
    let failed_units = failed_text
        .as_deref()
        .map(parse_failed_units)
        .unwrap_or_default();
    let critical_chain = critical_text
        .as_deref()
        .map(parse_critical_chain)
        .unwrap_or_default();

    let (status_label, summary) = match phases.total_secs {
        Some(total) if total < 15.0 => (
            "Fast".to_string(),
            format!("Boot completed in {total:.1}s. No broad startup delay is visible."),
        ),
        Some(total) if total < 30.0 => (
            "Normal".to_string(),
            format!("Boot completed in {total:.1}s. A few units may still be worth reviewing."),
        ),
        Some(total) if total < 60.0 => (
            "Slow".to_string(),
            format!("Boot took {total:.1}s. LinuxCare found enough delay to justify a startup review."),
        ),
        Some(total) => (
            "Very slow".to_string(),
            format!("Boot took {total:.1}s. Startup latency is materially high and should be investigated."),
        ),
        None => (
            "Unavailable".to_string(),
            "systemd-analyze did not return a usable boot duration on this system.".to_string(),
        ),
    };

    let recommendation = recommendation_for(&slow_units, &failed_units);

    BootReport {
        total_secs: phases.total_secs,
        firmware_secs: phases.firmware_secs,
        loader_secs: phases.loader_secs,
        kernel_secs: phases.kernel_secs,
        userspace_secs: phases.userspace_secs,
        graphical_secs: phases.graphical_secs,
        slow_units,
        failed_units,
        critical_chain,
        status_label,
        summary,
        recommendation,
    }
}

#[derive(Debug, Clone, Default)]
struct BootPhases {
    total_secs: Option<f64>,
    firmware_secs: Option<f64>,
    loader_secs: Option<f64>,
    kernel_secs: Option<f64>,
    userspace_secs: Option<f64>,
    graphical_secs: Option<f64>,
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .env("LC_ALL", "C")
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_boot_time(text: &str) -> BootPhases {
    let mut phases = BootPhases::default();

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Startup finished in ") {
            if let Some((parts, total)) = rest.split_once(" = ") {
                phases.total_secs = parse_duration_phrase(total.trim());
                for part in parts.split(" + ") {
                    if let Some((duration, label)) = split_duration_label(part) {
                        match label {
                            "firmware" => phases.firmware_secs = parse_duration_phrase(duration),
                            "loader" => phases.loader_secs = parse_duration_phrase(duration),
                            "kernel" => phases.kernel_secs = parse_duration_phrase(duration),
                            "userspace" => phases.userspace_secs = parse_duration_phrase(duration),
                            _ => {}
                        }
                    }
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("graphical.target reached after ") {
            let duration = rest
                .split_once(" in userspace")
                .map(|(value, _)| value)
                .unwrap_or(rest);
            phases.graphical_secs = parse_duration_phrase(duration);
        }
    }

    phases
}

fn split_duration_label(part: &str) -> Option<(&str, &str)> {
    let open = part.rfind('(')?;
    let close = part.rfind(')')?;
    if close <= open {
        return None;
    }
    Some((part[..open].trim(), part[open + 1..close].trim()))
}

fn parse_duration_phrase(input: &str) -> Option<f64> {
    let mut token = input
        .trim()
        .trim_end_matches('.')
        .trim_end_matches(',')
        .to_string();
    if token.is_empty() {
        return None;
    }

    let mut total = 0.0;
    if let Some(min_pos) = token.find("min") {
        let mins = token[..min_pos].trim().parse::<f64>().ok()?;
        total += mins * 60.0;
        token = token[min_pos + 3..].trim().to_string();
    }

    if token.is_empty() {
        return Some(total);
    }

    let value = if let Some(v) = token.strip_suffix("ms") {
        v.trim().parse::<f64>().ok()? / 1_000.0
    } else if let Some(v) = token.strip_suffix("us") {
        v.trim().parse::<f64>().ok()? / 1_000_000.0
    } else if let Some(v) = token.strip_suffix("µs") {
        v.trim().parse::<f64>().ok()? / 1_000_000.0
    } else if let Some(v) = token.strip_suffix('s') {
        v.trim().parse::<f64>().ok()?
    } else {
        token.parse::<f64>().ok()?
    };

    Some(total + value)
}

fn parse_blame(text: &str) -> Vec<BootUnit> {
    let mut units = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let mut parts = trimmed.split_whitespace();
        let duration = parts.next().and_then(parse_duration_phrase);
        let name = parts.next();
        if let (Some(duration_secs), Some(name)) = (duration, name) {
            units.push(BootUnit {
                name: name.to_string(),
                duration_secs,
            });
        }
    }
    units.sort_by(|a, b| {
        b.duration_secs
            .partial_cmp(&a.duration_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    units.truncate(12);
    units
}

fn parse_failed_units(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .take(12)
        .collect()
}

fn parse_critical_chain(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("The time when unit became active"))
        .filter(|line| !line.starts_with("The time the unit took"))
        .map(|line| {
            line.trim_start_matches(|c: char| {
                matches!(c, '└' | '─' | '●' | '@' | ' ' | '┌' | '├' | '│')
            })
            .to_string()
        })
        .filter(|line| !line.is_empty())
        .take(12)
        .collect()
}

fn recommendation_for(slow_units: &[BootUnit], failed_units: &[String]) -> String {
    if !failed_units.is_empty() {
        return format!(
            "{} failed service(s) are present. Review those failures before disabling startup units; a failed dependency can make boot slower or unreliable.",
            failed_units.len()
        );
    }

    if let Some(unit) = slow_units.first() {
        if unit.name.contains("NetworkManager-wait-online")
            || unit.name.contains("systemd-networkd-wait-online")
        {
            return format!(
                "{} is the slowest unit at {:.1}s. Wait-online services may be required by workloads that depend on network-online.target, so LinuxCare recommends dependency review instead of blind disabling.",
                unit.name, unit.duration_secs
            );
        }
        if unit.duration_secs >= 5.0 {
            return format!(
                "{} is the largest startup contributor at {:.1}s. Inspect why it starts at boot and whether it is actually required for every session.",
                unit.name, unit.duration_secs
            );
        }
    }

    "No single high-confidence startup bottleneck stands out. Keep the current boot configuration unless a service has a clear functional reason to change.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_parser_handles_common_systemd_units() {
        assert!((parse_duration_phrase("6.5s").unwrap() - 6.5).abs() < 0.001);
        assert!((parse_duration_phrase("320ms").unwrap() - 0.32).abs() < 0.001);
        assert!((parse_duration_phrase("1min 2.5s").unwrap() - 62.5).abs() < 0.001);
    }

    #[test]
    fn blame_parser_orders_slowest_first() {
        let parsed = parse_blame("1.2s a.service\n5.0s b.service\n");
        assert_eq!(parsed[0].name, "b.service");
        assert_eq!(parsed[1].name, "a.service");
    }
}
