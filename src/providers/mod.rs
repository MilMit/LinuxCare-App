pub mod flatpak;
mod privileged;

use thiserror::Error;

pub use privileged::PrivilegedClient;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("privileged LinuxCare helper is unavailable: {0}")]
    Unavailable(String),
    #[error("privileged operation failed: {0}")]
    Operation(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperStatus {
    Available { version: String },
    NotInstalled,
    ServiceUnavailable(String),
}

pub fn check_helper_status() -> HelperStatus {
    let helper_bin = std::path::Path::new("/usr/libexec/linuxcare-helper");
    let service_file = std::path::Path::new(
        "/usr/share/dbus-1/system-services/net.milmit.LinuxCare.Helper.service",
    );
    if !helper_bin.exists() && !service_file.exists() {
        return HelperStatus::NotInstalled;
    }

    match UbuntuSystemProvider::connect() {
        Ok(provider) => match provider.client.get_version() {
            Ok(version) => HelperStatus::Available { version },
            Err(e) => HelperStatus::ServiceUnavailable(e.to_string()),
        },
        Err(e) => HelperStatus::ServiceUnavailable(e.to_string()),
    }
}

pub trait PackageProvider {
    fn clean_package_cache(&self) -> Result<u64, ProviderError>;
    fn autoremove_packages(&self) -> Result<u64, ProviderError>;
}

pub trait SnapProvider {
    fn remove_disabled_revision(&self, name: &str, revision: &str) -> Result<u64, ProviderError>;
}

pub trait JournalProvider {
    fn vacuum_journal(&self, keep_days: u32) -> Result<u64, ProviderError>;
}

/// Ubuntu/GNOME v0.1 provider. Mutation is intentionally delegated to the
/// Polkit-authorized helper; this object never runs a shell or elevates the GUI.
pub struct UbuntuSystemProvider {
    client: PrivilegedClient,
}

impl UbuntuSystemProvider {
    pub fn connect() -> Result<Self, ProviderError> {
        Ok(Self {
            client: PrivilegedClient::connect()?,
        })
    }
}

impl PackageProvider for UbuntuSystemProvider {
    fn clean_package_cache(&self) -> Result<u64, ProviderError> {
        self.client.clean_apt_cache()
    }

    fn autoremove_packages(&self) -> Result<u64, ProviderError> {
        self.client.autoremove_packages()
    }
}

impl SnapProvider for UbuntuSystemProvider {
    fn remove_disabled_revision(&self, name: &str, revision: &str) -> Result<u64, ProviderError> {
        self.client.remove_disabled_snap_revision(name, revision)
    }
}

impl JournalProvider for UbuntuSystemProvider {
    fn vacuum_journal(&self, keep_days: u32) -> Result<u64, ProviderError> {
        self.client.vacuum_journal(keep_days)
    }
}
