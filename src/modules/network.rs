use crate::network_doctor::{DefaultRoute, NetworkDevice, NetworkDoctorReport};
use gtk::prelude::*;
use std::{sync::mpsc, time::Duration};

pub fn network_doctor_page() -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let title = gtk::Label::new(Some("Network Doctor 3.0"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let probe_btn = gtk::Button::with_label("Run Active Probe");
    probe_btn.add_css_class("pill");
    let refresh_btn = gtk::Button::with_label("Refresh Local State");
    refresh_btn.add_css_class("pill");
    title_row.append(&title);
    title_row.append(&probe_btn);
    title_row.append(&refresh_btn);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Normal refresh is local-only: NetworkManager, IPv4/IPv6 routes, DNS configuration, interface counters, MTU, firewall context and listeners. Active Probe is explicit because it pings the local gateway and resolves example.com through the configured resolver.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.append(&container);

    let render_local = make_renderer(
        container.clone(),
        refresh_btn.clone(),
        probe_btn.clone(),
        false,
    );
    let render_probe = make_renderer(
        container.clone(),
        refresh_btn.clone(),
        probe_btn.clone(),
        true,
    );

    let local_click = render_local.clone();
    refresh_btn.connect_clicked(move |_| local_click());
    let probe_click = render_probe.clone();
    probe_btn.connect_clicked(move |_| probe_click());
    render_local();

    let clamp = adw::Clamp::builder()
        .maximum_size(1000)
        .tightening_threshold(740)
        .child(&page)
        .build();
    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn make_renderer(
    container: gtk::Box,
    refresh_btn: gtk::Button,
    probe_btn: gtk::Button,
    active_probe: bool,
) -> impl Fn() + Clone {
    move || {
        refresh_btn.set_sensitive(false);
        probe_btn.set_sensitive(false);
        refresh_btn.set_label(if active_probe {
            "Probing…"
        } else {
            "Analyzing…"
        });
        while let Some(child) = container.first_child() {
            container.remove(&child);
        }
        let loading = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        loading.add_css_class("finding-card");
        let spinner = gtk::Spinner::new();
        spinner.start();
        loading.append(&spinner);
        let label = gtk::Label::new(Some(if active_probe {
            "Inspecting local network state, pinging the local gateway and resolving example.com…"
        } else {
            "Inspecting local network state without generating an external DNS query…"
        }));
        label.set_xalign(0.0);
        label.set_wrap(true);
        loading.append(&label);
        container.append(&loading);

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let report = if active_probe {
                crate::network_doctor::collect_report_with_active_probe()
            } else {
                crate::network_doctor::collect_report()
            };
            let _ = tx.send(report);
        });

        let container_ui = container.clone();
        let refresh_ui = refresh_btn.clone();
        let probe_ui = probe_btn.clone();
        glib::timeout_add_local(Duration::from_millis(70), move || match rx.try_recv() {
            Ok(report) => {
                while let Some(child) = container_ui.first_child() {
                    container_ui.remove(&child);
                }
                refresh_ui.set_sensitive(true);
                probe_ui.set_sensitive(true);
                refresh_ui.set_label("Refresh Local State");
                render_report(&container_ui, &report);
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                refresh_ui.set_sensitive(true);
                probe_ui.set_sensitive(true);
                refresh_ui.set_label("Refresh Local State");
                glib::ControlFlow::Break
            }
        });
    }
}

fn render_report(container: &gtk::Box, report: &NetworkDoctorReport) {
    container.append(&summary_card(report));
    if let Some(probe) = &report.active_probe {
        container.append(&probe_card(probe));
    }
    container.append(&route_card(&report.default_routes));
    container.append(&dns_card(&report.dns_servers));
    container.append(&devices_card(&report.devices));
    container.append(&interface_health_card(report));
    container.append(&deep_network_card(&report.deep_network));
    container.append(&exposure_card(report));

    if !report.notes.is_empty() {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("finding-card");
        let title = gtk::Label::new(Some("Doctor Notes"));
        title.set_xalign(0.0);
        title.add_css_class("title-3");
        card.append(&title);
        for note in &report.notes {
            let label = gtk::Label::new(Some(&format!("• {note}")));
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.add_css_class("dim-label");
            card.append(&label);
        }
        container.append(&card);
    }
}

fn summary_card(report: &NetworkDoctorReport) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("hero-card");
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let icon = gtk::Image::from_icon_name("network-workgroup-symbolic");
    icon.set_pixel_size(32);
    header.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some("Network Path Health"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    let connectivity = crate::network_doctor::connectivity_label(report.connectivity.as_deref());
    let ipv4 = report
        .default_routes
        .iter()
        .filter(|route| route.family == "IPv4")
        .count();
    let ipv6 = report
        .default_routes
        .iter()
        .filter(|route| route.family == "IPv6")
        .count();
    let detail = gtk::Label::new(Some(&format!(
        "{connectivity} • IPv4 default routes {ipv4} • IPv6 default routes {ipv6} • {} DNS server(s)",
        report.dns_servers.len()
    )));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    text.append(&title);
    text.append(&detail);
    header.append(&text);
    let badge = gtk::Label::new(Some(match report.connectivity.as_deref() {
        Some("full") => "ONLINE",
        Some("portal") | Some("limited") => "REVIEW",
        Some("none") => "OFFLINE",
        _ => "UNKNOWN",
    }));
    badge.add_css_class("risk-badge");
    badge.add_css_class(match report.connectivity.as_deref() {
        Some("full") => "risk-safe",
        Some("portal") | Some("limited") | Some("none") => "risk-review",
        _ => "health-neutral",
    });
    header.append(&badge);
    card.append(&header);
    card.upcast()
}

fn probe_card(probe: &crate::network_doctor::ActiveProbeResult) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("Explicit Active Probe"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);
    let detail = gtk::Label::new(Some(&crate::network_doctor::probe_summary(probe)));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    card.append(&detail);
    card.upcast()
}

fn route_card(routes: &[DefaultRoute]) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("Default Routes"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);
    if routes.is_empty() {
        card.append(&dim_label("No IPv4/IPv6 default route detected."));
    } else {
        for route in routes {
            let gateway = route.gateway.as_deref().unwrap_or("direct/on-link");
            let device = route.device.as_deref().unwrap_or("unknown device");
            let metric = route
                .metric
                .map(|value| format!("metric {value}"))
                .unwrap_or_else(|| "metric unknown".to_string());
            card.append(&detail_row(
                &format!("{} • gateway {gateway}", route.family),
                &format!("via {device} • {metric}"),
            ));
        }
    }
    card.upcast()
}

fn dns_card(servers: &[String]) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("DNS Resolution Path"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);
    if servers.is_empty() {
        card.append(&dim_label("No DNS servers discovered."));
    } else {
        card.append(&dim_label(&servers.join(" • ")));
    }
    card.upcast()
}

fn devices_card(devices: &[NetworkDevice]) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("Network Interfaces"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);
    if devices.is_empty() {
        card.append(&dim_label(
            "NetworkManager interface inventory is unavailable.",
        ));
    } else {
        for device in devices {
            let connection = if device.connection.is_empty() || device.connection == "--" {
                "No active connection"
            } else {
                &device.connection
            };
            card.append(&detail_row(
                &format!("{} • {}", device.name, device.kind),
                &format!("{} • {connection}", device.state),
            ));
        }
    }
    card.upcast()
}

fn interface_health_card(report: &NetworkDoctorReport) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("Interface Counters & MTU"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);
    if report.interface_health.is_empty() {
        card.append(&dim_label(
            "No relevant /sys/class/net counters were available.",
        ));
    } else {
        for interface in &report.interface_health {
            let detail = format!(
                "MTU {} • RX errors {} • TX errors {} • RX dropped {} • TX dropped {} • carrier changes {}",
                optional_u64(interface.mtu),
                optional_u64(interface.rx_errors),
                optional_u64(interface.tx_errors),
                optional_u64(interface.rx_dropped),
                optional_u64(interface.tx_dropped),
                optional_u64(interface.carrier_changes)
            );
            card.append(&detail_row(&interface.name, &detail));
        }
    }
    card.upcast()
}

fn deep_network_card(report: &crate::deep_network::DeepNetworkReport) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("finding-card");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    title_box.set_hexpand(true);
    let title = gtk::Label::new(Some("Deep Network & eBPF Readiness"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    let subtitle = gtk::Label::new(Some(
        "Live socket ownership is read from ss. LinuxCare does not claim per-process bandwidth until an explicit eBPF backend exists.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    title_box.append(&title);
    title_box.append(&subtitle);
    header.append(&title_box);
    let badge = gtk::Label::new(Some(report.ebpf.label()));
    badge.add_css_class("risk-badge");
    badge.add_css_class(if report.ebpf.label() == "READY" {
        "risk-safe"
    } else {
        "risk-review"
    });
    header.append(&badge);
    card.append(&header);

    card.append(&dim_label(&report.ebpf.summary()));
    let metrics = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    metrics.set_homogeneous(true);
    metrics.append(&metric("SOCKETS", &report.total_sockets.to_string()));
    metrics.append(&metric(
        "ESTABLISHED TCP",
        &report.established_tcp.to_string(),
    ));
    metrics.append(&metric("UDP", &report.udp_sockets.to_string()));
    metrics.append(&metric(
        "PID VISIBLE",
        &format!(
            "{}/{}",
            report.process_visible_sockets, report.total_sockets
        ),
    ));
    card.append(&metrics);

    if report.top_processes.is_empty() {
        card.append(&dim_label(
            "No process-owned sockets were visible to the current user session.",
        ));
    } else {
        let heading = gtk::Label::new(Some("Top processes by live socket count"));
        heading.set_xalign(0.0);
        heading.add_css_class("caption-heading");
        card.append(&heading);
        for process in report.top_processes.iter().take(8) {
            let pid = process
                .pid
                .map(|value| format!("PID {value}"))
                .unwrap_or_else(|| "PID hidden".to_string());
            card.append(&detail_row(
                &format!("{} • {pid}", process.process),
                &format!(
                    "{} socket(s) • {} established TCP • {} distinct remote endpoint(s)",
                    process.sockets, process.established_tcp, process.remote_endpoints
                ),
            ));
        }
    }
    card.append(&dim_label(&report.visibility_note));
    card.upcast()
}

fn exposure_card(report: &NetworkDoctorReport) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("Exposure Context"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);
    let metrics = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    metrics.set_homogeneous(true);
    metrics.append(&metric(
        "NETWORK LISTENERS",
        &report.network_listeners.to_string(),
    ));
    metrics.append(&metric(
        "LOCAL LISTENERS",
        &report.local_listeners.to_string(),
    ));
    metrics.append(&metric(
        "UFW",
        match report.firewall_active {
            Some(true) => "Active",
            Some(false) => "Inactive",
            None => "Unknown",
        },
    ));
    card.append(&metrics);
    card.append(&dim_label(
        "A socket bound beyond loopback is not automatically reachable from the Internet. Firewall, VPN, router/NAT, namespace and upstream policy still determine reachability.",
    ));
    card.upcast()
}

fn optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn metric(title: &str, value: &str) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 3);
    box_.add_css_class("metric-card");
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("caption-heading");
    let value = gtk::Label::new(Some(value));
    value.set_xalign(0.0);
    value.add_css_class("title-3");
    box_.append(&heading);
    box_.append(&value);
    box_.upcast()
}

fn detail_row(title: &str, detail: &str) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    row.add_css_class("health-signal-row");
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("heading");
    let detail = gtk::Label::new(Some(detail));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    row.append(&heading);
    row.append(&detail);
    row.upcast()
}

fn dim_label(text: &str) -> gtk::Widget {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.add_css_class("dim-label");
    label.upcast()
}
