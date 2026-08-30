use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::Path,
    process::{Command, Stdio},
    time::Instant,
};

#[derive(Debug, Clone, Default)]
pub struct NetworkDevice {
    pub name: String,
    pub kind: String,
    pub state: String,
    pub connection: String,
}

#[derive(Debug, Clone, Default)]
pub struct DefaultRoute {
    pub family: String,
    pub gateway: Option<String>,
    pub device: Option<String>,
    pub metric: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct InterfaceHealth {
    pub name: String,
    pub mtu: Option<u64>,
    pub rx_errors: Option<u64>,
    pub tx_errors: Option<u64>,
    pub rx_dropped: Option<u64>,
    pub tx_dropped: Option<u64>,
    pub carrier_changes: Option<u64>,
}

impl InterfaceHealth {
    pub fn error_total(&self) -> u64 {
        self.rx_errors.unwrap_or(0)
            + self.tx_errors.unwrap_or(0)
            + self.rx_dropped.unwrap_or(0)
            + self.tx_dropped.unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ActiveProbeResult {
    pub gateway_latency_ms: Option<f64>,
    pub dns_resolution_ms: Option<f64>,
    pub dns_resolution_ok: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkDoctorReport {
    pub network_manager_available: bool,
    pub connectivity: Option<String>,
    pub devices: Vec<NetworkDevice>,
    pub default_routes: Vec<DefaultRoute>,
    pub dns_servers: Vec<String>,
    pub interface_health: Vec<InterfaceHealth>,
    pub firewall_active: Option<bool>,
    pub network_listeners: usize,
    pub local_listeners: usize,
    pub active_probe: Option<ActiveProbeResult>,
    pub deep_network: crate::deep_network::DeepNetworkReport,
    pub notes: Vec<String>,
}

pub fn collect_report() -> NetworkDoctorReport {
    collect_report_internal(false)
}

pub fn collect_report_with_active_probe() -> NetworkDoctorReport {
    collect_report_internal(true)
}

fn collect_report_internal(active_probe: bool) -> NetworkDoctorReport {
    let mut report = NetworkDoctorReport::default();

    if let Ok(out) = Command::new("nmcli")
        .args(["-t", "-f", "CONNECTIVITY", "general"])
        .stdin(Stdio::null())
        .output()
    {
        report.network_manager_available = out.status.success();
        if out.status.success() {
            let value = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
            if !value.is_empty() {
                report.connectivity = Some(value);
            }
        }
    }

    report.devices = collect_devices();
    report.default_routes = collect_default_routes();
    report.dns_servers = collect_dns_servers();
    report.interface_health = collect_interface_health(&report.devices, &report.default_routes);
    report.deep_network = crate::deep_network::collect_report();

    let security = crate::modules::security::run_security_audit();
    report.firewall_active = security.firewall_active;
    report.network_listeners = security
        .open_ports
        .iter()
        .filter(|port| port.is_network_bound)
        .count();
    report.local_listeners = security
        .open_ports
        .iter()
        .filter(|port| !port.is_network_bound)
        .count();

    if active_probe {
        report.active_probe = Some(run_active_probe(&report.default_routes));
    }

    add_notes(&mut report);
    report
}

fn add_notes(report: &mut NetworkDoctorReport) {
    if report.default_routes.is_empty() {
        report.notes.push(
            "No IPv4 or IPv6 default route was detected. Local networking may still work, but normal routed connectivity is unavailable.".to_string(),
        );
    } else if report.default_routes.len() > 1 {
        report.notes.push(format!(
            "{} default routes were detected across IPv4/IPv6. This can be valid with VPNs or multiple uplinks; route metrics and policy determine the selected path.",
            report.default_routes.len()
        ));
    }

    if report.dns_servers.is_empty() {
        report.notes.push(
            "No DNS server could be discovered through resolvectl or /etc/resolv.conf.".to_string(),
        );
    }

    match report.connectivity.as_deref() {
        Some("portal") => report.notes.push(
            "NetworkManager reports a captive portal. Sign-in may be required before full Internet access works.".to_string(),
        ),
        Some("limited") => report.notes.push(
            "NetworkManager reports limited connectivity: a local network exists but full Internet access is not confirmed.".to_string(),
        ),
        Some("none") => report
            .notes
            .push("NetworkManager reports no network connectivity.".to_string()),
        _ => {}
    }

    for interface in &report.interface_health {
        let errors = interface.error_total();
        if errors > 0 {
            report.notes.push(format!(
                "{} reports {} cumulative RX/TX error or drop event(s) since the interface counters were reset. A non-zero lifetime count is context, not proof of a current fault.",
                interface.name, errors
            ));
        }
        if let Some(mtu) = interface.mtu {
            if mtu < 1280 {
                report.notes.push(format!(
                    "{} has MTU {mtu}, below the IPv6 minimum MTU. This may be intentional on a special tunnel but deserves review.",
                    interface.name
                ));
            }
        }
    }
}

fn collect_devices() -> Vec<NetworkDevice> {
    let Ok(out) = Command::new("nmcli")
        .args([
            "-t",
            "-e",
            "no",
            "-f",
            "DEVICE,TYPE,STATE,CONNECTION",
            "device",
            "status",
        ])
        .stdin(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(parse_device_line)
        .collect()
}

fn parse_device_line(line: &str) -> Option<NetworkDevice> {
    let mut parts = line.splitn(4, ':');
    let name = parts.next()?.trim();
    let kind = parts.next()?.trim();
    let state = parts.next()?.trim();
    let connection = parts.next().unwrap_or_default().trim();
    if name.is_empty() {
        return None;
    }
    Some(NetworkDevice {
        name: name.to_string(),
        kind: kind.to_string(),
        state: state.to_string(),
        connection: connection.to_string(),
    })
}

fn collect_default_routes() -> Vec<DefaultRoute> {
    let mut routes = Vec::new();
    routes.extend(collect_family_routes("IPv4", "-4"));
    routes.extend(collect_family_routes("IPv6", "-6"));
    routes
}

fn collect_family_routes(family: &str, flag: &str) -> Vec<DefaultRoute> {
    let Ok(out) = Command::new("ip")
        .args([flag, "route", "show", "default"])
        .stdin(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| parse_default_route(line, family))
        .collect()
}

fn parse_default_route(line: &str, family: &str) -> Option<DefaultRoute> {
    if !line.split_whitespace().any(|item| item == "default") {
        return None;
    }
    let parts = line.split_whitespace().collect::<Vec<_>>();
    let mut route = DefaultRoute {
        family: family.to_string(),
        ..Default::default()
    };
    let mut index = 0usize;
    while index < parts.len() {
        match parts[index] {
            "via" if index + 1 < parts.len() => route.gateway = Some(parts[index + 1].to_string()),
            "dev" if index + 1 < parts.len() => route.device = Some(parts[index + 1].to_string()),
            "metric" if index + 1 < parts.len() => route.metric = parts[index + 1].parse().ok(),
            _ => {}
        }
        index += 1;
    }
    Some(route)
}

fn collect_dns_servers() -> Vec<String> {
    let mut servers = BTreeSet::new();
    if let Ok(out) = Command::new("resolvectl")
        .arg("dns")
        .stdin(Stdio::null())
        .output()
    {
        if out.status.success() {
            for token in String::from_utf8_lossy(&out.stdout).split_whitespace() {
                let candidate = token.trim_matches(|ch: char| ch == ':' || ch == '[' || ch == ']');
                if candidate.parse::<std::net::IpAddr>().is_ok() {
                    servers.insert(candidate.to_string());
                }
            }
        }
    }

    if servers.is_empty() {
        if let Ok(text) = fs::read_to_string("/etc/resolv.conf") {
            for line in text.lines() {
                let mut parts = line.split_whitespace();
                if parts.next() == Some("nameserver") {
                    if let Some(server) = parts.next() {
                        if server.parse::<std::net::IpAddr>().is_ok() {
                            servers.insert(server.to_string());
                        }
                    }
                }
            }
        }
    }

    servers.into_iter().collect()
}

fn collect_interface_health(
    devices: &[NetworkDevice],
    routes: &[DefaultRoute],
) -> Vec<InterfaceHealth> {
    let mut names = HashSet::new();
    for device in devices {
        if device.name != "lo" {
            names.insert(device.name.clone());
        }
    }
    for route in routes {
        if let Some(device) = &route.device {
            if device != "lo" {
                names.insert(device.clone());
            }
        }
    }
    let mut names = names.into_iter().collect::<Vec<_>>();
    names.sort();
    names
        .into_iter()
        .filter(|name| Path::new("/sys/class/net").join(name).exists())
        .map(|name| {
            let root = Path::new("/sys/class/net").join(&name);
            InterfaceHealth {
                name,
                mtu: read_u64(&root.join("mtu")),
                rx_errors: read_u64(&root.join("statistics/rx_errors")),
                tx_errors: read_u64(&root.join("statistics/tx_errors")),
                rx_dropped: read_u64(&root.join("statistics/rx_dropped")),
                tx_dropped: read_u64(&root.join("statistics/tx_dropped")),
                carrier_changes: read_u64(&root.join("carrier_changes")),
            }
        })
        .collect()
}

fn run_active_probe(routes: &[DefaultRoute]) -> ActiveProbeResult {
    let gateway_latency_ms = routes
        .iter()
        .filter_map(|route| route.gateway.as_deref())
        .find_map(ping_gateway);

    let started = Instant::now();
    let dns_output = Command::new("getent")
        .args(["ahosts", "example.com"])
        .stdin(Stdio::null())
        .output();
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    let dns_resolution_ok = dns_output
        .as_ref()
        .ok()
        .map(|output| output.status.success());
    let dns_resolution_ms = dns_resolution_ok.filter(|ok| *ok).map(|_| elapsed);

    ActiveProbeResult {
        gateway_latency_ms,
        dns_resolution_ms,
        dns_resolution_ok,
    }
}

fn ping_gateway(gateway: &str) -> Option<f64> {
    let output = Command::new("ping")
        .args(["-n", "-c", "1", "-W", "1", gateway])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_ping_latency(&text)
}

fn parse_ping_latency(text: &str) -> Option<f64> {
    for token in text.split_whitespace() {
        if let Some(value) = token.strip_prefix("time=") {
            if let Ok(ms) = value.trim_end_matches("ms").parse::<f64>() {
                return Some(ms);
            }
        }
    }
    None
}

fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

pub fn connectivity_label(value: Option<&str>) -> &'static str {
    match value {
        Some("full") => "Full connectivity",
        Some("portal") => "Captive portal",
        Some("limited") => "Limited connectivity",
        Some("none") => "No connectivity",
        Some("unknown") | None => "Unknown",
        Some(_) => "Unknown",
    }
}

pub fn probe_summary(probe: &ActiveProbeResult) -> String {
    let gateway = probe
        .gateway_latency_ms
        .map(|value| format!("gateway {value:.1} ms"))
        .unwrap_or_else(|| "gateway unavailable".to_string());
    let dns = match (probe.dns_resolution_ok, probe.dns_resolution_ms) {
        (Some(true), Some(value)) => format!("DNS {value:.1} ms"),
        (Some(false), _) => "DNS resolution failed".to_string(),
        _ => "DNS probe unavailable".to_string(),
    };
    format!("{gateway} • {dns}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_route() {
        let route = parse_default_route(
            "default via 192.168.1.1 dev wlp0s20f3 proto dhcp src 192.168.1.10 metric 600",
            "IPv4",
        )
        .expect("route");
        assert_eq!(route.family, "IPv4");
        assert_eq!(route.gateway.as_deref(), Some("192.168.1.1"));
        assert_eq!(route.device.as_deref(), Some("wlp0s20f3"));
        assert_eq!(route.metric, Some(600));
    }

    #[test]
    fn parses_nmcli_device() {
        let device = parse_device_line("wlp0s20f3:wifi:connected:Home WiFi").expect("device");
        assert_eq!(device.name, "wlp0s20f3");
        assert_eq!(device.kind, "wifi");
        assert_eq!(device.state, "connected");
        assert_eq!(device.connection, "Home WiFi");
    }

    #[test]
    fn parses_ping_time() {
        let text = "64 bytes from 192.168.1.1: icmp_seq=1 ttl=64 time=1.42 ms";
        assert_eq!(parse_ping_latency(text), Some(1.42));
    }
}
