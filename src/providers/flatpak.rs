use super::ProviderError;
use std::process::Command;

pub trait FlatpakProvider {
    fn remove_unused_runtimes(&self, user_scope: bool) -> Result<u64, ProviderError>;
}

pub struct FlatpakCliProvider;

impl FlatpakProvider for FlatpakCliProvider {
    fn remove_unused_runtimes(&self, user_scope: bool) -> Result<u64, ProviderError> {
        if std::env::var_os("LINUXCARE_STORE_EDITION").as_deref()
            == Some(std::ffi::OsStr::new("1"))
        {
            return Err(ProviderError::Unavailable(
                "Flatpak runtime removal is unavailable in the strictly confined Snap Store edition."
                    .into(),
            ));
        }

        let binary = "/usr/bin/flatpak";
        if !std::path::Path::new(binary).exists() {
            return Err(ProviderError::Unavailable(
                "Flatpak is not installed".into(),
            ));
        }

        let scope_arg = if user_scope { "--user" } else { "--system" };
        let output = Command::new(binary)
            .args(["uninstall", "--unused", "-y", scope_arg])
            .env("LC_ALL", "C")
            .output()
            .map_err(|e| ProviderError::Operation(format!("Failed to execute flatpak: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProviderError::Operation(format!(
                "Flatpak unused runtime cleanup failed: {}",
                stderr.trim()
            )));
        }

        Ok(0)
    }
}
