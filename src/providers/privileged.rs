use super::ProviderError;
use std::time::Duration;
use zbus::{
    blocking::{connection::Builder, Connection, Proxy},
    proxy::MethodFlags,
};

const SERVICE: &str = "net.milmit.LinuxCare.Helper";
const PATH: &str = "/net/milmit/LinuxCare/Helper";
const INTERFACE: &str = "net.milmit.LinuxCare.Helper1";
const INTERACTIVE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

fn is_store_edition() -> bool {
    std::env::var_os("LINUXCARE_STORE_EDITION").as_deref()
        == Some(std::ffi::OsStr::new("1"))
}

pub struct PrivilegedClient {
    connection: Connection,
}

impl PrivilegedClient {
    pub fn connect() -> Result<Self, ProviderError> {
        if is_store_edition() {
            return Err(ProviderError::Unavailable(
                "This host-level maintenance action is unavailable in the strictly confined Snap Store edition. Use the LinuxCare Debian edition for Polkit-authorized APT, Snap, journal and privileged storage maintenance."
                    .into(),
            ));
        }

        let connection = Builder::system()
            .map_err(|e| ProviderError::Unavailable(e.to_string()))?
            .method_timeout(INTERACTIVE_TIMEOUT)
            .build()
            .map_err(|e| ProviderError::Unavailable(e.to_string()))?;
        Ok(Self { connection })
    }

    fn proxy(&self) -> Result<Proxy<'_>, ProviderError> {
        Proxy::new(&self.connection, SERVICE, PATH, INTERFACE)
            .map_err(|e| ProviderError::Unavailable(e.to_string()))
    }

    pub fn get_version(&self) -> Result<String, ProviderError> {
        self.proxy()?
            .call_with_flags("GetVersion", MethodFlags::AllowInteractiveAuth.into(), &())
            .map_err(|e| ProviderError::Unavailable(e.to_string()))?
            .ok_or_else(|| {
                ProviderError::Unavailable("privileged helper returned empty version".into())
            })
    }

    pub fn clean_apt_cache(&self) -> Result<u64, ProviderError> {
        self.interactive_call("CleanAptCache", &())
    }

    pub fn autoremove_packages(&self) -> Result<u64, ProviderError> {
        self.interactive_call("AutoremovePackages", &())
    }

    pub fn remove_disabled_snap_revision(
        &self,
        name: &str,
        revision: &str,
    ) -> Result<u64, ProviderError> {
        self.interactive_call("RemoveDisabledSnapRevision", &(name, revision))
    }

    pub fn vacuum_journal(&self, keep_days: u32) -> Result<u64, ProviderError> {
        self.interactive_call("VacuumJournal", &(keep_days,))
    }

    pub fn read_storage_health_json(&self, device: &str) -> Result<String, ProviderError> {
        self.proxy()?
            .call_with_flags(
                "ReadStorageHealthJson",
                MethodFlags::AllowInteractiveAuth.into(),
                &(device,),
            )
            .map_err(|error| ProviderError::Operation(error.to_string()))?
            .ok_or_else(|| {
                ProviderError::Operation("privileged helper returned no health reply".into())
            })
    }

    fn interactive_call<B>(&self, method: &str, body: &B) -> Result<u64, ProviderError>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        self.proxy()?
            .call_with_flags(method, MethodFlags::AllowInteractiveAuth.into(), body)
            .map_err(|e| ProviderError::Operation(e.to_string()))?
            .ok_or_else(|| ProviderError::Operation("privileged helper returned no reply".into()))
    }
}
