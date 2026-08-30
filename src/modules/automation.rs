use crate::{automation_engine, cleanup_policy, notification_center};
use gtk::glib::prelude::IsA;
use gtk::prelude::*;
use std::{path::PathBuf, rc::Rc};

pub fn automation_page(home: PathBuf) -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title = gtk::Label::new(Some("Automation & Notifications"));
    title.set_xalign(0.0);
    title.add_css_class("title-1");
    page.append(&title);

    let subtitle = gtk::Label::new(Some(
        "Real background maintenance uses a user-level systemd timer. LinuxCare never runs privileged cleanup unattended; automated cleanup is restricted to SAFE user-space items and Safety Quarantine.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let scheduler = scheduler_card(home.clone());
    page.append(&scheduler);
    page.append(&cleanup_policy_card(home.clone()));
    page.append(&notification_card(home));

    let clamp = adw::Clamp::builder()
        .maximum_size(960)
        .tightening_threshold(720)
        .child(&page)
        .build();
    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn scheduler_card(home: PathBuf) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("finding-card");

    let title = gtk::Label::new(Some("Scheduled Maintenance"));
    title.set_xalign(0.0);
    title.add_css_class("title-2");
    card.append(&title);

    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.add_css_class("dim-label");
    card.append(&status);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let cadence = gtk::DropDown::from_strings(&["Daily", "Weekly"]);
    let weekday = gtk::DropDown::from_strings(&["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
    let hour = gtk::SpinButton::with_range(0.0, 23.0, 1.0);
    let minute = gtk::SpinButton::with_range(0.0, 59.0, 5.0);
    let mode =
        gtk::DropDown::from_strings(&["Scan only", "Scan + notify", "Scan + safe quarantine"]);
    row.append(&field("Cadence", &cadence));
    row.append(&field("Weekday", &weekday));
    row.append(&field("Hour", &hour));
    row.append(&field("Minute", &minute));
    row.append(&field("Mode", &mode));
    card.append(&row);

    let current = automation_engine::load(&home).unwrap_or_default();
    cadence.set_selected(match current.cadence {
        automation_engine::AutomationCadence::Daily => 0,
        automation_engine::AutomationCadence::Weekly => 1,
    });
    weekday.set_selected(weekday_index(&current.weekday));
    hour.set_value(current.hour as f64);
    minute.set_value(current.minute as f64);
    mode.set_selected(match current.mode {
        automation_engine::AutomationMode::ScanOnly => 0,
        automation_engine::AutomationMode::ScanAndNotify => 1,
        automation_engine::AutomationMode::SafeQuarantine => 2,
    });

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let enable = gtk::Button::with_label("Save & Enable Timer");
    enable.add_css_class("suggested-action");
    enable.add_css_class("pill");
    let run_now = gtk::Button::with_label("Run Now");
    run_now.add_css_class("pill");
    let disable = gtk::Button::with_label("Disable Timer");
    disable.add_css_class("pill");
    buttons.append(&enable);
    buttons.append(&run_now);
    buttons.append(&disable);
    card.append(&buttons);

    let refresh_status: Rc<dyn Fn()> = Rc::new({
        let home = home.clone();
        let status = status.clone();
        let run_now = run_now.clone();
        move || {
            let state = automation_engine::status(&home);
            let next = state.next_run.as_deref().unwrap_or("not scheduled");
            let last = state
                .last_trigger
                .as_deref()
                .unwrap_or("no recorded trigger");
            status.set_text(&format!(
                "{} {}\nNext: {next}\nLast trigger: {last}",
                if state.enabled && state.active {
                    "ACTIVE •"
                } else {
                    "INACTIVE •"
                },
                state.detail
            ));
            run_now.set_sensitive(state.enabled && state.timer_path.is_some());
        }
    });
    refresh_status();

    {
        let home = home.clone();
        let cadence = cadence.clone();
        let weekday = weekday.clone();
        let hour = hour.clone();
        let minute = minute.clone();
        let mode = mode.clone();
        let status = status.clone();
        let refresh_status = refresh_status.clone();
        enable.connect_clicked(move |_| {
            let config = automation_engine::AutomationConfig {
                enabled: true,
                cadence: if cadence.selected() == 0 {
                    automation_engine::AutomationCadence::Daily
                } else {
                    automation_engine::AutomationCadence::Weekly
                },
                weekday: weekday_name(weekday.selected()).to_string(),
                hour: hour.value_as_int().clamp(0, 23) as u8,
                minute: minute.value_as_int().clamp(0, 59) as u8,
                mode: match mode.selected() {
                    0 => automation_engine::AutomationMode::ScanOnly,
                    2 => automation_engine::AutomationMode::SafeQuarantine,
                    _ => automation_engine::AutomationMode::ScanAndNotify,
                },
            };
            match automation_engine::install_schedule(&home, &config) {
                Ok(()) => refresh_status(),
                Err(err) => status.set_text(&format!("Could not enable timer: {err}")),
            }
        });
    }
    {
        let status = status.clone();
        let refresh_status = refresh_status.clone();
        run_now.connect_clicked(move |_| match automation_engine::run_now() {
            Ok(()) => {
                status.set_text(
                    "Manual maintenance run started through the installed user systemd service.",
                );
                refresh_status();
            }
            Err(err) => status.set_text(&format!("Could not start maintenance now: {err}")),
        });
    }

    {
        let home = home.clone();
        let status = status.clone();
        let refresh_status = refresh_status.clone();
        disable.connect_clicked(move |_| match automation_engine::disable_schedule(&home) {
            Ok(()) => refresh_status(),
            Err(err) => status.set_text(&format!("Could not disable timer: {err}")),
        });
    }

    card.upcast()
}

fn cleanup_policy_card(home: PathBuf) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("Safe Cleanup 2.0 Policy"));
    title.set_xalign(0.0);
    title.add_css_class("title-2");
    let desc = gtk::Label::new(Some(
        "Controls how long Undo data is retained and caps how much storage Quarantine may consume. Automated cleanup has a separate smaller budget.",
    ));
    desc.set_xalign(0.0);
    desc.set_wrap(true);
    desc.add_css_class("dim-label");
    card.append(&title);
    card.append(&desc);

    let policy = cleanup_policy::load(&home).unwrap_or_default();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let retention = gtk::DropDown::from_strings(&["30 minutes", "1 hour", "1 day", "7 days"]);
    retention.set_selected(match policy.quarantine_retention_secs {
        1800 => 0,
        86400 => 2,
        604800 => 3,
        _ => 1,
    });
    let limit = gtk::SpinButton::with_range(1.0, 100.0, 1.0);
    limit.set_value((policy.quarantine_limit_bytes / (1024 * 1024 * 1024)).max(1) as f64);
    let auto_limit = gtk::SpinButton::with_range(0.1, 20.0, 0.1);
    auto_limit.set_value(policy.automation_max_bytes as f64 / (1024.0 * 1024.0 * 1024.0));
    let auto_purge = gtk::Switch::new();
    auto_purge.set_active(policy.auto_purge_expired);
    row.append(&field("Undo retention", &retention));
    row.append(&field("Quarantine cap (GiB)", &limit));
    row.append(&field("Automation cap (GiB)", &auto_limit));
    row.append(&field("Auto-purge expired", &auto_purge));
    card.append(&row);

    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.add_css_class("dim-label");
    let save = gtk::Button::with_label("Save Safety Policy");
    save.add_css_class("pill");
    let home_save = home.clone();
    let status_save = status.clone();
    save.connect_clicked(move |_| {
        let retention_secs = match retention.selected() {
            0 => 1800,
            2 => 86400,
            3 => 604800,
            _ => 3600,
        };
        let next = cleanup_policy::CleanupPolicy {
            quarantine_retention_secs: retention_secs,
            quarantine_limit_bytes: (limit.value().max(1.0) * 1024.0 * 1024.0 * 1024.0) as u64,
            automation_max_bytes: (auto_limit.value().max(0.1) * 1024.0 * 1024.0 * 1024.0) as u64,
            auto_purge_expired: auto_purge.is_active(),
        };
        match cleanup_policy::save(&home_save, &next) {
            Ok(()) => status_save.set_text("Safety policy saved."),
            Err(err) => status_save.set_text(&format!("Could not save policy: {err}")),
        }
    });
    card.append(&save);
    card.append(&status);
    card.upcast()
}

fn notification_card(home: PathBuf) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("finding-card");
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some("Notification Center"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-2");
    let test = gtk::Button::with_label("Test Alert");
    test.add_css_class("pill");
    let clear = gtk::Button::with_label("Clear History");
    clear.add_css_class("pill");
    top.append(&title);
    top.append(&test);
    top.append(&clear);
    card.append(&top);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 7);
    card.append(&list);
    let render: Rc<dyn Fn()> = Rc::new({
        let home = home.clone();
        let list = list.clone();
        move || {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let events = notification_center::load(&home).unwrap_or_default();
            if events.is_empty() {
                let label =
                    gtk::Label::new(Some("No LinuxCare notifications have been recorded yet."));
                label.set_xalign(0.0);
                label.add_css_class("dim-label");
                list.append(&label);
                return;
            }
            for event in events.iter().rev().take(8) {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
                text.set_hexpand(true);
                let heading = gtk::Label::new(Some(&event.title));
                heading.set_xalign(0.0);
                heading.add_css_class("heading");
                let detail = gtk::Label::new(Some(&event.body));
                detail.set_xalign(0.0);
                detail.set_wrap(true);
                detail.add_css_class("dim-label");
                text.append(&heading);
                text.append(&detail);
                row.append(&text);
                let badge = gtk::Label::new(Some(event.severity.label()));
                badge.add_css_class("risk-badge");
                badge.add_css_class(event.severity.css_class());
                row.append(&badge);
                list.append(&row);
            }
        }
    });
    render();
    {
        let home = home.clone();
        let render = render.clone();
        test.connect_clicked(move |_| {
            let _ = notification_center::emit(
                &home,
                "test",
                "LinuxCare notification test",
                "Desktop notifications and the local Notification Center are working.",
                notification_center::NotificationSeverity::Info,
                true,
            );
            render();
        });
    }
    {
        let home = home.clone();
        let render = render.clone();
        clear.connect_clicked(move |_| {
            let _ = notification_center::clear(&home);
            render();
        });
    }
    card.upcast()
}

fn field<W: IsA<gtk::Widget>>(label: &str, widget: &W) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
    box_.set_hexpand(true);
    let name = gtk::Label::new(Some(label));
    name.set_xalign(0.0);
    name.add_css_class("caption-heading");
    box_.append(&name);
    box_.append(widget);
    box_.upcast()
}

fn weekday_index(value: &str) -> u32 {
    match value {
        "Mon" => 0,
        "Tue" => 1,
        "Wed" => 2,
        "Thu" => 3,
        "Fri" => 4,
        "Sat" => 5,
        _ => 6,
    }
}

fn weekday_name(index: u32) -> &'static str {
    match index {
        0 => "Mon",
        1 => "Tue",
        2 => "Wed",
        3 => "Thu",
        4 => "Fri",
        5 => "Sat",
        _ => "Sun",
    }
}
