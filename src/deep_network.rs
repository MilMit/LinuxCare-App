use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Default)]
pub struct EbpfReadiness {
    pub bpffs_mounted: bool,
    pub cgroup_v2: bool,
    pub bpftool_available: bool,
    pub unprivileged_bpf_disabled: Option<u8>,
}

impl EbpfReadiness {
    pub fn label(&self) -> &'static str {
        if self.bpffs_mounted && self.cgroup_v2 && self.bpftool_available {
            "READY"
        } else if self.bpffs_mounted && self.cgroup_v2 {
            "KERNEL READY"
        } else {
            "LIMITED"
        }
    }

    pub fn summary(&self) -> String {
        let unprivileged = match self.unprivileged_bpf_disabled {
            Some(0) => "unprivileged BPF allowed",
            Some(_) => "unprivileged BPF restricted",
            None => "unprivileged BPF policy unknown",
        };
        format!(
            "bpffs {} • cgroup v2 {} • bpftool {} • {unprivileged}",
            yes_no(self.bpffs_mounted),
            yes_no(self.cgroup_v2),
            yes_no(self.bpftool_available)
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProcessConnectionSummary {
    pub process: String,
    pub pid: Option<u32>,
    pub sockets: usize,
    pub established_tcp: usize,
    pub remote_endpoints: usize,
}

#[derive(Debug, Clone, Default)]
pub struct DeepNetworkReport {
    pub ebpf: EbpfReadiness,
    pub total_sockets: usize,
    pub established_tcp: usize,
    pub udp_sockets: usize,
    pub process_visible_sockets: usize,
    pub top_processes: Vec<ProcessConnectionSummary>,
    pub visibility_note: String,
}

pub fn collect_report() -> DeepNetworkReport {
    let ebpf = collect_ebpf_readiness();
    let sockets = collect_socket_lines();
    let mut grouped: BTreeMap<(String, Option<u32>), ProcessAccumulator> = BTreeMap::new();
    let mut established_tcp = 0usize;
    let mut udp_sockets = 0usize;
    let mut process_visible_sockets = 0usize;

    for socket in &sockets {
        if socket.protocol == "tcp" && socket.state.eq_ignore_ascii_case("ESTAB") {
            established_tcp = established_tcp.saturating_add(1);
        }
        if socket.protocol == "udp" {
            udp_sockets = udp_sockets.saturating_add(1);
        }
        let Some(process) = &socket.process else {
            continue;
        };
        process_visible_sockets = process_visible_sockets.saturating_add(1);
        let entry = grouped.entry((process.clone(), socket.pid)).or_default();
        entry.sockets = entry.sockets.saturating_add(1);
        if socket.protocol == "tcp" && socket.state.eq_ignore_ascii_case("ESTAB") {
            entry.established_tcp = entry.established_tcp.saturating_add(1);
        }
        if !socket.remote.is_empty() && !is_unspecified_peer(&socket.remote) {
            entry.remote_endpoints.insert(socket.remote.clone());
        }
    }

    let mut top_processes = grouped
        .into_iter()
        .map(|((process, pid), value)| ProcessConnectionSummary {
            process,
            pid,
            sockets: value.sockets,
            established_tcp: value.established_tcp,
            remote_endpoints: value.remote_endpoints.len(),
        })
        .collect::<Vec<_>>();
    top_processes.sort_by_key(|item| std::cmp::Reverse(item.sockets));
    top_processes.truncate(10);

    let visibility_note = if sockets.is_empty() {
        "Socket inventory is unavailable. Install iproute2/ss or check the current sandbox permissions."
            .to_string()
    } else if process_visible_sockets < sockets.len() {
        format!(
            "Process ownership is visible for {process_visible_sockets}/{} socket entries. Linux hides some ownership metadata from unprivileged users; LinuxCare does not guess missing PIDs.",
            sockets.len()
        )
    } else {
        "Process ownership was visible for all returned socket entries. Endpoint metadata is used only for this live view and is not persisted.".to_string()
    };

    DeepNetworkReport {
        ebpf,
        total_sockets: sockets.len(),
        established_tcp,
        udp_sockets,
        process_visible_sockets,
        top_processes,
        visibility_note,
    }
}

#[derive(Debug, Clone, Default)]
struct SocketEntry {
    protocol: String,
    state: String,
    remote: String,
    process: Option<String>,
    pid: Option<u32>,
}

#[derive(Debug, Default)]
struct ProcessAccumulator {
    sockets: usize,
    established_tcp: usize,
    remote_endpoints: BTreeSet<String>,
}

fn collect_ebpf_readiness() -> EbpfReadiness {
    let bpffs_mounted = fs::read_to_string("/proc/mounts")
        .map(|text| {
            text.lines().any(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                fields.get(1) == Some(&"/sys/fs/bpf") && fields.get(2) == Some(&"bpf")
            })
        })
        .unwrap_or(false);
    let cgroup_v2 = Path::new("/sys/fs/cgroup/cgroup.controllers").exists();
    let bpftool_available = Command::new("bpftool")
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    let unprivileged_bpf_disabled =
        fs::read_to_string("/proc/sys/kernel/unprivileged_bpf_disabled")
            .ok()
            .and_then(|value| value.trim().parse::<u8>().ok());

    EbpfReadiness {
        bpffs_mounted,
        cgroup_v2,
        bpftool_available,
        unprivileged_bpf_disabled,
    }
}

fn collect_socket_lines() -> Vec<SocketEntry> {
    let Ok(output) = Command::new("ss")
        .args(["-H", "-t", "-u", "-n", "-a", "-p"])
        .stdin(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_ss_line)
        .collect()
}

fn parse_ss_line(line: &str) -> Option<SocketEntry> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 6 {
        return None;
    }
    let protocol = fields[0].to_ascii_lowercase();
    if protocol != "tcp" && protocol != "udp" {
        return None;
    }
    let state = fields[1].to_string();
    let remote = fields.get(5).copied().unwrap_or_default().to_string();
    let users = fields.iter().skip(6).copied().collect::<Vec<_>>().join(" ");
    let (process, pid) = parse_users_field(&users);
    Some(SocketEntry {
        protocol,
        state,
        remote,
        process,
        pid,
    })
}

fn parse_users_field(value: &str) -> (Option<String>, Option<u32>) {
    let process = value.find("((\"").and_then(|start| {
        let rest = &value[start + 3..];
        rest.find('"').map(|end| rest[..end].to_string())
    });
    let pid = value.find("pid=").and_then(|start| {
        let rest = &value[start + 4..];
        let end = rest
            .find(|ch: char| !ch.is_ascii_digit())
            .unwrap_or(rest.len());
        rest[..end].parse::<u32>().ok()
    });
    (process, pid)
}

fn is_unspecified_peer(value: &str) -> bool {
    value.starts_with("0.0.0.0:")
        || value.starts_with("[::]:")
        || value == "*:*"
        || value.starts_with("*:0")
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tcp_ss_line_with_process() {
        let line =
            "tcp ESTAB 0 0 192.168.1.20:42310 1.1.1.1:443 users:((\"firefox\",pid=4242,fd=91))";
        let socket = parse_ss_line(line).expect("socket");
        assert_eq!(socket.protocol, "tcp");
        assert_eq!(socket.state, "ESTAB");
        assert_eq!(socket.process.as_deref(), Some("firefox"));
        assert_eq!(socket.pid, Some(4242));
        assert_eq!(socket.remote, "1.1.1.1:443");
    }

    #[test]
    fn does_not_invent_process_for_hidden_owner() {
        let line = "udp UNCONN 0 0 0.0.0.0:5353 0.0.0.0:*";
        let socket = parse_ss_line(line).expect("socket");
        assert!(socket.process.is_none());
        assert!(socket.pid.is_none());
    }
}
