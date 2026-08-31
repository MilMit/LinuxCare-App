use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    sync::Mutex,
};
use walkdir::WalkDir;
use zbus::{
    connection, fdo, interface,
    message::{Flags, Header},
};

const BUS_NAME: &str = "net.milmit.LinuxCare.Helper";
const OBJECT_PATH: &str = "/net/milmit/LinuxCare/Helper";

const ACTION_APT: &str = "net.milmit.LinuxCare.clean-apt-cache";
const ACTION_SNAP: &str = "net.milmit.LinuxCare.remove-snap-revision";
const ACTION_JOURNAL: &str = "net.milmit.LinuxCare.vacuum-journal";
const ACTION_AUTOREMOVE: &str = "net.milmit.LinuxCare.autoremove-packages";
const ACTION_STORAGE_HEALTH: &str = "net.milmit.LinuxCare.read-storage-health";

struct Helper {
    mutation_lock: Mutex<()>,
}

#[interface(name = "net.milmit.LinuxCare.Helper1")]
impl Helper {
    async fn get_version(&self) -> fdo::Result<String> {
        Ok(env!("CARGO_PKG_VERSION").to_string())
    }

    async fn clean_apt_cache(&self, #[zbus(header)] header: Header<'_>) -> fdo::Result<u64> {
        authorize(&header, ACTION_APT)?;
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| fdo::Error::Failed("lock poisoned".into()))?;
        ensure_binary("/usr/bin/apt-get")?;

        let before = tree_bytes(Path::new("/var/cache/apt/archives"));
        let output = Command::new("/usr/bin/apt-get")
            .arg("clean")
            .env_clear()
            .env("LC_ALL", "C")
            .env("DEBIAN_FRONTEND", "noninteractive")
            .stdin(Stdio::null())
            .output()
            .map_err(failed)?;

        if !output.status.success() {
            return Err(fdo::Error::Failed(clean_stderr(&output.stderr)));
        }
        let after = tree_bytes(Path::new("/var/cache/apt/archives"));
        Ok(before.saturating_sub(after))
    }

    async fn autoremove_packages(&self, #[zbus(header)] header: Header<'_>) -> fdo::Result<u64> {
        authorize(&header, ACTION_AUTOREMOVE)?;
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| fdo::Error::Failed("lock poisoned".into()))?;
        ensure_binary("/usr/bin/apt-get")?;

        let output = Command::new("/usr/bin/apt-get")
            .arg("autoremove")
            .arg("-y")
            .env_clear()
            .env("LC_ALL", "C")
            .env("DEBIAN_FRONTEND", "noninteractive")
            .stdin(Stdio::null())
            .output()
            .map_err(failed)?;

        if !output.status.success() {
            return Err(fdo::Error::Failed(clean_stderr(&output.stderr)));
        }
        // apt-get does not expose a stable machine-readable reclaimed-byte total.
        // Return zero rather than fabricating a storage figure; the GUI reports completion.
        Ok(0)
    }

    async fn remove_disabled_snap_revision(
        &self,
        name: &str,
        revision: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<u64> {
        validate_snap_name(name)?;
        validate_snap_revision(revision)?;
        authorize(&header, ACTION_SNAP)?;
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| fdo::Error::Failed("lock poisoned".into()))?;
        ensure_binary("/usr/bin/snap")?;

        if !snap_revision_is_disabled(name, revision)? {
            return Err(fdo::Error::Failed(
                "refusing removal because snapd no longer reports this exact revision as disabled"
                    .into(),
            ));
        }

        let snap_file = Path::new("/var/lib/snapd/snaps").join(format!("{name}_{revision}.snap"));
        let before = regular_file_len(&snap_file);
        let output = Command::new("/usr/bin/snap")
            .args(["remove", name, &format!("--revision={revision}")])
            .env_clear()
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .output()
            .map_err(failed)?;

        if !output.status.success() {
            return Err(fdo::Error::Failed(clean_stderr(&output.stderr)));
        }

        Ok(before.saturating_sub(regular_file_len(&snap_file)))
    }

    async fn read_storage_health_json(
        &self,
        device: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        validate_block_device_name(device)?;

        let sys_path = Path::new("/sys/block").join(device);
        if !sys_path.exists() {
            return Err(fdo::Error::InvalidArgs(
                "block device is not present in /sys/block".into(),
            ));
        }

        let smartctl = find_executable(&["/usr/sbin/smartctl", "/usr/bin/smartctl"]).ok_or_else(|| {
            fdo::Error::Failed(
                "smartctl is not installed; install the smartmontools package to enable SMART/NVMe health".into(),
            )
        })?;
        ensure_binary("/usr/bin/timeout")?;
        authorize(&header, ACTION_STORAGE_HEALTH)?;

        let device_path = format!("/dev/{device}");
        let output = Command::new("/usr/bin/timeout")
            .args([
                "8s",
                smartctl,
                "-a",
                "-n",
                "standby,0",
                "-j",
                device_path.as_str(),
            ])
            .env_clear()
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .output()
            .map_err(failed)?;

        if output.status.code() == Some(124) {
            return Err(fdo::Error::Failed(
                "smartctl did not respond within 8 seconds".into(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            let stderr = clean_stderr(&output.stderr);
            return Err(fdo::Error::Failed(stderr));
        }

        // smartctl uses an exit-status bitmask for both command errors and health
        // findings. Parse valid JSON even when the process status is non-zero.
        if let Some(error) = linuxcare::hardware::smartctl_command_error(stdout.as_ref()) {
            return Err(fdo::Error::Failed(error));
        }
        let reading = linuxcare::hardware::parse_smartctl_json(device, stdout.as_ref())
            .map_err(fdo::Error::Failed)?;
        serde_json::to_string(&reading)
            .map_err(|error| fdo::Error::Failed(format!("could not encode health result: {error}")))
    }

    async fn vacuum_journal(
        &self,
        keep_days: u32,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<u64> {
        if !matches!(keep_days, 7 | 30 | 90) {
            return Err(fdo::Error::InvalidArgs(
                "keep_days must be exactly 7, 30, or 90".into(),
            ));
        }
        authorize(&header, ACTION_JOURNAL)?;
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| fdo::Error::Failed("lock poisoned".into()))?;
        ensure_binary("/usr/bin/journalctl")?;

        let before = journal_bytes();
        let vacuum = format!("--vacuum-time={keep_days}days");
        let output = Command::new("/usr/bin/journalctl")
            .args(["--rotate", vacuum.as_str()])
            .env_clear()
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .output()
            .map_err(failed)?;

        if !output.status.success() {
            return Err(fdo::Error::Failed(clean_stderr(&output.stderr)));
        }
        let after = journal_bytes();
        Ok(before.saturating_sub(after))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { libc::geteuid() } != 0 {
        return Err("linuxcare-helper must run as root via system D-Bus activation".into());
    }

    // Register the object before claiming the well-known name. This avoids a
    // systemd D-Bus activation race where a queued method call can be delivered
    // as soon as the name is acquired but before ObjectServer::at() completes.
    let _connection = connection::Builder::system()?
        .name(BUS_NAME)?
        .serve_at(
            OBJECT_PATH,
            Helper {
                mutation_lock: Mutex::new(()),
            },
        )?
        .build()
        .await?;

    std::future::pending::<()>().await;
    Ok(())
}

fn authorize(header: &Header<'_>, action: &str) -> fdo::Result<()> {
    if !header
        .primary()
        .flags()
        .contains(Flags::AllowInteractiveAuth)
    {
        return Err(fdo::Error::InteractiveAuthorizationRequired(
            "caller did not declare readiness for interactive authorization".into(),
        ));
    }

    let sender = header
        .sender()
        .ok_or_else(|| fdo::Error::AccessDenied("D-Bus caller has no unique sender".into()))?
        .as_str()
        .to_owned();

    ensure_binary("/usr/bin/pkcheck")?;
    let status = Command::new("/usr/bin/pkcheck")
        .args([
            "--action-id",
            action,
            "--system-bus-name",
            sender.as_str(),
            "--allow-user-interaction",
        ])
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(failed)?;

    if status.success() {
        Ok(())
    } else {
        Err(fdo::Error::AccessDenied(format!(
            "Polkit did not authorize action {action}"
        )))
    }
}

fn snap_revision_is_disabled(name: &str, revision: &str) -> fdo::Result<bool> {
    let output = Command::new("/usr/bin/snap")
        .args(["list", "--all", name])
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(failed)?;

    if !output.status.success() {
        return Err(fdo::Error::Failed(clean_stderr(&output.stderr)));
    }

    for line in String::from_utf8_lossy(&output.stdout).lines().skip(1) {
        let cols: Vec<_> = line.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        if cols[0] == name && cols.get(2).copied() == Some(revision) && cols.contains(&"disabled") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_snap_name(name: &str) -> fdo::Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 40
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if valid {
        Ok(())
    } else {
        Err(fdo::Error::InvalidArgs("invalid Snap package name".into()))
    }
}

fn validate_snap_revision(revision: &str) -> fdo::Result<()> {
    let valid = !revision.is_empty()
        && revision.len() <= 12
        && revision.bytes().all(|b| b.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(fdo::Error::InvalidArgs(
            "invalid Snap revision identifier".into(),
        ))
    }
}

fn validate_block_device_name(device: &str) -> fdo::Result<()> {
    let valid = !device.is_empty()
        && device.len() <= 64
        && device
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        && !device.starts_with('.')
        && Path::new("/sys/block").join(device).exists();
    if valid {
        Ok(())
    } else {
        Err(fdo::Error::InvalidArgs("invalid block device name".into()))
    }
}

fn find_executable<'a>(paths: &[&'a str]) -> Option<&'a str> {
    paths.iter().copied().find(|path| {
        fs::metadata(path)
            .ok()
            .filter(|meta| meta.is_file())
            .map(|meta| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    meta.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            })
            .unwrap_or(false)
    })
}

fn ensure_binary(path: &str) -> fdo::Result<()> {
    let meta = fs::metadata(path).map_err(|e| {
        fdo::Error::Failed(format!(
            "required system binary {path} is not accessible: {e}"
        ))
    })?;
    if !meta.is_file() {
        return Err(fdo::Error::Failed(format!(
            "required path {path} is not a regular file"
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return Err(fdo::Error::Failed(format!(
                "required system binary {path} is not executable"
            )));
        }
    }
    Ok(())
}

fn tree_bytes(root: &Path) -> u64 {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| fs::symlink_metadata(entry.path()).ok())
        .filter(|meta| meta.file_type().is_file())
        .fold(0u64, |acc, meta| acc.saturating_add(meta.len()))
}

fn regular_file_len(path: &Path) -> u64 {
    fs::symlink_metadata(path)
        .ok()
        .filter(|m| m.file_type().is_file())
        .map(|m| m.len())
        .unwrap_or(0)
}

fn journal_bytes() -> u64 {
    tree_bytes(Path::new("/var/log/journal"))
        .saturating_add(tree_bytes(Path::new("/run/log/journal")))
}

fn failed(error: std::io::Error) -> fdo::Error {
    fdo::Error::Failed(error.to_string())
}

fn clean_stderr(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr).trim().to_string();
    if text.is_empty() {
        "privileged command failed".into()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_snap_identifiers() {
        assert!(validate_snap_name("firefox").is_ok());
        assert!(validate_snap_name("gnome-42-2204").is_ok());
        assert!(validate_snap_revision("7312").is_ok());
    }

    #[test]
    fn rejects_argument_shaped_snap_identifiers() {
        for value in ["--help", "foo/bar", "Foo", "foo--bar", "foo-"] {
            assert!(
                validate_snap_name(value).is_err(),
                "{value} should be rejected"
            );
        }
        for value in ["", "-1", "7;rm", "latest"] {
            assert!(
                validate_snap_revision(value).is_err(),
                "{value} should be rejected"
            );
        }
    }
    #[test]
    fn validates_only_sysfs_block_device_basenames() {
        assert!(validate_block_device_name("../sda").is_err());
        assert!(validate_block_device_name("/dev/sda").is_err());
        assert!(validate_block_device_name("--help").is_err());
    }
}
