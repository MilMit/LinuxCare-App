use serde_json::Value;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Docker,
    Podman,
}

impl EngineKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Docker => "Docker",
            Self::Podman => "Podman",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StorageCategory {
    pub name: String,
    pub total_items: Option<u64>,
    pub active_items: Option<u64>,
    pub size_bytes: Option<u64>,
    pub reclaimable_bytes: Option<u64>,
    pub raw_size: String,
    pub raw_reclaimable: String,
}

#[derive(Debug, Clone)]
pub struct ContainerEngineReport {
    pub kind: EngineKind,
    pub installed: bool,
    pub reachable: bool,
    pub version: Option<String>,
    pub storage_root: Option<String>,
    pub categories: Vec<StorageCategory>,
    pub error: Option<String>,
}

impl ContainerEngineReport {
    pub fn total_size_bytes(&self) -> u64 {
        self.categories
            .iter()
            .filter_map(|item| item.size_bytes)
            .sum()
    }

    pub fn reclaimable_bytes(&self) -> u64 {
        self.categories
            .iter()
            .filter_map(|item| item.reclaimable_bytes)
            .sum()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ContainerAnalyzerReport {
    pub engines: Vec<ContainerEngineReport>,
}

pub fn collect_report() -> ContainerAnalyzerReport {
    ContainerAnalyzerReport {
        engines: vec![collect_docker(), collect_podman()],
    }
}

fn collect_docker() -> ContainerEngineReport {
    let installed = command_installed("docker");
    if !installed {
        return unavailable(EngineKind::Docker);
    }

    let version = run_cli(&["docker", "version", "--format", "{{.Client.Version}}"])
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let storage_root = run_cli(&["docker", "info", "--format", "{{.DockerRootDir}}"])
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    match run_cli(&["docker", "system", "df", "--format", "{{json .}}"]) {
        Ok(output) => {
            let categories = parse_docker_df(&output);
            ContainerEngineReport {
                kind: EngineKind::Docker,
                installed,
                reachable: true,
                version,
                storage_root,
                categories,
                error: None,
            }
        }
        Err(error) => ContainerEngineReport {
            kind: EngineKind::Docker,
            installed,
            reachable: false,
            version,
            storage_root,
            categories: Vec::new(),
            error: Some(error),
        },
    }
}

fn collect_podman() -> ContainerEngineReport {
    let installed = command_installed("podman");
    if !installed {
        return unavailable(EngineKind::Podman);
    }

    let version = run_cli(&["podman", "version", "--format", "{{.Client.Version}}"])
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let info = run_cli(&["podman", "info", "--format", "json"]).ok();
    let storage_root = info
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .and_then(|value| {
            value
                .pointer("/store/graphRoot")
                .or_else(|| value.pointer("/Store/GraphRoot"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });

    match run_cli(&["podman", "system", "df", "--format", "json"]) {
        Ok(output) => match parse_podman_df(&output) {
            Ok(categories) => ContainerEngineReport {
                kind: EngineKind::Podman,
                installed,
                reachable: true,
                version,
                storage_root,
                categories,
                error: None,
            },
            Err(error) => ContainerEngineReport {
                kind: EngineKind::Podman,
                installed,
                reachable: false,
                version,
                storage_root,
                categories: Vec::new(),
                error: Some(error),
            },
        },
        Err(error) => ContainerEngineReport {
            kind: EngineKind::Podman,
            installed,
            reachable: false,
            version,
            storage_root,
            categories: Vec::new(),
            error: Some(error),
        },
    }
}

fn unavailable(kind: EngineKind) -> ContainerEngineReport {
    ContainerEngineReport {
        kind,
        installed: false,
        reachable: false,
        version: None,
        storage_root: None,
        categories: Vec::new(),
        error: None,
    }
}

fn command_installed(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_cli(command: &[&str]) -> Result<String, String> {
    let Some((program, args)) = command.split_first() else {
        return Err("empty command".to_string());
    };

    let output = Command::new("timeout")
        .arg("6s")
        .arg(program)
        .args(args)
        .output()
        .map_err(|error| format!("Could not start {program}: {error}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else if output.status.code() == Some(124) {
        Err(format!("{program} did not respond within 6 seconds"))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(format!("{program} returned status {}", output.status))
        } else {
            Err(stderr)
        }
    }
}

fn parse_docker_df(text: &str) -> Vec<StorageCategory> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|value| {
            let raw_size = string_field(&value, &["Size"]);
            let raw_reclaimable = string_field(&value, &["Reclaimable"]);
            StorageCategory {
                name: string_field(&value, &["Type"]),
                total_items: u64_field(&value, &["TotalCount", "Total"]),
                active_items: u64_field(&value, &["Active"]),
                size_bytes: parse_human_bytes(&raw_size),
                reclaimable_bytes: parse_human_bytes(&raw_reclaimable),
                raw_size,
                raw_reclaimable,
            }
        })
        .collect()
}

fn parse_podman_df(text: &str) -> Result<Vec<StorageCategory>, String> {
    let values: Vec<Value> = serde_json::from_str(text)
        .map_err(|error| format!("Podman returned invalid JSON: {error}"))?;
    Ok(values
        .into_iter()
        .map(|value| StorageCategory {
            name: string_field(&value, &["Type"]),
            total_items: u64_field(&value, &["Total", "TotalCount"]),
            active_items: u64_field(&value, &["Active"]),
            size_bytes: u64_field(&value, &["RawSize"])
                .or_else(|| parse_human_bytes(&string_field(&value, &["Size"]))),
            reclaimable_bytes: u64_field(&value, &["RawReclaimable"])
                .or_else(|| parse_human_bytes(&string_field(&value, &["Reclaimable"]))),
            raw_size: string_field(&value, &["Size"]),
            raw_reclaimable: string_field(&value, &["Reclaimable"]),
        })
        .collect())
}

fn string_field(value: &Value, names: &[&str]) -> String {
    for name in names {
        if let Some(field) = value.get(*name) {
            if let Some(text) = field.as_str() {
                return text.to_string();
            }
            if field.is_number() {
                return field.to_string();
            }
        }
    }
    String::new()
}

fn u64_field(value: &Value, names: &[&str]) -> Option<u64> {
    for name in names {
        if let Some(field) = value.get(*name) {
            if let Some(number) = field.as_u64() {
                return Some(number);
            }
            if let Some(text) = field.as_str() {
                if let Ok(number) = text.parse::<u64>() {
                    return Some(number);
                }
            }
        }
    }
    None
}

fn parse_human_bytes(value: &str) -> Option<u64> {
    let clean = value.split('(').next()?.trim().replace(' ', "");
    if clean.is_empty() || clean == "0B" {
        return Some(0);
    }
    let split = clean
        .find(|ch: char| !ch.is_ascii_digit() && ch != '.')
        .unwrap_or(clean.len());
    let number: f64 = clean[..split].parse().ok()?;
    let unit = clean[split..].to_ascii_lowercase();
    let factor = match unit.as_str() {
        "b" | "" => 1.0,
        "kb" | "kib" => 1_000.0,
        "mb" | "mib" => 1_000_000.0,
        "gb" | "gib" => 1_000_000_000.0,
        "tb" | "tib" => 1_000_000_000_000.0,
        _ => return None,
    };
    let bytes = number * factor;
    if !bytes.is_finite() || !(0.0..=u64::MAX as f64).contains(&bytes) {
        None
    } else {
        Some(bytes.round() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_human_sizes() {
        assert_eq!(parse_human_bytes("11.63 MB (70%)"), Some(11_630_000));
        assert_eq!(parse_human_bytes("0B (0%)"), Some(0));
    }

    #[test]
    fn parses_podman_json() {
        let data = r#"[{"Type":"Images","Total":12,"Active":3,"RawSize":13491151377,"RawReclaimable":922956674,"Size":"13.49GB","Reclaimable":"923MB (7%)"}]"#;
        let rows = parse_podman_df(data).expect("podman json");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].reclaimable_bytes, Some(922_956_674));
    }

    #[test]
    fn parses_docker_json_lines() {
        let data = "{\"Type\":\"Images\",\"TotalCount\":\"5\",\"Active\":\"2\",\"Size\":\"16.43MB\",\"Reclaimable\":\"11.63MB (70%)\"}\n";
        let rows = parse_docker_df(data);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total_items, Some(5));
    }
}
