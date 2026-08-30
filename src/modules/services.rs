use gtk::prelude::*;
use std::{cell::Cell, process::Command, rc::Rc, sync::mpsc, time::Duration};

#[derive(Debug, Clone)]
struct UserService {
    name: String,
    active_state: String,
    substate: String,
    description: String,
}

impl UserService {
    fn is_active(&self) -> bool {
        self.active_state == "active" || self.substate == "running"
    }
}

pub fn services_page() -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Service Doctor"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let refresh_btn = gtk::Button::with_label("Refresh Services");
    refresh_btn.add_css_class("pill");
    title_row.append(&title);
    title_row.append(&refresh_btn);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Diagnose failed/restarting systemd services before changing them. User-session Start/Restart controls remain available; Stop requires an explicit second click. LinuxCare never disables system services automatically.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&container);

    let render = {
        let container = container.clone();
        let refresh_btn = refresh_btn.clone();
        move || {
            refresh_btn.set_sensitive(false);
            refresh_btn.set_label("Checking…");
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }
            let loading = gtk::Label::new(Some("Querying systemd service health and user units…"));
            loading.set_xalign(0.0);
            loading.add_css_class("dim-label");
            container.append(&loading);

            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let doctor = crate::service_doctor::collect_report();
                let services = collect_user_services();
                let _ = tx.send((doctor, services));
            });

            let container_ui = container.clone();
            let refresh_ui = refresh_btn.clone();
            glib::timeout_add_local(Duration::from_millis(70), move || match rx.try_recv() {
                Ok((doctor, services)) => {
                    while let Some(child) = container_ui.first_child() {
                        container_ui.remove(&child);
                    }
                    refresh_ui.set_sensitive(true);
                    refresh_ui.set_label("Refresh Services");
                    container_ui.append(&doctor_summary(&doctor));
                    container_ui.append(&doctor_signals(&doctor));
                    container_ui.append(&user_services_card(&services));
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    refresh_ui.set_sensitive(true);
                    refresh_ui.set_label("Refresh Services");
                    glib::ControlFlow::Break
                }
            });
        }
    };

    let render_button = render.clone();
    refresh_btn.connect_clicked(move |_| render_button());
    render();

    let clamp = adw::Clamp::builder()
        .maximum_size(980)
        .tightening_threshold(720)
        .child(&page)
        .build();
    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn collect_user_services() -> Vec<UserService> {
    let Ok(output) = Command::new("systemctl")
        .args([
            "--user",
            "list-units",
            "--type=service",
            "--all",
            "--no-legend",
            "--plain",
            "--no-pager",
        ])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 4 {
                return None;
            }
            Some(UserService {
                name: parts[0].to_string(),
                active_state: parts[2].to_string(),
                substate: parts[3].to_string(),
                description: if parts.len() > 4 {
                    parts[4..].join(" ")
                } else {
                    String::new()
                },
            })
        })
        .collect()
}

fn doctor_summary(report: &crate::service_doctor::ServiceDoctorReport) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("hero-card");
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some("systemd Health"));
    title.set_xalign(0.0);
    title.add_css_class("title-2");
    let summary = gtk::Label::new(Some(&report.summary));
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.add_css_class("dim-label");
    text.append(&title);
    text.append(&summary);
    top.append(&text);
    let badge = gtk::Label::new(Some(&report.status_label.to_uppercase()));
    badge.add_css_class("risk-badge");
    badge.add_css_class(
        if !report.restarting_system.is_empty() || !report.restarting_user.is_empty() {
            "risk-dangerous"
        } else if !report.failed_system.is_empty() || !report.failed_user.is_empty() {
            "risk-review"
        } else if !report.system_scope_available || !report.user_scope_available {
            "health-neutral"
        } else {
            "risk-safe"
        },
    );
    top.append(&badge);
    card.append(&top);

    let metrics = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    metrics.set_homogeneous(true);
    metrics.append(&metric(
        "FAILED SYSTEM",
        &report.failed_system.len().to_string(),
    ));
    metrics.append(&metric(
        "FAILED USER",
        &report.failed_user.len().to_string(),
    ));
    metrics.append(&metric(
        "RESTARTING",
        &(report.restarting_system.len() + report.restarting_user.len()).to_string(),
    ));
    metrics.append(&metric(
        "ENABLED SYSTEM",
        &report
            .enabled_system_count
            .map(|value| value.to_string())
            .unwrap_or_else(|| "?".to_string()),
    ));
    card.append(&metrics);
    let metrics2 = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    metrics2.set_homogeneous(true);
    metrics2.append(&metric(
        "ENABLED USER",
        &report
            .enabled_user_count
            .map(|value| value.to_string())
            .unwrap_or_else(|| "?".to_string()),
    ));
    metrics2.append(&metric(
        "MASKED SYSTEM",
        &report
            .masked_system_count
            .map(|value| value.to_string())
            .unwrap_or_else(|| "?".to_string()),
    ));
    metrics2.append(&metric(
        "SYSTEM SCOPE",
        if report.system_scope_available {
            "Available"
        } else {
            "Unknown"
        },
    ));
    metrics2.append(&metric(
        "USER SCOPE",
        if report.user_scope_available {
            "Available"
        } else {
            "Unknown"
        },
    ));
    card.append(&metrics2);
    card.upcast()
}

fn doctor_signals(report: &crate::service_doctor::ServiceDoctorReport) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 7);
    card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("Diagnostic Signals"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);
    for signal in &report.signals {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.add_css_class("health-signal-row");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let title = gtk::Label::new(Some(&signal.title));
        title.set_xalign(0.0);
        title.add_css_class("heading");
        let detail = gtk::Label::new(Some(&signal.detail));
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        detail.add_css_class("dim-label");
        text.append(&title);
        text.append(&detail);
        row.append(&text);
        let badge = gtk::Label::new(Some(signal.state.label()));
        badge.add_css_class("risk-badge");
        badge.add_css_class(signal.state.css_class());
        row.append(&badge);
        card.append(&row);
    }
    card.upcast()
}

fn user_services_card(services: &[UserService]) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("User-session Services"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);
    let note = gtk::Label::new(Some(
        "Controls below affect only --user services. They do not enable/disable startup policy.",
    ));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.add_css_class("dim-label");
    card.append(&note);

    if services.is_empty() {
        let empty = gtk::Label::new(Some("No user service units were returned by systemctl."));
        empty.set_xalign(0.0);
        empty.add_css_class("dim-label");
        card.append(&empty);
        return card.upcast();
    }

    for service in services.iter().take(30) {
        card.append(&user_service_row(service));
    }
    card.upcast()
}

fn user_service_row(service: &UserService) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("health-signal-row");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(&service.name));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    let detail_text = if service.description.is_empty() {
        format!("{} / {}", service.active_state, service.substate)
    } else {
        format!(
            "{} • {} / {}",
            service.description, service.active_state, service.substate
        )
    };
    let detail = gtk::Label::new(Some(&detail_text));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    text.append(&title);
    text.append(&detail);
    row.append(&text);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    if service.is_active() {
        let restart = gtk::Button::with_label("Restart");
        restart.add_css_class("pill");
        bind_user_service_action(&restart, "restart", &service.name, false);
        actions.append(&restart);

        let stop = gtk::Button::with_label("Stop");
        stop.add_css_class("pill");
        stop.add_css_class("destructive-action");
        bind_user_service_action(&stop, "stop", &service.name, true);
        actions.append(&stop);
    } else {
        let start = gtk::Button::with_label("Start");
        start.add_css_class("pill");
        start.add_css_class("suggested-action");
        bind_user_service_action(&start, "start", &service.name, false);
        actions.append(&start);
    }
    row.append(&actions);
    row.upcast()
}

fn bind_user_service_action(button: &gtk::Button, action: &'static str, unit: &str, confirm: bool) {
    let unit = unit.to_string();
    let armed = Rc::new(Cell::new(false));
    button.connect_clicked(move |button| {
        if confirm && !armed.get() {
            armed.set(true);
            button.set_label("Confirm Stop");
            return;
        }
        button.set_sensitive(false);
        button.set_label(match action {
            "start" => "Starting…",
            "stop" => "Stopping…",
            _ => "Restarting…",
        });
        let target = unit.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let ok = Command::new("systemctl")
                .args(["--user", action, &target])
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false);
            let _ = tx.send(ok);
        });
        let button_ui = button.clone();
        glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
            Ok(true) => {
                button_ui.set_label(match action {
                    "start" => "Started",
                    "stop" => "Stopped",
                    _ => "Restarted",
                });
                glib::ControlFlow::Break
            }
            Ok(false) => {
                button_ui.set_label("Action failed");
                button_ui.set_sensitive(true);
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                button_ui.set_label("Action failed");
                button_ui.set_sensitive(true);
                glib::ControlFlow::Break
            }
        });
    });
}

fn metric(title: &str, value: &str) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 3);
    box_.add_css_class("health-domain-card");
    let title = gtk::Label::new(Some(title));
    title.add_css_class("caption-heading");
    title.add_css_class("dim-label");
    let value = gtk::Label::new(Some(value));
    value.add_css_class("heading");
    box_.append(&title);
    box_.append(&value);
    box_.upcast()
}
