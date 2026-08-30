use std::{ffi::CString, fs, mem::MaybeUninit};

#[derive(Debug, Clone)]
pub struct SystemStats {
    pub os_name: String,
    pub kernel_version: String,
    pub uptime_str: String,
    pub ram_used_bytes: u64,
    pub ram_total_bytes: u64,
    pub ram_percent: f32,
    pub disk_root_used_bytes: u64,
    pub disk_root_total_bytes: u64,
    pub disk_root_percent: f32,
    pub disk_home_used_bytes: u64,
    pub disk_home_total_bytes: u64,
    pub disk_home_percent: f32,
}

pub fn collect_system_stats() -> SystemStats {
    let (ram_used_bytes, ram_total_bytes, ram_percent) = read_meminfo();
    let (disk_root_used_bytes, disk_root_total_bytes, disk_root_percent) = read_disk_stat("/");
    let (disk_home_used_bytes, disk_home_total_bytes, disk_home_percent) = read_disk_stat("/home");
    let os_name = read_os_name();
    let kernel_version = read_kernel_version();
    let uptime_str = read_uptime();

    SystemStats {
        os_name,
        kernel_version,
        uptime_str,
        ram_used_bytes,
        ram_total_bytes,
        ram_percent,
        disk_root_used_bytes,
        disk_root_total_bytes,
        disk_root_percent,
        disk_home_used_bytes,
        disk_home_total_bytes,
        disk_home_percent,
    }
}

fn read_meminfo() -> (u64, u64, f32) {
    if let Ok(content) = fs::read_to_string("/proc/meminfo") {
        let mut total_kb = 0u64;
        let mut avail_kb = 0u64;
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = parse_mem_line(line);
            } else if line.starts_with("MemAvailable:") {
                avail_kb = parse_mem_line(line);
            }
        }
        if total_kb > 0 {
            let used_kb = total_kb.saturating_sub(avail_kb);
            let pct = (used_kb as f64 / total_kb as f64) as f32;
            return (used_kb * 1024, total_kb * 1024, pct);
        }
    }
    (0, 0, 0.0)
}

fn parse_mem_line(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
}

fn read_disk_stat(path: &str) -> (u64, u64, f32) {
    let Ok(c_path) = CString::new(path) else {
        return (0, 0, 0.0);
    };
    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
    if unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) } == 0 {
        let stat = unsafe { stat.assume_init() };
        let block_size = stat.f_frsize;
        let total_bytes = stat.f_blocks * block_size;
        let free_bytes = stat.f_bavail * block_size;
        let used_bytes = total_bytes.saturating_sub(free_bytes);
        let pct = if total_bytes > 0 {
            (used_bytes as f64 / total_bytes as f64) as f32
        } else {
            0.0
        };
        (used_bytes, total_bytes, pct)
    } else {
        (0, 0, 0.0)
    }
}

fn read_os_name() -> String {
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                return val.trim_matches('"').to_string();
            }
        }
    }
    "Linux".to_string()
}

fn read_kernel_version() -> String {
    if let Ok(content) = fs::read_to_string("/proc/sys/kernel/osrelease") {
        return content.trim().to_string();
    }
    "—".to_string()
}

fn read_uptime() -> String {
    if let Ok(content) = fs::read_to_string("/proc/uptime") {
        if let Some(first) = content.split_whitespace().next() {
            if let Ok(secs) = first.parse::<f64>() {
                let s = secs as u64;
                let days = s / 86400;
                let hours = (s % 86400) / 3600;
                let mins = (s % 3600) / 60;
                if days > 0 {
                    return format!("{days}d {hours}h {mins}m");
                } else if hours > 0 {
                    return format!("{hours}h {mins}m");
                } else {
                    return format!("{mins}m");
                }
            }
        }
    }
    "—".to_string()
}
