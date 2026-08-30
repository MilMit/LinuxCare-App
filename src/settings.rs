use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

fn default_weekly_reminder() -> bool {
    true
}
fn default_auto_empty_trash() -> bool {
    false
}
fn default_true() -> bool {
    true
}
fn default_interval() -> u32 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TopBarConfig {
    #[serde(default = "default_true")]
    pub show_cpu: bool,
    #[serde(default = "default_true")]
    pub show_temp: bool,
    #[serde(default = "default_true")]
    pub show_ram: bool,
    #[serde(default = "default_true")]
    pub show_net: bool,
    #[serde(default = "default_true")]
    pub show_battery: bool,
    #[serde(default = "default_interval")]
    pub interval_sec: u32,
}

impl Default for TopBarConfig {
    fn default() -> Self {
        Self {
            show_cpu: true,
            show_temp: true,
            show_ram: true,
            show_net: true,
            show_battery: true,
            interval_sec: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub exclusions: Vec<String>,
    #[serde(default = "default_weekly_reminder")]
    pub weekly_reminder: bool,
    #[serde(default = "default_auto_empty_trash")]
    pub auto_empty_trash: bool,
    #[serde(default)]
    pub top_bar: TopBarConfig,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            exclusions: Vec::new(),
            weekly_reminder: true,
            auto_empty_trash: false,
            top_bar: TopBarConfig::default(),
        }
    }
}

pub fn load(home: &Path) -> io::Result<AppSettings> {
    let path = config_path(home);
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let data = fs::read(path)?;
    serde_json::from_slice(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn save(home: &Path, settings: &AppSettings) -> io::Result<()> {
    let path = config_path(home);
    let dir = path.parent().expect("config path always has parent");
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
    let tmp = dir.join(format!("config-{}-{nonce}.tmp", std::process::id()));
    let payload = serde_json::to_vec_pretty(settings)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(&payload)?;
    file.sync_all()?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn apply(appearance: Appearance) {
    let manager = adw::StyleManager::default();
    let scheme = match appearance {
        Appearance::System => adw::ColorScheme::Default,
        Appearance::Light => adw::ColorScheme::ForceLight,
        Appearance::Dark => adw::ColorScheme::ForceDark,
    };
    manager.set_color_scheme(scheme);
}

fn config_path(home: &Path) -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(dir).join("linuxcare/config.json")
    } else {
        home.join(".config/linuxcare/config.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_defaults_for_missing_fields() {
        let json = r#"{"appearance": "dark"}"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.appearance, Appearance::Dark);
        assert!(s.exclusions.is_empty());
        assert!(s.weekly_reminder);
        assert!(!s.auto_empty_trash);
    }

    #[test]
    fn serializes_and_deserializes_full_settings() {
        let original = AppSettings {
            appearance: Appearance::Light,
            exclusions: vec!["/tmp/ignore".into()],
            weekly_reminder: false,
            auto_empty_trash: true,
            top_bar: TopBarConfig::default(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.appearance, Appearance::Light);
        assert_eq!(parsed.exclusions, vec!["/tmp/ignore"]);
        assert!(!parsed.weekly_reminder);
        assert!(parsed.auto_empty_trash);
        assert_eq!(parsed.top_bar, TopBarConfig::default());
    }
}
