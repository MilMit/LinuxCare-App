use crate::{format, maintenance_intelligence::MaintenanceIntelligenceReport};
use gtk::prelude::*;
use std::{path::PathBuf, sync::mpsc, time::Duration};

pub fn maintenance_center_page(home: PathBuf) -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Maintenance Center"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let refresh = gtk::Button::with_label("Analyze System");
    refresh.add_css_class("pill");
    refresh.add_css_class("suggested-action");
    title_row.append(&title);
    title_row.append(&refresh);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "LinuxCare combines verified signals into prioritized recommendations. Every recommendation explains why it exists; unknown data is never silently treated as healthy.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.append(&container);

    let run = {
        let home = home.clone();
        let container = container.clone();
        let refresh = refresh.clone();
        move || {
            refresh.set_sensitive(false);
            refresh.set_label("Analyzing…");
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }
            let loading = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            loading.add_css_class("finding-card");
            let spinner = gtk::Spinner::new();
            spinner.start();
            loading.append(&spinner);
            let text = gtk::Label::new(Some(
                "Checking storage, services, packages, network, filesystems, thermal state, battery and boot telemetry…",
            ));
            text.set_xalign(0.0);
            text.set_wrap(true);
            loading.append(&text);
            container.append(&loading);

            let (tx, rx) = mpsc::channel();
            let worker_home = home.clone();
            std::thread::spawn(move || {
                let report = crate::maintenance_intelligence::collect_report(&worker_home);
                let _ = tx.send(report);
            });

            let container_ui = container.clone();
            let refresh_ui = refresh.clone();
            glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                Ok(report) => {
                    while let Some(child) = container_ui.first_child() {
                        container_ui.remove(&child);
                    }
                    refresh_ui.set_sensitive(true);
                    refresh_ui.set_label("Analyze System");
                    render_report(&container_ui, &report);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    while let Some(child) = container_ui.first_child() {
                        container_ui.remove(&child);
                    }
                    refresh_ui.set_sensitive(true);
                    refresh_ui.set_label("Analyze System");
                    let error = gtk::Label::new(Some(
                        "Maintenance Intelligence worker stopped before returning a report.",
                    ));
                    error.set_xalign(0.0);
                    error.set_wrap(true);
                    error.add_css_class("dim-label");
                    container_ui.append(&error);
                    glib::ControlFlow::Break
                }
            });
        }
    };

    let run_button = run.clone();
    refresh.connect_clicked(move |_| run_button());
    run();

    let clamp = adw::Clamp::builder()
        .maximum_size(1040)
        .tightening_threshold(760)
        .child(&page)
        .build();
    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn render_report(container: &gtk::Box, report: &MaintenanceIntelligenceReport) {
    let summary = gtk::Box::new(gtk::Orientation::Vertical, 12);
    summary.add_css_class("hero-card");
    let heading = gtk::Label::new(Some("Recommended Actions"));
    heading.set_xalign(0.0);
    heading.add_css_class("title-2");
    let detail = gtk::Label::new(Some(&report.summary));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    summary.append(&heading);
    summary.append(&detail);

    let metrics = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    metrics.set_homogeneous(true);
    metrics.append(&metric("AREAS CHECKED", &report.checked_areas.to_string()));
    metrics.append(&metric(
        "ACTIONS",
        &report.recommendations.len().to_string(),
    ));
    metrics.append(&metric(
        "QUARANTINE",
        &format::bytes(report.quarantine_bytes),
    ));
    metrics.append(&metric(
        "RECENT EVENTS",
        &report.recent_timeline_events.to_string(),
    ));
    summary.append(&metrics);
    container.append(&summary);

    if report.recommendations.is_empty() {
        let status = adw::StatusPage::builder()
            .title("No actionable recommendation")
            .description("LinuxCare did not find a verified signal that crossed its conservative recommendation thresholds.")
            .icon_name("emblem-ok-symbolic")
            .build();
        container.append(&status);
        return;
    }

    for recommendation in &report.recommendations {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 9);
        card.add_css_class("finding-card");

        let top = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let area = gtk::Label::new(Some(&recommendation.area.to_uppercase()));
        area.set_xalign(0.0);
        area.add_css_class("caption-heading");
        area.add_css_class("dim-label");
        let title = gtk::Label::new(Some(&recommendation.title));
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.add_css_class("title-3");
        text.append(&area);
        text.append(&title);
        top.append(&text);
        let badge = gtk::Label::new(Some(recommendation.priority.label()));
        badge.add_css_class("risk-badge");
        badge.add_css_class(recommendation.priority.css_class());
        badge.set_valign(gtk::Align::Center);
        top.append(&badge);
        card.append(&top);

        let why = gtk::Label::new(Some(&format!("Why: {}", recommendation.why)));
        why.set_xalign(0.0);
        why.set_wrap(true);
        card.append(&why);
        let action = gtk::Label::new(Some(&format!(
            "Suggested next step: {}",
            recommendation.suggested_action
        )));
        action.set_xalign(0.0);
        action.set_wrap(true);
        action.add_css_class("dim-label");
        card.append(&action);

        let flags = gtk::Label::new(Some(&format!(
            "{} • {}",
            if recommendation.reversible {
                "Potentially reversible"
            } else {
                "Not presented as reversible"
            },
            if recommendation.requires_privilege {
                "May require administrator privileges"
            } else {
                "No administrator privilege implied"
            }
        )));
        flags.set_xalign(0.0);
        flags.set_wrap(true);
        flags.add_css_class("caption");
        flags.add_css_class("dim-label");
        card.append(&flags);
        container.append(&card);
    }
}

fn metric(title: &str, value: &str) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 3);
    box_.add_css_class("health-domain-card");
    let title = gtk::Label::new(Some(title));
    title.add_css_class("caption-heading");
    title.add_css_class("dim-label");
    let value = gtk::Label::new(Some(value));
    value.add_css_class("heading");
    value.set_wrap(true);
    box_.append(&title);
    box_.append(&value);
    box_.upcast()
}
