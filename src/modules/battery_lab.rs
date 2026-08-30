use crate::battery::{BatteryDevice, BatteryLabReport};
use gtk::prelude::*;
use std::{path::PathBuf, sync::mpsc, time::Duration};

pub fn battery_lab_page(home: PathBuf) -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Battery Lab"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let refresh_btn = gtk::Button::with_label("Refresh Battery");
    refresh_btn.add_css_class("pill");
    title_row.append(&title);
    title_row.append(&refresh_btn);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Battery Lab 2.0 adds private local history for capacity-health trend and recent discharge power while keeping firmware thresholds read-only.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.append(&container);

    let container_c = container.clone();
    let refresh_btn_c = refresh_btn.clone();
    let render_home = home.clone();
    let render = move || {
        refresh_btn_c.set_sensitive(false);
        refresh_btn_c.set_label("Reading…");
        while let Some(child) = container_c.first_child() {
            container_c.remove(&child);
        }

        let loading = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        loading.add_css_class("battery-lab-card");
        let spinner = gtk::Spinner::new();
        spinner.start();
        loading.append(&spinner);
        let label = gtk::Label::new(Some("Reading /sys/class/power_supply…"));
        label.set_xalign(0.0);
        loading.append(&label);
        container_c.append(&loading);

        let (tx, rx) = mpsc::channel();
        let worker_home = render_home.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::battery::collect_report_with_history(&worker_home));
        });

        let container_ui = container_c.clone();
        let refresh_btn_ui = refresh_btn_c.clone();
        glib::timeout_add_local(Duration::from_millis(70), move || match rx.try_recv() {
            Ok(report) => {
                while let Some(child) = container_ui.first_child() {
                    container_ui.remove(&child);
                }
                refresh_btn_ui.set_sensitive(true);
                refresh_btn_ui.set_label("Refresh Battery");
                render_report(&container_ui, &report);
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                while let Some(child) = container_ui.first_child() {
                    container_ui.remove(&child);
                }
                refresh_btn_ui.set_sensitive(true);
                refresh_btn_ui.set_label("Refresh Battery");
                let label = gtk::Label::new(Some(
                    "Battery Lab worker stopped before returning telemetry.",
                ));
                label.set_xalign(0.0);
                label.add_css_class("dim-label");
                container_ui.append(&label);
                glib::ControlFlow::Break
            }
        });
    };

    let render_c = render.clone();
    refresh_btn.connect_clicked(move |_| render_c());
    render();

    let clamp = adw::Clamp::builder()
        .maximum_size(980)
        .tightening_threshold(720)
        .child(&page)
        .build();
    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn render_report(container: &gtk::Box, report: &BatteryLabReport) {
    let summary = gtk::Box::new(gtk::Orientation::Vertical, 12);
    summary.add_css_class("battery-lab-card");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let icon = gtk::Image::from_icon_name("battery-symbolic");
    icon.set_pixel_size(32);
    header.append(&icon);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(if report.batteries.is_empty() {
        "No battery detected"
    } else if report.batteries.len() == 1 {
        "1 battery detected"
    } else {
        "Multiple batteries detected"
    }));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    let detail = gtk::Label::new(Some(&external_power_text(report.external_power_online)));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    text.append(&title);
    text.append(&detail);
    header.append(&text);
    summary.append(&header);

    if report.batteries.is_empty() {
        let note = gtk::Label::new(Some(
            "This is normal on desktops and virtual machines. LinuxCare does not fabricate battery health when the kernel exposes no Battery-type power_supply device.",
        ));
        note.set_xalign(0.0);
        note.set_wrap(true);
        note.add_css_class("dim-label");
        summary.append(&note);
    }
    container.append(&summary);

    for battery in &report.batteries {
        container.append(&battery_card(battery));
    }

    if !report.trends.is_empty() {
        container.append(&trend_card(report));
    }

    if !report.batteries.is_empty() {
        let note = gtk::Box::new(gtk::Orientation::Vertical, 5);
        note.add_css_class("battery-lab-card");
        let title = gtk::Label::new(Some("How LinuxCare interprets battery health"));
        title.set_xalign(0.0);
        title.add_css_class("heading");
        let body = gtk::Label::new(Some(
            "Health is calculated only when the kernel exposes both current full capacity and design capacity. It is a capacity-wear estimate, not a vendor SMART-style diagnostic. Runtime estimates are based on instantaneous power/current and can move significantly with workload.",
        ));
        body.set_xalign(0.0);
        body.set_wrap(true);
        body.add_css_class("dim-label");
        note.append(&title);
        note.append(&body);
        container.append(&note);
    }
}

fn trend_card(report: &BatteryLabReport) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("battery-lab-card");
    let title = gtk::Label::new(Some("Battery History & Trend"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);
    let note = gtk::Label::new(Some(
        "LinuxCare records at most one local battery sample every 15 minutes for up to 14 days. Health change is shown only after at least 24 hours because battery gauges can recalibrate.",
    ));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.add_css_class("dim-label");
    card.append(&note);
    for trend in &report.trends {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
        row.add_css_class("health-signal-row");
        let name = gtk::Label::new(Some(&trend.name));
        name.set_xalign(0.0);
        name.add_css_class("heading");
        let delta = trend
            .health_delta_percent
            .map(|value| format!("{value:+.1} percentage points health ratio"))
            .unwrap_or_else(|| "health trend still learning".to_string());
        let power = trend
            .average_discharge_power_w
            .map(|value| format!("recent discharge avg {value:.1} W"))
            .unwrap_or_else(|| "recent discharge average unavailable".to_string());
        let detail = gtk::Label::new(Some(&format!(
            "{} samples • {:.1}h span • {delta} • {power}",
            trend.sample_count, trend.span_hours
        )));
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        detail.add_css_class("dim-label");
        row.append(&name);
        row.append(&detail);
        card.append(&row);
    }
    card.upcast()
}

fn battery_card(battery: &BatteryDevice) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("battery-lab-card");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let icon = gtk::Image::from_icon_name("battery-symbolic");
    icon.set_pixel_size(28);
    header.append(&icon);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(&battery.display_name()));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    let mut descriptors = vec![format!("Kernel device {}", battery.name)];
    if let Some(scope) = &battery.scope {
        descriptors.push(format!("Scope {scope}"));
    }
    if let Some(technology) = &battery.technology {
        descriptors.push(technology.clone());
    }
    let detail = gtk::Label::new(Some(&descriptors.join(" • ")));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    text.append(&title);
    text.append(&detail);
    header.append(&text);

    let badge_text = battery
        .capacity_percent
        .map(|value| format!("{value}%"))
        .unwrap_or_else(|| "UNKNOWN".to_string());
    let badge = gtk::Label::new(Some(&badge_text));
    badge.add_css_class("risk-badge");
    badge.add_css_class(capacity_css(battery.capacity_percent));
    badge.set_valign(gtk::Align::Center);
    header.append(&badge);
    card.append(&header);

    let state_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    state_row.set_homogeneous(true);
    state_row.append(&battery_metric("STATUS", &battery.status));
    state_row.append(&battery_metric(
        "HEALTH",
        &battery
            .health_percent
            .map(|value| format!("{:.0}% • {}", value.min(100.0), battery.health_label()))
            .unwrap_or_else(|| "Unknown".to_string()),
    ));
    state_row.append(&battery_metric(
        "CYCLES",
        &battery
            .cycle_count
            .map(|value| value.to_string())
            .unwrap_or_else(|| "Unknown".to_string()),
    ));
    state_row.append(&battery_metric(
        "RUNTIME",
        &crate::battery::format_minutes(battery.time_remaining_minutes, &battery.status),
    ));
    card.append(&state_row);

    let capacity_bar = gtk::ProgressBar::new();
    capacity_bar.add_css_class("battery-capacity-progress");
    capacity_bar.set_fraction(
        battery
            .capacity_percent
            .map(|value| value as f64 / 100.0)
            .unwrap_or(0.0),
    );
    card.append(&capacity_bar);

    let energy_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    energy_row.set_homogeneous(true);
    energy_row.append(&battery_metric(
        "CURRENT ENERGY",
        &format_optional(battery.energy_now_wh, "Wh", 1),
    ));
    energy_row.append(&battery_metric(
        "FULL CAPACITY",
        &format_optional(battery.energy_full_wh, "Wh", 1),
    ));
    energy_row.append(&battery_metric(
        "DESIGN CAPACITY",
        &format_optional(battery.energy_design_wh, "Wh", 1),
    ));
    card.append(&energy_row);

    let electrical_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    electrical_row.set_homogeneous(true);
    electrical_row.append(&battery_metric(
        "POWER",
        &format_optional(battery.power_w, "W", 1),
    ));
    electrical_row.append(&battery_metric(
        "VOLTAGE",
        &format_optional(battery.voltage_v, "V", 2),
    ));
    electrical_row.append(&battery_metric(
        "CURRENT",
        &format_optional(battery.current_a.map(f64::abs), "A", 2),
    ));
    card.append(&electrical_row);

    if battery.charge_start_threshold.is_some() || battery.charge_end_threshold.is_some() {
        let threshold = gtk::Box::new(gtk::Orientation::Vertical, 3);
        threshold.add_css_class("battery-threshold-row");
        let title = gtk::Label::new(Some("Firmware charge thresholds"));
        title.set_xalign(0.0);
        title.add_css_class("heading");
        let start = battery
            .charge_start_threshold
            .map(|value| format!("start {value}%"))
            .unwrap_or_else(|| "start unknown".to_string());
        let end = battery
            .charge_end_threshold
            .map(|value| format!("stop {value}%"))
            .unwrap_or_else(|| "stop unknown".to_string());
        let body = gtk::Label::new(Some(&format!(
            "{start} • {end}. LinuxCare only reads these values; it does not change firmware thresholds."
        )));
        body.set_xalign(0.0);
        body.set_wrap(true);
        body.add_css_class("dim-label");
        threshold.append(&title);
        threshold.append(&body);
        card.append(&threshold);
    }

    card.upcast()
}

fn battery_metric(title: &str, value: &str) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 3);
    box_.add_css_class("battery-metric");
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("caption-heading");
    let value_label = gtk::Label::new(Some(value));
    value_label.set_wrap(true);
    value_label.add_css_class("heading");
    box_.append(&title_label);
    box_.append(&value_label);
    box_.upcast()
}

fn external_power_text(online: Option<bool>) -> String {
    match online {
        Some(true) => "External power is currently connected according to the kernel.".to_string(),
        Some(false) => "External power supplies are detected but currently offline.".to_string(),
        None => "External power state is not exposed by this system.".to_string(),
    }
}

fn capacity_css(capacity: Option<u8>) -> &'static str {
    match capacity {
        Some(value) if value >= 30 => "risk-safe",
        Some(value) if value >= 15 => "risk-review",
        Some(_) => "risk-dangerous",
        None => "health-neutral",
    }
}

fn format_optional(value: Option<f64>, unit: &str, decimals: usize) -> String {
    let Some(value) = value else {
        return "Unknown".to_string();
    };
    match decimals {
        0 => format!("{value:.0} {unit}"),
        1 => format!("{value:.1} {unit}"),
        2 => format!("{value:.2} {unit}"),
        _ => format!("{value} {unit}"),
    }
}
