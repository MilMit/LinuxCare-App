use super::ProviderError;
use std::process::Command;

pub trait FlatpakProvider {
    fn remove_unused_runtimes(&self, user_scope: bool) -> Result<u64, ProviderError>;
}

pub struct FlatpakCliProvider;

impl FlatpakProvider for FlatpakCliProvider {
    fn remove_unused_runtimes(&self, user_scope: bool) -> Result<u64, ProviderError> {
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

        // Parse Flatpak output to estimate recovered space or return status
        Ok(0)
    }
}
