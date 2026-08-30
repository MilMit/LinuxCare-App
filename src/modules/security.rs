use adw::prelude::*;
use std::{cell::Cell, rc::Rc};

#[derive(Debug, Clone)]
pub struct OpenPort {
    pub port: u16,
    pub proto: String,
    pub bind_addr: String,
    pub is_network_bound: bool,
    pub process_name: String,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct SecurityAudit {
    pub firewall_active: Option<bool>,
    pub open_ports: Vec<OpenPort>,
}

pub fn security_page() -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Security & Firewall"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");

    let refresh_btn = gtk::Button::with_label("🔄 Refresh Audit");
    refresh_btn.add_css_class("pill");
    refresh_btn.set_valign(gtk::Align::Center);

    title_row.append(&title);
    title_row.append(&refresh_btn);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Audit listening sockets, UFW status, and local exposure indicators without assuming network reachability.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let audit_container = gtk::Box::new(gtk::Orientation::Vertical, 14);
    page.append(&audit_container);

    let refresh_audit = {
        let container = audit_container.clone();
        Rc::new(move || {
            while let Some(c) = container.first_child() {
                container.remove(&c);
            }

            let audit = run_security_audit();

            // 1. Firewall Status Card
            let fw_card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
            fw_card.add_css_class("hero-card");

            let fw_icon = gtk::Image::from_icon_name(match audit.firewall_active {
                Some(true) => "security-high-symbolic",
                Some(false) => "security-low-symbolic",
                None => "dialog-question-symbolic",
            });
            fw_icon.set_pixel_size(28);

            let fw_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
            fw_text.set_hexpand(true);
            let fw_title = gtk::Label::new(Some(match audit.firewall_active {
                Some(true) => "UFW Firewall: Active",
                Some(false) => "UFW Firewall: Inactive",
                None => "UFW Firewall: Status unavailable",
            }));
            fw_title.set_xalign(0.0);
            fw_title.add_css_class("heading");
            let fw_desc = gtk::Label::new(Some(match audit.firewall_active {
                Some(true) => "UFW reports an active policy. Effective reachability still depends on the configured rules and network path.",
                Some(false) => "UFW reports an inactive policy. This does not prove the system has no other firewall; review nftables/firewalld or network policy before changing configuration.",
                None => "LinuxCare could not verify UFW state (for example, UFW may be unavailable or status access may be restricted). No security assumption is made.",
            }));
            fw_desc.set_xalign(0.0);
            fw_desc.set_wrap(true);
            fw_desc.add_css_class("dim-label");
            fw_text.append(&fw_title);
            fw_text.append(&fw_desc);

            let fw_badge = gtk::Label::new(Some(match audit.firewall_active {
                Some(true) => "ACTIVE",
                Some(false) => "INACTIVE",
                None => "UNKNOWN",
            }));
            fw_badge.add_css_class("risk-badge");
            fw_badge.add_css_class(match audit.firewall_active {
                Some(true) => "risk-safe",
                Some(false) => "risk-review",
                None => "health-neutral",
            });
            fw_badge.set_valign(gtk::Align::Center);

            fw_card.append(&fw_icon);
            fw_card.append(&fw_text);
            fw_card.append(&fw_badge);
            container.append(&fw_card);

            // 2. Open Listening Ports Card
            let ports_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
            ports_card.add_css_class("finding-card");

            let ports_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let ports_icon = gtk::Image::from_icon_name("network-server-symbolic");
            ports_icon.set_pixel_size(24);

            let ports_title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
            ports_title_box.set_hexpand(true);
            let ports_title = gtk::Label::new(Some(&format!(
                "Active Listening Ports ({} detected)",
                audit.open_ports.len()
            )));
            ports_title.set_xalign(0.0);
            ports_title.add_css_class("heading");
            let ports_sub = gtk::Label::new(Some(
                "Processes listening on local and public network sockets for incoming connections.",
            ));
            ports_sub.set_xalign(0.0);
            ports_sub.add_css_class("dim-label");
            ports_title_box.append(&ports_title);
            ports_title_box.append(&ports_sub);

            ports_header.append(&ports_icon);
            ports_header.append(&ports_title_box);
            ports_card.append(&ports_header);

            if audit.open_ports.is_empty() {
                let empty_lbl = gtk::Label::new(Some("No open listening ports detected."));
                empty_lbl.set_xalign(0.0);
                empty_lbl.add_css_class("dim-label");
                ports_card.append(&empty_lbl);
            } else {
                let ports_list = gtk::Box::new(gtk::Orientation::Vertical, 6);
                for p in audit.open_ports {
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                    row.add_css_class("finding-card");
                    row.set_margin_bottom(2);

                    let icon = gtk::Image::from_icon_name(if p.is_network_bound {
                        "network-wireless-signal-excellent-symbolic"
                    } else {
                        "network-wired-symbolic"
                    });
                    icon.set_pixel_size(18);

                    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
                    info_box.set_hexpand(true);
                    let port_name = format!("Port {} ({}) • {}", p.port, p.proto, p.process_name);
                    let name_lbl = gtk::Label::new(Some(&port_name));
                    name_lbl.set_xalign(0.0);
                    name_lbl.add_css_class("heading");
                    let addr_desc = if p.is_network_bound {
                        format!("Bound beyond loopback at {} • Reachability still depends on firewall/router rules", p.bind_addr)
                    } else {
                        format!(
                            "Loopback-only ({}) • Not directly exposed on external interfaces",
                            p.bind_addr
                        )
                    };
                    let desc_lbl = gtk::Label::new(Some(&addr_desc));
                    desc_lbl.set_xalign(0.0);
                    desc_lbl.add_css_class("dim-label");
                    info_box.append(&name_lbl);
                    info_box.append(&desc_lbl);

                    let badge = gtk::Label::new(Some(if p.is_network_bound {
                        "NETWORK"
                    } else {
                        "LOCAL"
                    }));
                    badge.add_css_class("risk-badge");
                    badge.add_css_class(if p.is_network_bound {
                        "risk-review"
                    } else {
                        "risk-safe"
                    });
                    badge.set_valign(gtk::Align::Center);

                    row.append(&icon);
                    row.append(&info_box);
                    row.append(&badge);

                    if let Some(pid) = p.pid {
                        let kill_btn = gtk::Button::with_label("Close Port");
                        kill_btn.add_css_class("pill");
                        kill_btn.add_css_class("destructive-action");
                        kill_btn.set_valign(gtk::Align::Center);
                        let armed = Rc::new(Cell::new(false));
                        let armed_c = armed.clone();
                        kill_btn.connect_clicked(move |btn| {
                            if !armed_c.replace(true) {
                                btn.set_label("Confirm Close");
                                return;
                            }
                            armed_c.set(false);
                            btn.set_label("Close Port");
                            let _ = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                        });
                        row.append(&kill_btn);
                    }

                    ports_list.append(&row);
                }
                ports_card.append(&ports_list);
            }
            container.append(&ports_card);
        })
    };

    refresh_audit();

    let ref_c = refresh_audit.clone();
    refresh_btn.connect_clicked(move |_| {
        ref_c();
    });

    let clamp = adw::Clamp::builder()
        .maximum_size(920)
        .tightening_threshold(700)
        .child(&page)
        .build();

    outer.set_child(Some(&clamp));
    outer.upcast()
}

pub fn run_security_audit() -> SecurityAudit {
    let mut audit = SecurityAudit::default();

    // 1. Check UFW
    if let Ok(out) = std::process::Command::new("ufw").arg("status").output() {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
            audit.firewall_active = if text.contains("status: active") {
                Some(true)
            } else if text.contains("status: inactive") {
                Some(false)
            } else {
                None
            };
        }
    }

    // 2. Parse listening ports
    audit.open_ports = parse_listening_ports();

    audit
}

fn is_loopback_bind(addr: &str) -> bool {
    addr.starts_with("127.")
        || addr.starts_with("[::1]")
        || addr.starts_with("::1:")
        || addr.starts_with("localhost:")
}

fn parse_listening_ports() -> Vec<OpenPort> {
    let mut ports: Vec<OpenPort> = Vec::new();

    if let Ok(out) = std::process::Command::new("ss").args(["-tulnp"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 5 {
                continue;
            }
            let proto = parts[0].to_uppercase();
            let local_addr = parts[4];

            let port_str = local_addr.rsplit(':').next().unwrap_or_default();
            let Ok(port_num) = port_str.parse::<u16>() else {
                continue;
            };

            let is_network_bound = !is_loopback_bind(local_addr);
            let mut proc_name = "Unknown".to_string();
            let mut pid = None;

            if parts.len() >= 6 {
                let users_field = parts[parts.len() - 1];
                if let Some(start) = users_field.find("(\"") {
                    if let Some(end) = users_field[start + 2..].find('"') {
                        proc_name = users_field[start + 2..start + 2 + end].to_string();
                    }
                }
                if let Some(pid_start) = users_field.find("pid=") {
                    let rest = &users_field[pid_start + 4..];
                    let pid_end = rest
                        .find(',')
                        .unwrap_or_else(|| rest.find(')').unwrap_or(rest.len()));
                    if let Ok(p) = rest[..pid_end].parse::<u32>() {
                        pid = Some(p);
                    }
                }
            }

            if let Some(existing) = ports
                .iter_mut()
                .find(|p| p.port == port_num && p.proto == proto)
            {
                if is_network_bound && !existing.is_network_bound {
                    existing.bind_addr = local_addr.to_string();
                }
                existing.is_network_bound |= is_network_bound;
                if existing.process_name == "Unknown" && proc_name != "Unknown" {
                    existing.process_name = proc_name;
                }
                if existing.pid.is_none() {
                    existing.pid = pid;
                }
            } else {
                ports.push(OpenPort {
                    port: port_num,
                    proto,
                    bind_addr: local_addr.to_string(),
                    is_network_bound,
                    process_name: proc_name,
                    pid,
                });
            }
        }
    }

    ports.sort_by_key(|p| p.port);
    ports
}
