use crate::process_intelligence::{
    anomaly_age, AnomalySeverity, HistoricalVitalsSummary, ProcessAnomaly,
    ProcessIntelligenceReport, ProcessMetric, ProcessSampler,
};
use gtk::prelude::*;
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};

pub fn process_intelligence_page(home: PathBuf) -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Process Intelligence"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let status = gtk::Label::new(Some("LEARNING"));
    status.add_css_class("risk-badge");
    status.add_css_class("health-neutral");
    status.set_valign(gtk::Align::Center);
    title_row.append(&title);
    title_row.append(&status);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Tracks CPU, memory, disk I/O, and system-level network history to explain which processes are drifting from their own recent baseline.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let privacy = gtk::Label::new(Some(
        "Local-only telemetry: LinuxCare stores process name, PID/start identity, and resource counters for up to six hours. Command lines and file paths are not persisted. Per-process network bytes are intentionally not inferred without eBPF/cgroup instrumentation.",
    ));
    privacy.set_xalign(0.0);
    privacy.set_wrap(true);
    privacy.add_css_class("dim-label");
    page.append(&privacy);

    let baseline_label = gtk::Label::new(Some("Collecting baseline…"));
    baseline_label.set_xalign(0.0);
    baseline_label.add_css_class("caption-heading");
    page.append(&baseline_label);

    let vitals_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    vitals_card.add_css_class("process-intelligence-card");
    let vitals_title = gtk::Label::new(Some("Historical Vitals • last 30 minutes"));
    vitals_title.set_xalign(0.0);
    vitals_title.add_css_class("title-3");
    vitals_card.append(&vitals_title);
    let vitals_grid = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    vitals_grid.set_homogeneous(true);
    vitals_card.append(&vitals_grid);
    page.append(&vitals_card);

    let anomaly_card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    anomaly_card.add_css_class("process-intelligence-card");
    let anomaly_title = gtk::Label::new(Some("Anomaly Detection"));
    anomaly_title.set_xalign(0.0);
    anomaly_title.add_css_class("title-3");
    anomaly_card.append(&anomaly_title);
    let anomaly_list = gtk::Box::new(gtk::Orientation::Vertical, 7);
    anomaly_card.append(&anomaly_list);
    page.append(&anomaly_card);

    let process_card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    process_card.add_css_class("process-intelligence-card");
    let process_title = gtk::Label::new(Some("Active Resource Consumers"));
    process_title.set_xalign(0.0);
    process_title.add_css_class("title-3");
    process_card.append(&process_title);
    let process_list = gtk::Box::new(gtk::Orientation::Vertical, 7);
    process_card.append(&process_list);
    page.append(&process_card);

    let notification_home = home.clone();
    let sampler = Rc::new(RefCell::new(ProcessSampler::new(home)));
    let render = {
        let sampler = sampler.clone();
        let status = status.clone();
        let baseline_label = baseline_label.clone();
        let vitals_grid = vitals_grid.clone();
        let anomaly_list = anomaly_list.clone();
        let process_list = process_list.clone();
        let notification_home = notification_home.clone();
        move || {
            let report = sampler.borrow_mut().sample_report();
            if let Some(anomaly) = report
                .anomalies
                .iter()
                .find(|item| item.severity == AnomalySeverity::Critical)
            {
                let _ = crate::notification_center::emit(
                    &notification_home,
                    "process-critical-anomaly",
                    &anomaly.title,
                    &anomaly.detail,
                    crate::notification_center::NotificationSeverity::Critical,
                    false,
                );
            }
            render_report(
                &status,
                &baseline_label,
                &vitals_grid,
                &anomaly_list,
                &process_list,
                &report,
            );
        }
    };

    render();
    let render_timer = render.clone();
    glib::timeout_add_local(Duration::from_secs(3), move || {
        render_timer();
        glib::ControlFlow::Continue
    });

    let clamp = adw::Clamp::builder()
        .maximum_size(1000)
        .tightening_threshold(740)
        .child(&page)
        .build();
    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn render_report(
    status: &gtk::Label,
    baseline_label: &gtk::Label,
    vitals_grid: &gtk::Box,
    anomaly_list: &gtk::Box,
    process_list: &gtk::Box,
    report: &ProcessIntelligenceReport,
) {
    baseline_label.set_text(&report.baseline_detail);
    for class in [
        "risk-safe",
        "risk-review",
        "risk-dangerous",
        "health-neutral",
    ] {
        status.remove_css_class(class);
    }
    if !report.baseline_ready {
        status.set_text("LEARNING");
        status.add_css_class("health-neutral");
    } else if report.anomalies.is_empty() {
        status.set_text("QUIET");
        status.add_css_class("risk-safe");
    } else if report
        .anomalies
        .iter()
        .any(|anomaly| anomaly.severity == AnomalySeverity::Critical)
    {
        status.set_text("ANOMALY");
        status.add_css_class("risk-dangerous");
    } else {
        status.set_text("REVIEW");
        status.add_css_class("risk-review");
    }

    clear_box(vitals_grid);
    render_historical_vitals(vitals_grid, &report.history_summary, report);

    clear_box(anomaly_list);
    render_anomalies(anomaly_list, report);

    clear_box(process_list);
    let anomalous_pids: Vec<u32> = report.anomalies.iter().filter_map(|a| a.pid).collect();
    for process in report.current.processes.iter().take(10) {
        process_list.append(&process_row(process, anomalous_pids.contains(&process.pid)));
    }
    if report.current.processes.is_empty() {
        process_list.append(&dim_label("No readable process telemetry is available."));
    }
}

fn render_historical_vitals(
    container: &gtk::Box,
    summary: &HistoricalVitalsSummary,
    report: &ProcessIntelligenceReport,
) {
    let current = &report.current.system;
    container.append(&metric_card(
        "CPU",
        &format!("{:.0}% now", current.cpu_pct),
        &format!(
            "{:.0}% avg • {:.0}% peak",
            summary.cpu_avg_pct, summary.cpu_peak_pct
        ),
    ));
    container.append(&metric_card(
        "RAM",
        &format!("{:.0}% now", current.ram_pct),
        &format!(
            "{:.0}% avg • {:.0}% peak",
            summary.ram_avg_pct, summary.ram_peak_pct
        ),
    ));
    let network_now = current
        .network_rx_bytes_per_sec
        .saturating_add(current.network_tx_bytes_per_sec);
    container.append(&metric_card(
        "Network",
        &format!("{}/s now", crate::format::bytes(network_now)),
        &format!(
            "{}/s avg • {}/s peak",
            crate::format::bytes(summary.network_avg_bytes_per_sec),
            crate::format::bytes(summary.network_peak_bytes_per_sec)
        ),
    ));
    let disk_now = current
        .disk_read_bytes_per_sec
        .saturating_add(current.disk_write_bytes_per_sec);
    container.append(&metric_card(
        "Disk I/O",
        &format!("{}/s now", crate::format::bytes(disk_now)),
        &format!(
            "{}/s avg • {}/s peak",
            crate::format::bytes(summary.disk_io_avg_bytes_per_sec),
            crate::format::bytes(summary.disk_io_peak_bytes_per_sec)
        ),
    ));
}

fn render_anomalies(container: &gtk::Box, report: &ProcessIntelligenceReport) {
    if !report.baseline_ready {
        container.append(&dim_label(
            "Anomaly alerts stay disabled until enough time-series samples exist. LinuxCare will not label a first-run spike as abnormal.",
        ));
        return;
    }
    if report.anomalies.is_empty() {
        container.append(&dim_label(
            "No meaningful CPU, memory-growth, disk-I/O, or system-network anomaly is visible against the recent baseline.",
        ));
        return;
    }
    for anomaly in &report.anomalies {
        container.append(&anomaly_row(anomaly, report.current.timestamp_unix));
    }
}

fn anomaly_row(anomaly: &ProcessAnomaly, now: u64) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("process-anomaly-row");
    let icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
    icon.set_pixel_size(20);
    row.append(&icon);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(&anomaly.title));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    let detail = gtk::Label::new(Some(&format!(
        "{} • {}",
        anomaly.detail,
        anomaly_age(now, anomaly.first_observed_unix)
    )));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    text.append(&title);
    text.append(&detail);
    row.append(&text);

    let badge = gtk::Label::new(Some(anomaly.severity.label()));
    badge.add_css_class("risk-badge");
    badge.add_css_class(anomaly.severity.css_class());
    badge.set_valign(gtk::Align::Center);
    row.append(&badge);
    row.upcast()
}

fn process_row(process: &ProcessMetric, anomalous: bool) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("process-metric-row");
    let icon = gtk::Image::from_icon_name("application-x-executable-symbolic");
    icon.set_pixel_size(20);
    row.append(&icon);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let name = gtk::Label::new(Some(&format!("{} • PID {}", process.name, process.pid)));
    name.set_xalign(0.0);
    name.add_css_class("heading");
    let io = process.io_bytes_per_sec();
    let owner = if process.owned_by_user {
        "user"
    } else {
        "system/other"
    };
    let detail = gtk::Label::new(Some(&format!(
        "CPU {:.1}% • RAM {} • Disk {}/s • {owner}",
        process.cpu_pct,
        crate::format::bytes(process.ram_bytes),
        crate::format::bytes(io)
    )));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    text.append(&name);
    text.append(&detail);
    row.append(&text);

    if anomalous {
        let badge = gtk::Label::new(Some("ANOMALY"));
        badge.add_css_class("risk-badge");
        badge.add_css_class("risk-review");
        badge.set_valign(gtk::Align::Center);
        row.append(&badge);
    }
    row.upcast()
}

fn metric_card(title: &str, value: &str, detail: &str) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.add_css_class("process-vital-metric");
    let title_label = gtk::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.add_css_class("caption-heading");
    let value_label = gtk::Label::new(Some(value));
    value_label.set_xalign(0.0);
    value_label.add_css_class("title-3");
    let detail_label = gtk::Label::new(Some(detail));
    detail_label.set_xalign(0.0);
    detail_label.set_wrap(true);
    detail_label.add_css_class("dim-label");
    card.append(&title_label);
    card.append(&value_label);
    card.append(&detail_label);
    card.upcast()
}

fn dim_label(text: &str) -> gtk::Widget {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.add_css_class("dim-label");
    label.upcast()
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}
