use crate::{filesystem_doctor::FilesystemDoctorReport, format};
use gtk::prelude::*;
use std::{sync::mpsc, time::Duration};

pub fn filesystem_doctor_page() -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Filesystem Doctor"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let refresh = gtk::Button::with_label("Refresh Filesystems");
    refresh.add_css_class("pill");
    title_row.append(&title);
    title_row.append(&refresh);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Capacity, inode pressure, read-only state, current-boot filesystem errors, and snapshot awareness. LinuxCare does not run fsck or mutate snapshots automatically.",
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
            refresh.set_label("Checking…");
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }
            let loading = gtk::Label::new(Some(
                "Reading mountinfo, statvfs and current-boot kernel errors…",
            ));
            loading.set_xalign(0.0);
            loading.set_wrap(true);
            loading.add_css_class("dim-label");
            container.append(&loading);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::filesystem_doctor::collect_report());
            });
            let container_ui = container.clone();
            let refresh_ui = refresh.clone();
            glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                Ok(report) => {
                    while let Some(child) = container_ui.first_child() {
                        container_ui.remove(&child);
                    }
                    refresh_ui.set_sensitive(true);
                    refresh_ui.set_label("Refresh Filesystems");
                    render_report(&container_ui, &report);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    refresh_ui.set_sensitive(true);
                    refresh_ui.set_label("Refresh Filesystems");
                    glib::ControlFlow::Break
                }
            });
        }
    };

    let render_button = render.clone();
    refresh.connect_clicked(move |_| render_button());
    render();

    let clamp = adw::Clamp::builder()
        .maximum_size(1040)
        .tightening_threshold(760)
        .child(&page)
        .build();
    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn render_report(container: &gtk::Box, report: &FilesystemDoctorReport) {
    let summary = gtk::Box::new(gtk::Orientation::Vertical, 8);
    summary.add_css_class("hero-card");
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let title = gtk::Label::new(Some("Filesystem Health"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-2");
    let badge = gtk::Label::new(Some(&report.status_label.to_uppercase()));
    badge.add_css_class("risk-badge");
    badge.add_css_class(if !report.kernel_errors_available {
        "health-neutral"
    } else if report.kernel_error_lines.is_empty()
        && report
            .mounts
            .iter()
            .all(|mount| mount.state == crate::filesystem_doctor::FilesystemState::Good)
    {
        "risk-safe"
    } else {
        "risk-review"
    });
    top.append(&title);
    top.append(&badge);
    summary.append(&top);
    let detail = gtk::Label::new(Some(&report.summary));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    summary.append(&detail);
    container.append(&summary);

    let mounts_card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    mounts_card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("Persistent Mounts"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    mounts_card.append(&title);
    for mount in &report.mounts {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.add_css_class("health-signal-row");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let name = gtk::Label::new(Some(&format!(
            "{} • {}",
            mount.mount_point.display(),
            mount.fs_type
        )));
        name.set_xalign(0.0);
        name.add_css_class("heading");
        let mut parts = vec![mount.source.clone(), mount.detail.clone()];
        if let Some(free) = mount.free_bytes {
            parts.push(format!("{} free", format::bytes(free)));
        }
        if let Some(inodes) = mount.inode_used_percent {
            parts.push(format!("{inodes:.0}% inodes used"));
        }
        let detail = gtk::Label::new(Some(&parts.join(" • ")));
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        detail.add_css_class("dim-label");
        text.append(&name);
        text.append(&detail);
        row.append(&text);
        let badge = gtk::Label::new(Some(mount.state.label()));
        badge.add_css_class("risk-badge");
        badge.add_css_class(mount.state.css_class());
        badge.set_valign(gtk::Align::Center);
        row.append(&badge);
        mounts_card.append(&row);
    }
    container.append(&mounts_card);

    let snapshot = &report.snapshots;
    let snapshot_card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    snapshot_card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("Snapshot Awareness"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    snapshot_card.append(&title);
    let detail = gtk::Label::new(Some(&format!(
        "Btrfs mounts: {} • Snapper: {}{} • Timeshift: {} • snapshot directories: {}",
        snapshot.btrfs_mounts.len(),
        if snapshot.snapper_available {
            "detected"
        } else {
            "not detected"
        },
        snapshot
            .snapper_snapshot_count
            .map(|count| format!(" ({count} listed)"))
            .unwrap_or_default(),
        if snapshot.timeshift_detected {
            "detected"
        } else {
            "not detected"
        },
        snapshot.snapshot_directories.len()
    )));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    snapshot_card.append(&detail);
    let warning = gtk::Label::new(Some(
        "LinuxCare only detects snapshot tooling/state here. It does not create, delete, restore or prune snapshots in this preview.",
    ));
    warning.set_xalign(0.0);
    warning.set_wrap(true);
    warning.add_css_class("dim-label");
    snapshot_card.append(&warning);
    container.append(&snapshot_card);

    if !report.kernel_error_lines.is_empty() {
        let errors = gtk::Box::new(gtk::Orientation::Vertical, 6);
        errors.add_css_class("warning-card");
        let title = gtk::Label::new(Some("Current-boot filesystem-related kernel errors"));
        title.set_xalign(0.0);
        title.add_css_class("title-3");
        errors.append(&title);
        for line in &report.kernel_error_lines {
            let label = gtk::Label::new(Some(line));
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.add_css_class("monospace");
            errors.append(&label);
        }
        container.append(&errors);
    }
}
