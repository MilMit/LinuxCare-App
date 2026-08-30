use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticState {
    Good,
    Review,
    Unknown,
}

impl DiagnosticState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Good => "GOOD",
            Self::Review => "REVIEW",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::Good => "risk-safe",
            Self::Review => "risk-review",
            Self::Unknown => "health-neutral",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiagnosticSignal {
    pub title: String,
    pub detail: String,
    pub state: DiagnosticState,
}

#[derive(Debug, Clone)]
pub struct PackageDoctorReport {
    pub overall_label: String,
    pub summary: String,
    pub signals: Vec<DiagnosticSignal>,
    pub held_packages: Vec<String>,
    pub autoremove_candidates: Vec<String>,
    pub upgrade_candidates: Vec<String>,
    pub active_source_entries: usize,
    pub duplicate_source_entries: Vec<String>,
    pub apt_metadata_age_days: Option<u64>,
    pub old_kernel_packages: Vec<String>,
    pub foreign_architectures: Vec<String>,
    pub apt_cache_bytes: u64,
}

pub fn collect_report() -> PackageDoctorReport {
    let apt_check = run_command("/usr/bin/apt-get", &["-o", "Debug::NoLocking=1", "check"]);
    let dpkg_audit = run_command("/usr/bin/dpkg", &["--audit"]);
    let held_result = command_lines("/usr/bin/apt-mark", &["showhold"]);
    let autoremove_result =
        simulation_packages(&["-o", "Debug::NoLocking=1", "-s", "autoremove"], "Remv ");
    let upgrades_result =
        simulation_packages(&["-o", "Debug::NoLocking=1", "-s", "upgrade"], "Inst ");
    let source_entries = collect_active_source_entries();
    let active_source_entries = source_entries.len();
    let duplicate_source_entries = duplicate_entries(&source_entries);
    let apt_metadata_age_days = apt_metadata_age_days();
    let old_kernel_packages = installed_old_kernels();
    let foreign_architectures =
        command_lines("/usr/bin/dpkg", &["--print-foreign-architectures"]).unwrap_or_default();
    let apt_cache_bytes = directory_bytes(Path::new("/var/cache/apt/archives"));

    let mut signals = vec![apt_check_signal(apt_check), dpkg_audit_signal(dpkg_audit)];

    let held_packages = held_result.clone().unwrap_or_default();
    signals.push(match held_result {
        Some(_) if held_packages.is_empty() => DiagnosticSignal {
            title: "Held packages".to_string(),
            detail: "No packages are currently held back with apt-mark.".to_string(),
            state: DiagnosticState::Good,
        },
        Some(_) => DiagnosticSignal {
            title: "Held packages".to_string(),
            detail: format!(
                "{} held package(s): {}",
                held_packages.len(),
                preview_names(&held_packages, 5)
            ),
            state: DiagnosticState::Review,
        },
        None => DiagnosticSignal {
            title: "Held packages".to_string(),
            detail: "apt-mark showhold could not be completed, so held-package state is unknown."
                .to_string(),
            state: DiagnosticState::Unknown,
        },
    });

    let autoremove_candidates = autoremove_result.clone().unwrap_or_default();
    signals.push(match autoremove_result {
        Some(_) if autoremove_candidates.is_empty() => DiagnosticSignal {
            title: "Autoremove candidates".to_string(),
            detail: "APT currently reports no automatically-installed packages that are candidates for autoremove.".to_string(),
            state: DiagnosticState::Good,
        },
        Some(_) => DiagnosticSignal {
            title: "Autoremove candidates".to_string(),
            detail: format!(
                "APT simulation reports {} candidate(s): {}",
                autoremove_candidates.len(),
                preview_names(&autoremove_candidates, 5)
            ),
            state: DiagnosticState::Review,
        },
        None => DiagnosticSignal {
            title: "Autoremove candidates".to_string(),
            detail: "APT autoremove simulation could not be completed, so the candidate set is unknown."
                .to_string(),
            state: DiagnosticState::Unknown,
        },
    });

    let upgrade_candidates = upgrades_result.clone().unwrap_or_default();
    signals.push(match upgrades_result {
        Some(_) if upgrade_candidates.is_empty() => DiagnosticSignal {
            title: "Available upgrades".to_string(),
            detail: "No upgrade candidates are visible in the currently cached APT metadata."
                .to_string(),
            state: DiagnosticState::Good,
        },
        Some(_) => DiagnosticSignal {
            title: "Available upgrades".to_string(),
            detail: format!(
                "{} package upgrade(s) are visible in cached metadata: {}",
                upgrade_candidates.len(),
                preview_names(&upgrade_candidates, 5)
            ),
            state: DiagnosticState::Good,
        },
        None => DiagnosticSignal {
            title: "Available upgrades".to_string(),
            detail: "APT upgrade simulation could not be completed. LinuxCare will not infer that zero upgrades are available.".to_string(),
            state: DiagnosticState::Unknown,
        },
    });

    signals.push(DiagnosticSignal {
        title: "APT sources".to_string(),
        detail: if active_source_entries == 0 {
            "No active deb/deb-src source entries were detected. LinuxCare does not run apt update automatically, so network reachability is not tested here.".to_string()
        } else {
            format!(
                "{active_source_entries} active source entr{} detected. Network/repository reachability is intentionally not probed automatically.",
                if active_source_entries == 1 { "y" } else { "ies" }
            )
        },
        state: if active_source_entries == 0 {
            DiagnosticState::Review
        } else {
            DiagnosticState::Good
        },
    });

    signals.push(if duplicate_source_entries.is_empty() {
        DiagnosticSignal {
            title: "Duplicate repository definitions".to_string(),
            detail: "No exact duplicate active APT source definition was detected across .list/.sources files.".to_string(),
            state: DiagnosticState::Good,
        }
    } else {
        DiagnosticSignal {
            title: "Duplicate repository definitions".to_string(),
            detail: format!(
                "{} duplicate active source definition(s) were detected. Duplicate definitions can cause repeated metadata downloads or confusing warnings.",
                duplicate_source_entries.len()
            ),
            state: DiagnosticState::Review,
        }
    });

    signals.push(match apt_metadata_age_days {
        Some(days) if days <= 14 => DiagnosticSignal {
            title: "APT metadata age".to_string(),
            detail: format!("The newest cached repository metadata is approximately {days} day(s) old."),
            state: DiagnosticState::Good,
        },
        Some(days) => DiagnosticSignal {
            title: "APT metadata age".to_string(),
            detail: format!("The newest cached repository metadata is approximately {days} day(s) old. Upgrade counts may be stale until the user explicitly refreshes APT metadata."),
            state: DiagnosticState::Review,
        },
        None => DiagnosticSignal {
            title: "APT metadata age".to_string(),
            detail: "No usable timestamp was found in /var/lib/apt/lists; LinuxCare will not guess whether cached metadata is fresh.".to_string(),
            state: DiagnosticState::Unknown,
        },
    });

    signals.push(if old_kernel_packages.is_empty() {
        DiagnosticSignal {
            title: "Installed kernel images".to_string(),
            detail: "No additional versioned linux-image package was identified beyond the running kernel by this conservative check.".to_string(),
            state: DiagnosticState::Good,
        }
    } else {
        DiagnosticSignal {
            title: "Additional kernel images".to_string(),
            detail: format!(
                "{} non-running versioned kernel image package(s) are installed: {}. LinuxCare does not call them safe to remove; a fallback kernel should normally be retained.",
                old_kernel_packages.len(),
                preview_names(&old_kernel_packages, 5)
            ),
            state: DiagnosticState::Review,
        }
    });

    let review_count = signals
        .iter()
        .filter(|signal| signal.state == DiagnosticState::Review)
        .count();
    let unknown_count = signals
        .iter()
        .filter(|signal| signal.state == DiagnosticState::Unknown)
        .count();

    let (overall_label, summary) = if review_count > 0 {
        (
            "Needs review".to_string(),
            format!(
                "Package Doctor found {review_count} item(s) worth reviewing. Diagnostics are read-only; cleanup actions remain separate and require the LinuxCare helper."
            ),
        )
    } else if unknown_count > 0 {
        (
            "Partially checked".to_string(),
            "Core package checks that were available look healthy, but one or more diagnostic commands are unavailable on this system.".to_string(),
        )
    } else {
        (
            "Healthy".to_string(),
            "APT dependency checks and the dpkg audit look healthy. No package-database repair action is currently indicated.".to_string(),
        )
    };

    PackageDoctorReport {
        overall_label,
        summary,
        signals,
        held_packages,
        autoremove_candidates,
        upgrade_candidates,
        active_source_entries,
        duplicate_source_entries,
        apt_metadata_age_days,
        old_kernel_packages,
        foreign_architectures,
        apt_cache_bytes,
    }
}

#[derive(Debug)]
struct CommandResult {
    success: bool,
    stdout: String,
    stderr: String,
}

fn run_command(program: &str, args: &[&str]) -> Option<CommandResult> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .output()
        .ok()?;

    Some(CommandResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn apt_check_signal(result: Option<CommandResult>) -> DiagnosticSignal {
    match result {
        Some(result) if result.success => DiagnosticSignal {
            title: "APT dependency health".to_string(),
            detail: "apt-get check completed without reporting broken package dependencies."
                .to_string(),
            state: DiagnosticState::Good,
        },
        Some(result) => DiagnosticSignal {
            title: "APT dependency health".to_string(),
            detail: format!(
                "apt-get check reported a problem: {}",
                compact_output(&result.stdout, &result.stderr)
            ),
            state: DiagnosticState::Review,
        },
        None => DiagnosticSignal {
            title: "APT dependency health".to_string(),
            detail: "apt-get is unavailable, so dependency consistency could not be checked."
                .to_string(),
            state: DiagnosticState::Unknown,
        },
    }
}

fn dpkg_audit_signal(result: Option<CommandResult>) -> DiagnosticSignal {
    match result {
        Some(result) if result.success && result.stdout.trim().is_empty() => DiagnosticSignal {
            title: "dpkg audit".to_string(),
            detail: "dpkg --audit reports no partially installed or inconsistent packages."
                .to_string(),
            state: DiagnosticState::Good,
        },
        Some(result) if result.success => DiagnosticSignal {
            title: "dpkg audit".to_string(),
            detail: format!(
                "dpkg reported package state that needs review: {}",
                compact_output(&result.stdout, &result.stderr)
            ),
            state: DiagnosticState::Review,
        },
        Some(result) => DiagnosticSignal {
            title: "dpkg audit".to_string(),
            detail: format!(
                "dpkg audit could not complete cleanly: {}",
                compact_output(&result.stdout, &result.stderr)
            ),
            state: DiagnosticState::Review,
        },
        None => DiagnosticSignal {
            title: "dpkg audit".to_string(),
            detail: "dpkg is unavailable, so installed-package state could not be audited."
                .to_string(),
            state: DiagnosticState::Unknown,
        },
    }
}

fn command_lines(program: &str, args: &[&str]) -> Option<Vec<String>> {
    let result = run_command(program, args)?;
    if !result.success {
        return None;
    }
    Some(
        result
            .stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
    )
}

fn simulation_packages(args: &[&str], prefix: &str) -> Option<Vec<String>> {
    let result = run_command("/usr/bin/apt-get", args)?;
    if !result.success {
        return None;
    }
    Some(parse_simulation_packages(&result.stdout, prefix))
}

fn parse_simulation_packages(output: &str, prefix: &str) -> Vec<String> {
    let mut packages = Vec::new();
    for line in output.lines() {
        let Some(rest) = line.strip_prefix(prefix) else {
            continue;
        };
        let Some(name) = rest.split_whitespace().next() else {
            continue;
        };
        if !name.is_empty() && !packages.iter().any(|existing| existing == name) {
            packages.push(name.to_string());
        }
    }
    packages
}

fn collect_active_source_entries() -> Vec<String> {
    let mut entries = Vec::new();
    entries.extend(list_source_entries(Path::new("/etc/apt/sources.list")));
    if let Ok(files) = fs::read_dir("/etc/apt/sources.list.d") {
        let mut paths = files
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            match path.extension().and_then(|ext| ext.to_str()) {
                Some("list") => entries.extend(list_source_entries(&path)),
                Some("sources") => entries.extend(deb822_source_entries(&path)),
                _ => {}
            }
        }
    }
    entries
}

fn list_source_entries(path: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| line.starts_with("deb ") || line.starts_with("deb-src "))
        .map(normalize_spaces)
        .collect()
}

fn deb822_source_entries(path: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .split("\n\n")
        .filter_map(|stanza| {
            let mut enabled = true;
            let mut types = String::new();
            let mut uris = String::new();
            let mut suites = String::new();
            let mut components = String::new();
            for line in stanza.lines().map(str::trim) {
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some(value) = line.strip_prefix("Enabled:") {
                    enabled = !value.trim().eq_ignore_ascii_case("no");
                } else if let Some(value) = line.strip_prefix("Types:") {
                    types = normalize_spaces(value);
                } else if let Some(value) = line.strip_prefix("URIs:") {
                    uris = normalize_spaces(value);
                } else if let Some(value) = line.strip_prefix("Suites:") {
                    suites = normalize_spaces(value);
                } else if let Some(value) = line.strip_prefix("Components:") {
                    components = normalize_spaces(value);
                }
            }
            if !enabled || types.is_empty() || uris.is_empty() || suites.is_empty() {
                return None;
            }
            Some(format!("deb822|{types}|{uris}|{suites}|{components}"))
        })
        .collect()
}

fn normalize_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn duplicate_entries(entries: &[String]) -> Vec<String> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut counts = BTreeMap::<&str, usize>::new();
    for entry in entries {
        *counts.entry(entry.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(entry, _)| entry.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn apt_metadata_age_days() -> Option<u64> {
    let entries = fs::read_dir("/var/lib/apt/lists").ok()?;
    let newest = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            name != std::ffi::OsStr::new("lock") && name != std::ffi::OsStr::new("partial")
        })
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .max()?;
    let age = std::time::SystemTime::now().duration_since(newest).ok()?;
    Some(age.as_secs() / 86_400)
}

fn installed_old_kernels() -> Vec<String> {
    let running = Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();
    let Some(result) = run_command(
        "/usr/bin/dpkg-query",
        &["-W", "-f=${Package}\\t${Status}\\n", "linux-image-*"],
    ) else {
        return Vec::new();
    };
    if !result.success {
        return Vec::new();
    }
    let running_package = format!("linux-image-{running}");
    result
        .stdout
        .lines()
        .filter_map(|line| {
            let (name, status) = line.split_once('\t')?;
            if status.trim() != "install ok installed" || name == running_package.as_str() {
                return None;
            }
            let suffix = name.strip_prefix("linux-image-")?;
            if suffix.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn directory_bytes(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    walkdir::WalkDir::new(path)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

fn compact_output(stdout: &str, stderr: &str) -> String {
    let joined = format!("{stdout}\n{stderr}");
    let lines = joined
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "command returned a non-success status without diagnostic text".to_string()
    } else {
        lines.join(" • ")
    }
}

fn preview_names(names: &[String], max: usize) -> String {
    let mut preview = names
        .iter()
        .take(max)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > max {
        preview.push_str(&format!(" +{} more", names.len() - max));
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_apt_simulation_lines() {
        let output = "NOTE: This is only a simulation!\nRemv libfoo [1.0]\nRemv libbar [2.0]\n";
        assert_eq!(
            parse_simulation_packages(output, "Remv "),
            vec!["libfoo".to_string(), "libbar".to_string()]
        );
    }

    #[test]
    fn counts_only_enabled_deb822_stanzas() {
        let root =
            std::env::temp_dir().join(format!("linuxcare-deb822-test-{}", std::process::id()));
        let _ = fs::remove_file(&root);
        fs::write(
            &root,
            "Types: deb\nURIs: https://example.invalid\nSuites: stable\n\nTypes: deb\nEnabled: no\nURIs: https://disabled.invalid\nSuites: stable\n",
        )
        .unwrap();
        assert_eq!(deb822_source_entries(&root).len(), 1);
        let _ = fs::remove_file(root);
    }
}
