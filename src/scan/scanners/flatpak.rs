use crate::{
    model::{CleanupCandidate, CleanupCategory, CleanupRisk, ExecutionKind, ScannerResult},
    scan::{ScanContext, Scanner},
};
use std::{collections::HashSet, path::Path, process::Command, time::Instant};

pub struct FlatpakScanner;

impl Scanner for FlatpakScanner {
    fn name(&self) -> &'static str {
        "Flatpak runtimes"
    }

    fn scan(&self, _ctx: &ScanContext) -> ScannerResult {
        let started = Instant::now();
        let mut result = ScannerResult {
            module: self.name().into(),
            ..Default::default()
        };
        let binary = Path::new("/usr/bin/flatpak");
        if !binary.exists() {
            result.duration_ms = started.elapsed().as_millis();
            return result;
        }

        for scope in ["--user", "--system"] {
            let apps = Command::new(binary)
                .args([scope, "list", "--app", "--columns=runtime"])
                .env("LC_ALL", "C")
                .output();
            let runtimes = Command::new(binary)
                .args([scope, "list", "--runtime", "--columns=ref,size"])
                .env("LC_ALL", "C")
                .output();
            let (Ok(apps), Ok(runtimes)) = (apps, runtimes) else {
                continue;
            };
            if !apps.status.success() || !runtimes.status.success() {
                continue;
            }

            let used: HashSet<String> = String::from_utf8_lossy(&apps.stdout)
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();

            for line in String::from_utf8_lossy(&runtimes.stdout).lines() {
                let mut parts = line.splitn(2, '\t');
                let Some(reference) = parts.next().map(str::trim).filter(|s| !s.is_empty()) else {
                    continue;
                };
                let normalized = reference.strip_prefix("runtime/").unwrap_or(reference);
                let id = normalized.split('/').next().unwrap_or("");
                let primary_platform = matches!(
                    id,
                    "org.freedesktop.Platform" | "org.gnome.Platform" | "org.kde.Platform"
                );
                if !primary_platform || used.contains(normalized) {
                    continue;
                }

                let size_text = parts.next().unwrap_or("").trim();
                let size_bytes = parse_flatpak_size(size_text).unwrap_or(0);
                result.candidates.push(CleanupCandidate {
                    id: format!("flatpak:{scope}:{normalized}"),
                    category: CleanupCategory::FlatpakRuntime,
                    title: normalized.to_string(),
                    description: format!("A platform runtime with no directly installed app reporting it as its runtime ({scope})."),
                    path: None,
                    size_bytes,
                    risk: CleanupRisk::Advanced,
                    reason: format!("No installed app directly references this runtime. Flatpak still gets final authority over whether it is truly unused. Flatpak reports an installed size of {size_text}; OSTree deduplication means that figure is not guaranteed reclaimable space."),
                    consequence: "Unused Flatpak runtimes will be uninstalled via Flatpak dependency validation.".into(),
                    requires_privilege: scope == "--system",
                    execution: ExecutionKind::FlatpakUnused,
                    privileged_action: Some(crate::model::PrivilegedAction::RemoveUnusedFlatpakRuntimes { user_scope: scope == "--user" }),
                });
            }
        }
        result.duration_ms = started.elapsed().as_millis();
        result
    }
}

fn parse_flatpak_size(input: &str) -> Option<u64> {
    let normalized = input.replace('\u{a0}', " ");
    let mut parts = normalized.split_whitespace();
    let value: f64 = parts.next()?.replace(',', ".").parse().ok()?;
    let unit = parts.next().unwrap_or("B").to_ascii_uppercase();
    let factor = match unit.as_str() {
        "B" => 1f64,
        "KB" | "KIB" => 1024f64,
        "MB" | "MIB" => 1024f64.powi(2),
        "GB" | "GIB" => 1024f64.powi(3),
        "TB" | "TIB" => 1024f64.powi(4),
        _ => return None,
    };
    Some((value * factor) as u64)
}

#[cfg(test)]
mod tests {
    use super::parse_flatpak_size;

    #[test]
    fn parses_common_sizes() {
        assert_eq!(
            parse_flatpak_size("557.0 MB"),
            Some((557.0 * 1024.0 * 1024.0) as u64)
        );
        assert_eq!(
            parse_flatpak_size("1.5 GB"),
            Some((1.5 * 1024.0 * 1024.0 * 1024.0) as u64)
        );
    }
}
