use std::{
    ffi::CString,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemState {
    Good,
    Review,
    Critical,
    Unknown,
}

impl FilesystemState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Good => "GOOD",
            Self::Review => "REVIEW",
            Self::Critical => "CRITICAL",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::Good => "risk-safe",
            Self::Review => "risk-review",
            Self::Critical => "risk-dangerous",
            Self::Unknown => "health-neutral",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MountHealth {
    pub mount_point: PathBuf,
    pub fs_type: String,
    pub source: String,
    pub read_only: bool,
    pub used_percent: Option<f64>,
    pub inode_used_percent: Option<f64>,
    pub free_bytes: Option<u64>,
    pub state: FilesystemState,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct SnapshotAwareness {
    pub btrfs_mounts: Vec<PathBuf>,
    pub snapper_available: bool,
    pub snapper_snapshot_count: Option<usize>,
    pub timeshift_detected: bool,
    pub snapshot_directories: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct FilesystemDoctorReport {
    pub mounts: Vec<MountHealth>,
    pub kernel_errors_available: bool,
    pub kernel_error_lines: Vec<String>,
    pub snapshots: SnapshotAwareness,
    pub status_label: String,
    pub summary: String,
}

pub fn collect_report() -> FilesystemDoctorReport {
    let mut mounts = parse_mountinfo()
        .into_iter()
        .filter(|mount| should_show_fs(&mount.fs_type, &mount.mount_point))
        .map(enrich_mount)
        .collect::<Vec<_>>();
    mounts.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));

    let (kernel_errors_available, kernel_error_lines) = collect_filesystem_errors();
    let snapshots = collect_snapshot_awareness(&mounts);

    let critical = mounts
        .iter()
        .filter(|mount| mount.state == FilesystemState::Critical)
        .count();
    let review = mounts
        .iter()
        .filter(|mount| mount.state == FilesystemState::Review)
        .count();

    let (status_label, summary) = if !kernel_error_lines.is_empty() || critical > 0 {
        (
            "Needs attention".to_string(),
            "Filesystem Doctor found critical capacity/inode pressure, an unexpected read-only mount, or filesystem-related kernel errors from this boot.".to_string(),
        )
    } else if review > 0 {
        (
            "Review".to_string(),
            "One or more persistent filesystems are approaching capacity or inode pressure. No repair is performed automatically.".to_string(),
        )
    } else if mounts.is_empty() {
        (
            "Unavailable".to_string(),
            "LinuxCare could not identify a persistent filesystem mount to evaluate.".to_string(),
        )
    } else if !kernel_errors_available {
        (
            "Partially checked".to_string(),
            "Mount capacity/inode state is available, but the current-boot kernel journal could not be read. LinuxCare does not infer that filesystem errors are absent.".to_string(),
        )
    } else {
        (
            "Healthy".to_string(),
            "Persistent filesystems are mounted with no high capacity/inode pressure and no matching current-boot kernel error visible in the available data.".to_string(),
        )
    };

    FilesystemDoctorReport {
        mounts,
        kernel_errors_available,
        kernel_error_lines,
        snapshots,
        status_label,
        summary,
    }
}

#[derive(Debug, Clone)]
struct RawMount {
    mount_point: PathBuf,
    fs_type: String,
    source: String,
    options: String,
    super_options: String,
}

fn parse_mountinfo() -> Vec<RawMount> {
    let Ok(text) = fs::read_to_string("/proc/self/mountinfo") else {
        return Vec::new();
    };
    text.lines().filter_map(parse_mountinfo_line).collect()
}

fn parse_mountinfo_line(line: &str) -> Option<RawMount> {
    let (pre, post) = line.split_once(" - ")?;
    let pre_fields = pre.split_whitespace().collect::<Vec<_>>();
    let post_fields = post.split_whitespace().collect::<Vec<_>>();
    if pre_fields.len() < 6 || post_fields.len() < 3 {
        return None;
    }
    Some(RawMount {
        mount_point: PathBuf::from(unescape_mount_field(pre_fields[4])),
        options: pre_fields[5].to_string(),
        fs_type: post_fields[0].to_string(),
        source: unescape_mount_field(post_fields[1]),
        super_options: post_fields[2].to_string(),
    })
}

fn unescape_mount_field(value: &str) -> String {
    value
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

fn should_show_fs(fs_type: &str, mount: &Path) -> bool {
    const PSEUDO: &[&str] = &[
        "proc",
        "sysfs",
        "devtmpfs",
        "devpts",
        "tmpfs",
        "cgroup",
        "cgroup2",
        "pstore",
        "securityfs",
        "debugfs",
        "tracefs",
        "configfs",
        "fusectl",
        "mqueue",
        "hugetlbfs",
        "autofs",
        "binfmt_misc",
        "rpc_pipefs",
        "nsfs",
        "overlay",
        "squashfs",
    ];
    if PSEUDO.contains(&fs_type) {
        return false;
    }
    !mount.starts_with("/run") && !mount.starts_with("/snap")
}

fn enrich_mount(raw: RawMount) -> MountHealth {
    let stats = statvfs(&raw.mount_point);
    let read_only = raw.options.split(',').any(|opt| opt == "ro")
        || raw.super_options.split(',').any(|opt| opt == "ro");
    let is_root = raw.mount_point == Path::new("/");

    let used_percent = stats.as_ref().and_then(|stats| stats.used_percent());
    let inode_used_percent = stats.as_ref().and_then(|stats| stats.inode_used_percent());
    let free_bytes = stats.as_ref().map(|stats| stats.free_bytes);

    let mut state = FilesystemState::Good;
    let mut reasons = Vec::new();
    if read_only && is_root {
        state = FilesystemState::Critical;
        reasons.push("root filesystem is read-only".to_string());
    } else if read_only {
        state = FilesystemState::Review;
        reasons.push("mounted read-only".to_string());
    }
    if let Some(percent) = used_percent {
        if percent >= 95.0 {
            state = FilesystemState::Critical;
            reasons.push(format!("{percent:.0}% space used"));
        } else if percent >= 85.0 && state != FilesystemState::Critical {
            state = FilesystemState::Review;
            reasons.push(format!("{percent:.0}% space used"));
        }
    }
    if let Some(percent) = inode_used_percent {
        if percent >= 95.0 {
            state = FilesystemState::Critical;
            reasons.push(format!("{percent:.0}% inodes used"));
        } else if percent >= 85.0 && state != FilesystemState::Critical {
            state = FilesystemState::Review;
            reasons.push(format!("{percent:.0}% inodes used"));
        }
    }
    if stats.is_none() {
        state = FilesystemState::Unknown;
        reasons.push("statvfs unavailable".to_string());
    }
    if reasons.is_empty() {
        reasons.push("capacity and inode pressure are below review thresholds".to_string());
    }

    MountHealth {
        mount_point: raw.mount_point,
        fs_type: raw.fs_type,
        source: raw.source,
        read_only,
        used_percent,
        inode_used_percent,
        free_bytes,
        state,
        detail: reasons.join(" • "),
    }
}

#[derive(Debug, Clone)]
struct FsStats {
    blocks: u64,
    free_blocks: u64,
    files: u64,
    free_files: u64,
    free_bytes: u64,
}

impl FsStats {
    fn used_percent(&self) -> Option<f64> {
        if self.blocks == 0 {
            return None;
        }
        let used = self.blocks.saturating_sub(self.free_blocks);
        Some(used as f64 / self.blocks as f64 * 100.0)
    }

    fn inode_used_percent(&self) -> Option<f64> {
        if self.files == 0 {
            return None;
        }
        let used = self.files.saturating_sub(self.free_files);
        Some(used as f64 / self.files as f64 * 100.0)
    }
}

fn statvfs(path: &Path) -> Option<FsStats> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut raw = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(c_path.as_ptr(), raw.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    let raw = unsafe { raw.assume_init() };
    let frsize = raw.f_frsize;
    let bsize = raw.f_bsize;
    let block_size = if frsize > 0 { frsize } else { bsize };
    let blocks = raw.f_blocks;
    let free_blocks = raw.f_bfree;
    let available_blocks = raw.f_bavail;
    let files = raw.f_files;
    let free_files = raw.f_ffree;
    Some(FsStats {
        blocks,
        free_blocks,
        files,
        free_files,
        free_bytes: available_blocks.saturating_mul(block_size),
    })
}

fn collect_filesystem_errors() -> (bool, Vec<String>) {
    let Ok(output) = Command::new("journalctl")
        .args(["-k", "-b", "--no-pager", "-p", "err..alert", "-o", "cat"])
        .output()
    else {
        return (false, Vec::new());
    };
    if !output.status.success() {
        return (false, Vec::new());
    }

    let needles = [
        "ext4-fs error",
        "btrfs error",
        "xfs (",
        "i/o error",
        "buffer i/o error",
        "filesystem error",
        "metadata corruption",
    ];
    let lines = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            let lower = line.to_lowercase();
            needles.iter().any(|needle| lower.contains(needle))
        })
        .take(8)
        .map(|line| line.trim().to_string())
        .collect();
    (true, lines)
}

fn collect_snapshot_awareness(mounts: &[MountHealth]) -> SnapshotAwareness {
    let btrfs_mounts = mounts
        .iter()
        .filter(|mount| mount.fs_type == "btrfs")
        .map(|mount| mount.mount_point.clone())
        .collect::<Vec<_>>();
    let snapper_available = command_exists("snapper");
    let snapper_snapshot_count = if snapper_available {
        Command::new("snapper")
            .args(["--csvout", "list"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .skip(1)
                    .filter(|line| !line.trim().is_empty())
                    .count()
            })
    } else {
        None
    };

    let snapshot_directories = [Path::new("/.snapshots"), Path::new("/timeshift/snapshots")]
        .into_iter()
        .filter(|path| path.exists())
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    let timeshift_detected = command_exists("timeshift")
        || snapshot_directories
            .iter()
            .any(|path| path.starts_with("/timeshift"));

    SnapshotAwareness {
        btrfs_mounts,
        snapper_available,
        snapper_snapshot_count,
        timeshift_detected,
        snapshot_directories,
    }
}

fn command_exists(command: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(command).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mountinfo() {
        let line = "36 25 8:2 / / rw,relatime - ext4 /dev/sda2 rw,errors=remount-ro";
        let mount = parse_mountinfo_line(line).expect("mount");
        assert_eq!(mount.mount_point, PathBuf::from("/"));
        assert_eq!(mount.fs_type, "ext4");
        assert_eq!(mount.source, "/dev/sda2");
    }

    #[test]
    fn unescapes_spaces() {
        assert_eq!(unescape_mount_field("/media/My\\040Disk"), "/media/My Disk");
    }
}
