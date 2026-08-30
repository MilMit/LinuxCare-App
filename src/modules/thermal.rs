use crate::thermal_doctor::ThermalDoctorReport;
use gtk::prelude::*;
use std::{sync::mpsc, time::Duration};

pub fn thermal_doctor_page() -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Thermal & Power"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let refresh = gtk::Button::with_label("Refresh Sensors");
    refresh.add_css_class("pill");
    title_row.append(&title);
    title_row.append(&refresh);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Read-only hwmon temperatures, CPU frequency/governor, power profile and available throttle counters. A historical throttle count is evidence to investigate, not proof of a current fault.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.append(&container);

    let render = {
        let container = container.clone();
        let refresh = refresh.clone();
        move || {
            refresh.set_sensitive(false);
            refresh.set_label("Reading…");
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }
            let loading = gtk::Label::new(Some("Reading hwmon and CPU power telemetry…"));
            loading.set_xalign(0.0);
            loading.add_css_class("dim-label");
            container.append(&loading);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::thermal_doctor::collect_report());
            });
            let container_ui = container.clone();
            let refresh_ui = refresh.clone();
            glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                Ok(report) => {
                    while let Some(child) = container_ui.first_child() {
                        container_ui.remove(&child);
                    }
                    refresh_ui.set_sensitive(true);
                    refresh_ui.set_label("Refresh Sensors");
                    render_report(&container_ui, &report);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    refresh_ui.set_sensitive(true);
                    refresh_ui.set_label("Refresh Sensors");
                    glib::ControlFlow::Break
                }
            });
        }
    };
    let render_button = render.clone();
    refresh.connect_clicked(move |_| render_button());
    render();

    let clamp = adw::Clamp::builder()
        .maximum_size(1000)
        .tightening_threshold(740)
        .child(&page)
        .build();
    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn render_report(container: &gtk::Box, report: &ThermalDoctorReport) {
    let hero = gtk::Box::new(gtk::Orientation::Vertical, 10);
    hero.add_css_class("hero-card");
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let title = gtk::Label::new(Some("Thermal State"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-2");
    let badge = gtk::Label::new(Some(&report.status_label.to_uppercase()));
    badge.add_css_class("risk-badge");
    badge.add_css_class(
        if report
            .sensors
            .iter()
            .any(|sensor| sensor.state == crate::thermal_doctor::ThermalState::Critical)
        {
            "risk-dangerous"
        } else if report
            .sensors
            .iter()
            .any(|sensor| sensor.state == crate::thermal_doctor::ThermalState::Review)
            || report.cpu.throttle_events.unwrap_or(0) > 0
        {
            "risk-review"
        } else {
            "risk-safe"
        },
    );
    top.append(&title);
    top.append(&badge);
    hero.append(&top);
    let summary = gtk::Label::new(Some(&report.summary));
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.add_css_class("dim-label");
    hero.append(&summary);

    let metrics = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    metrics.set_homogeneous(true);
    metrics.append(&metric(
        "HOTTEST",
        &report
            .max_temperature_c
            .map(|value| format!("{value:.1}°C"))
            .unwrap_or_else(|| "Unknown".to_string()),
    ));
    metrics.append(&metric(
        "CPU FREQ",
        &report
            .cpu
            .current_mhz
            .map(|value| format!("{value:.0} MHz"))
            .unwrap_or_else(|| "Unknown".to_string()),
    ));
    metrics.append(&metric(
        "GOVERNOR",
        report.cpu.governor.as_deref().unwrap_or("Unknown"),
    ));
    metrics.append(&metric(
        "POWER PROFILE",
        report.cpu.power_profile.as_deref().unwrap_or("Unknown"),
    ));
    hero.append(&metrics);
    container.append(&hero);

    let cpu = gtk::Box::new(gtk::Orientation::Vertical, 5);
    cpu.add_css_class("finding-card");
    let title = gtk::Label::new(Some("CPU Power Context"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    cpu.append(&title);
    let detail = gtk::Label::new(Some(&format!(
        "Current frequency: {} • hardware max: {} • throttle events exposed since boot: {}",
        report
            .cpu
            .current_mhz
            .map(|value| format!("{value:.0} MHz"))
            .unwrap_or_else(|| "unknown".to_string()),
        report
            .cpu
            .max_mhz
            .map(|value| format!("{value:.0} MHz"))
            .unwrap_or_else(|| "unknown".to_string()),
        report
            .cpu
            .throttle_events
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    )));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    cpu.append(&detail);
    container.append(&cpu);

    let sensors = gtk::Box::new(gtk::Orientation::Vertical, 7);
    sensors.add_css_class("finding-card");
    let title = gtk::Label::new(Some("Temperature Sensors"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    sensors.append(&title);
    if report.sensors.is_empty() {
        let empty = gtk::Label::new(Some("No usable hwmon temperature sensors were exposed."));
        empty.set_xalign(0.0);
        empty.add_css_class("dim-label");
        sensors.append(&empty);
    } else {
        for sensor in &report.sensors {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row.add_css_class("health-signal-row");
            let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
            text.set_hexpand(true);
            let name = gtk::Label::new(Some(&sensor.name));
            name.set_xalign(0.0);
            name.add_css_class("heading");
            let detail = gtk::Label::new(Some(&format!(
                "{:.1}°C • max {} • critical {}",
                sensor.temperature_c,
                sensor
                    .max_c
                    .map(|value| format!("{value:.1}°C"))
                    .unwrap_or_else(|| "unknown".to_string()),
                sensor
                    .critical_c
                    .map(|value| format!("{value:.1}°C"))
                    .unwrap_or_else(|| "unknown".to_string())
            )));
            detail.set_xalign(0.0);
            detail.add_css_class("dim-label");
            text.append(&name);
            text.append(&detail);
            row.append(&text);
            let badge = gtk::Label::new(Some(sensor.state.label()));
            badge.add_css_class("risk-badge");
            badge.add_css_class(sensor.state.css_class());
            row.append(&badge);
            sensors.append(&row);
        }
    }
    container.append(&sensors);
}

fn metric(title: &str, value: &str) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 3);
    box_.add_css_class("health-domain-card");
    let title = gtk::Label::new(Some(title));
    title.add_css_class("caption-heading");
    title.add_css_class("dim-label");
    let value = gtk::Label::new(Some(value));
    value.set_wrap(true);
    value.add_css_class("heading");
    box_.append(&title);
    box_.append(&value);
    box_.upcast()
}
