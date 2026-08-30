use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub const EXTENSION_UUID: &str = "linuxcare-vitals@milmit.net";

#[derive(Debug, Clone, Default)]
pub struct GnomeIntegrationStatus {
    pub shell_version: Option<u32>,
    pub extension_installed: bool,
    pub extension_enabled: Option<bool>,
    pub shell_supported: Option<bool>,
    pub install_location: Option<PathBuf>,
    pub detail: String,
}

#[derive(Debug, Deserialize)]
struct ExtensionMetadata {
    #[serde(rename = "shell-version")]
    shell_versions: Vec<String>,
}

pub fn status(home: &Path) -> GnomeIntegrationStatus {
    let shell_version = detect_shell_major();
    let locations = [
        home.join(".local/share/gnome-shell/extensions")
            .join(EXTENSION_UUID),
        PathBuf::from("/usr/share/gnome-shell/extensions").join(EXTENSION_UUID),
    ];
    let install_location = locations
        .into_iter()
        .find(|path| path.join("metadata.json").exists());
    let extension_installed = install_location.is_some();
    let shell_supported = match (shell_version, install_location.as_deref()) {
        (Some(version), Some(path)) => metadata_supports(path, version),
        _ => None,
    };
    let extension_enabled = if extension_installed {
        extension_enabled()
    } else {
        None
    };
    let detail = if !extension_installed {
        "LinuxCare Vitals extension is not installed in a system or user GNOME extension directory."
            .to_string()
    } else if shell_supported == Some(false) {
        format!(
            "The installed extension metadata does not list GNOME Shell {} as supported.",
            shell_version
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        )
    } else {
        match extension_enabled {
            Some(true) => "LinuxCare Vitals is installed and GNOME reports it enabled.".to_string(),
            Some(false) => "LinuxCare Vitals is installed but currently disabled.".to_string(),
            None => "LinuxCare Vitals is installed; GNOME extension state could not be read in this session."
                .to_string(),
        }
    };

    GnomeIntegrationStatus {
        shell_version,
        extension_installed,
        extension_enabled,
        shell_supported,
        install_location,
        detail,
    }
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let action = if enabled { "enable" } else { "disable" };
    let output = Command::new("gnome-extensions")
        .args([action, EXTENSION_UUID])
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("gnome-extensions is unavailable: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("GNOME could not {action} the extension in this session")
        } else {
            stderr
        })
    }
}

fn detect_shell_major() -> Option<u32> {
    let output = Command::new("gnome-shell")
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .find_map(|part| part.split('.').next()?.parse::<u32>().ok())
}

fn metadata_supports(root: &Path, shell_version: u32) -> Option<bool> {
    let payload = fs::read(root.join("metadata.json")).ok()?;
    let metadata: ExtensionMetadata = serde_json::from_slice(&payload).ok()?;
    Some(metadata.shell_versions.iter().any(|version| {
        version
            .split('.')
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            == Some(shell_version)
    }))
}

fn extension_enabled() -> Option<bool> {
    let output = Command::new("gnome-extensions")
        .args(["info", EXTENSION_UUID])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if text
        .lines()
        .any(|line| line.trim().starts_with("state:") && line.contains("enabled"))
    {
        Some(true)
    } else if text.lines().any(|line| line.trim().starts_with("state:")) {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_uuid_is_stable() {
        assert_eq!(EXTENSION_UUID, "linuxcare-vitals@milmit.net");
    }
}
