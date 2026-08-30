use crate::{
    format,
    package_doctor::{DiagnosticSignal, DiagnosticState, PackageDoctorReport},
    providers::{HelperStatus, PackageProvider},
};
use gtk::prelude::*;
use std::{cell::Cell, rc::Rc, sync::mpsc, time::Duration};

pub fn packages_page() -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Package Doctor 2.0"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let refresh_btn = gtk::Button::with_label("Run Package Check");
    refresh_btn.add_css_class("pill");
    title_row.append(&title);
    title_row.append(&refresh_btn);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "APT/dpkg health plus duplicate sources, metadata age, additional kernel images and foreign architectures. Diagnostics stay read-only; privileged cleanup remains explicit and Polkit-authorized.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.append(&container);

    let container_c = container.clone();
    let refresh_btn_c = refresh_btn.clone();

    let render_page = move || {
        refresh_btn_c.set_sensitive(false);
        refresh_btn_c.set_label("Checking…");
        while let Some(child) = container_c.first_child() {
            container_c.remove(&child);
        }

        let loading = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        loading.add_css_class("package-doctor-card");
        let spinner = gtk::Spinner::new();
        spinner.start();
        loading.append(&spinner);
        let loading_label = gtk::Label::new(Some(
            "Running apt-get check, dpkg audit, and read-only APT simulations…",
        ));
        loading_label.set_wrap(true);
        loading_label.set_xalign(0.0);
        loading.append(&loading_label);
        container_c.append(&loading);

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let report = crate::package_doctor::collect_report();
            let helper = crate::providers::check_helper_status();
            let _ = tx.send((report, helper));
        });

        let container_ui = container_c.clone();
        let refresh_btn_ui = refresh_btn_c.clone();

        glib::timeout_add_local(Duration::from_millis(70), move || match rx.try_recv() {
            Ok((report, helper_status)) => {
                while let Some(child) = container_ui.first_child() {
                    container_ui.remove(&child);
                }
                refresh_btn_ui.set_sensitive(true);
                refresh_btn_ui.set_label("Run Package Check");

                container_ui.append(&doctor_summary_card(&report));

                let signals_card = gtk::Box::new(gtk::Orientation::Vertical, 8);
                signals_card.add_css_class("package-doctor-card");
                let heading = gtk::Label::new(Some("Diagnostic Signals"));
                heading.set_xalign(0.0);
                heading.add_css_class("title-3");
                signals_card.append(&heading);
                for signal in &report.signals {
                    signals_card.append(&diagnostic_row(signal));
                }
                container_ui.append(&signals_card);

                container_ui.append(&package_inventory_card(&report));
                container_ui.append(&repository_kernel_card(&report));
                container_ui.append(&maintenance_card(&report, &helper_status));
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                while let Some(child) = container_ui.first_child() {
                    container_ui.remove(&child);
                }
                refresh_btn_ui.set_sensitive(true);
                refresh_btn_ui.set_label("Run Package Check");
                let warning = gtk::Label::new(Some(
                    "Package Doctor worker stopped before returning a report.",
                ));
                warning.set_wrap(true);
                warning.set_xalign(0.0);
                warning.add_css_class("dim-label");
                container_ui.append(&warning);
                glib::ControlFlow::Break
            }
        });
    };

    let render_c = render_page.clone();
    refresh_btn.connect_clicked(move |_| render_c());
    render_page();

    let clamp = adw::Clamp::builder()
        .maximum_size(980)
        .tightening_threshold(720)
        .child(&page)
        .build();

    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn doctor_summary_card(report: &PackageDoctorReport) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("package-doctor-card");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let icon = gtk::Image::from_icon_name("system-software-update-symbolic");
    icon.set_pixel_size(30);
    header.append(&icon);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some("Package Database Health"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    let summary = gtk::Label::new(Some(&report.summary));
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.add_css_class("dim-label");
    text.append(&title);
    text.append(&summary);
    header.append(&text);

    let badge = gtk::Label::new(Some(&report.overall_label.to_uppercase()));
    badge.add_css_class("risk-badge");
    badge.add_css_class(
        if report
            .signals
            .iter()
            .any(|signal| signal.state == DiagnosticState::Review)
        {
            "risk-review"
        } else if report
            .signals
            .iter()
            .any(|signal| signal.state == DiagnosticState::Unknown)
        {
            "health-neutral"
        } else {
            "risk-safe"
        },
    );
    badge.set_valign(gtk::Align::Center);
    header.append(&badge);
    card.append(&header);

    let metrics = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    metrics.set_homogeneous(true);
    metrics.append(&metric_box(
        "UPGRADES",
        &report.upgrade_candidates.len().to_string(),
    ));
    metrics.append(&metric_box(
        "AUTOREMOVE",
        &report.autoremove_candidates.len().to_string(),
    ));
    metrics.append(&metric_box("HELD", &report.held_packages.len().to_string()));
    metrics.append(&metric_box(
        "APT SOURCES",
        &report.active_source_entries.to_string(),
    ));
    card.append(&metrics);

    let metrics2 = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    metrics2.set_homogeneous(true);
    metrics2.append(&metric_box(
        "DUPLICATES",
        &report.duplicate_source_entries.len().to_string(),
    ));
    metrics2.append(&metric_box(
        "METADATA AGE",
        &report
            .apt_metadata_age_days
            .map(|days| format!("{days}d"))
            .unwrap_or_else(|| "?".to_string()),
    ));
    metrics2.append(&metric_box(
        "EXTRA KERNELS",
        &report.old_kernel_packages.len().to_string(),
    ));
    metrics2.append(&metric_box(
        "FOREIGN ARCH",
        &report.foreign_architectures.len().to_string(),
    ));
    card.append(&metrics2);

    card.upcast()
}

fn diagnostic_row(signal: &DiagnosticSignal) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("health-signal-row");

    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
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
    badge.set_valign(gtk::Align::Center);
    row.append(&badge);
    row.upcast()
}

fn package_inventory_card(report: &PackageDoctorReport) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("package-doctor-card");

    let title = gtk::Label::new(Some("Package Inventory Signals"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);

    card.append(&inventory_row(
        "Upgrade candidates",
        report.upgrade_candidates.len(),
        &report.upgrade_candidates,
        "Based on currently cached APT metadata; LinuxCare does not run apt update automatically.",
    ));
    card.append(&inventory_row(
        "Autoremove candidates",
        report.autoremove_candidates.len(),
        &report.autoremove_candidates,
        "APT simulation only. Removal still requires explicit confirmation and Polkit authorization.",
    ));
    card.append(&inventory_row(
        "Held packages",
        report.held_packages.len(),
        &report.held_packages,
        "Held packages are not automatically considered broken; review whether the hold is intentional.",
    ));

    card.upcast()
}

fn inventory_row(title: &str, count: usize, names: &[String], note: &str) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 3);
    row.add_css_class("package-inventory-row");
    let heading = gtk::Label::new(Some(&format!("{title} • {count}")));
    heading.set_xalign(0.0);
    heading.add_css_class("heading");
    row.append(&heading);

    if !names.is_empty() {
        let mut preview = names.iter().take(8).cloned().collect::<Vec<_>>().join(", ");
        if names.len() > 8 {
            preview.push_str(&format!(" +{} more", names.len() - 8));
        }
        let packages = gtk::Label::new(Some(&preview));
        packages.set_xalign(0.0);
        packages.set_wrap(true);
        packages.add_css_class("monospace");
        row.append(&packages);
    }

    let note_label = gtk::Label::new(Some(note));
    note_label.set_xalign(0.0);
    note_label.set_wrap(true);
    note_label.add_css_class("dim-label");
    row.append(&note_label);
    row.upcast()
}

fn repository_kernel_card(report: &PackageDoctorReport) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("package-doctor-card");
    let title = gtk::Label::new(Some("Repository & Kernel Context"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);

    if report.duplicate_source_entries.is_empty() {
        card.append(&inventory_row(
            "Exact duplicate sources",
            0,
            &[],
            "No exact duplicate active source definitions were found across classic and deb822 source files.",
        ));
    } else {
        card.append(&inventory_row(
            "Exact duplicate sources",
            report.duplicate_source_entries.len(),
            &report.duplicate_source_entries,
            "Review before editing. LinuxCare intentionally does not rewrite APT source files.",
        ));
    }

    card.append(&inventory_row(
        "Additional versioned kernel images",
        report.old_kernel_packages.len(),
        &report.old_kernel_packages,
        "These are non-running versioned linux-image packages, not automatically safe-to-remove kernels. Keep an appropriate fallback kernel.",
    ));
    card.append(&inventory_row(
        "Foreign dpkg architectures",
        report.foreign_architectures.len(),
        &report.foreign_architectures,
        "Foreign architectures can be intentional for multiarch software such as 32-bit compatibility.",
    ));
    card.upcast()
}

fn maintenance_card(report: &PackageDoctorReport, helper_status: &HelperStatus) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("package-doctor-card");

    let title = gtk::Label::new(Some("Explicit Maintenance Actions"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    card.append(&title);

    let helper_available = matches!(helper_status, HelperStatus::Available { .. });
    let helper_text = match helper_status {
        HelperStatus::Available { version } => {
            format!("Privileged helper available • version {version}")
        }
        HelperStatus::NotInstalled => {
            "Privileged helper is not installed. Diagnostics still work; cleanup actions are disabled."
                .to_string()
        }
        HelperStatus::ServiceUnavailable(error) => format!(
            "Privileged helper is installed but unavailable: {error}. Diagnostics still work."
        ),
    };
    let helper_label = gtk::Label::new(Some(&helper_text));
    helper_label.set_xalign(0.0);
    helper_label.set_wrap(true);
    helper_label.add_css_class(if helper_available {
        "dim-label"
    } else {
        "warning-card"
    });
    card.append(&helper_label);

    let apt_row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    apt_row.add_css_class("finding-card");
    let apt_info = gtk::Box::new(gtk::Orientation::Vertical, 3);
    apt_info.set_hexpand(true);
    let apt_title = gtk::Label::new(Some("APT Package Cache"));
    apt_title.set_xalign(0.0);
    apt_title.add_css_class("heading");
    let apt_sub = gtk::Label::new(Some(&format!(
        "/var/cache/apt/archives • {}",
        format::bytes(report.apt_cache_bytes)
    )));
    apt_sub.set_xalign(0.0);
    apt_sub.add_css_class("dim-label");
    apt_info.append(&apt_title);
    apt_info.append(&apt_sub);
    apt_row.append(&apt_info);

    let clean_btn = gtk::Button::with_label("Clean APT Cache");
    clean_btn.add_css_class("pill");
    clean_btn.add_css_class("suggested-action");
    clean_btn.set_sensitive(helper_available);
    let apt_sub_c = apt_sub.clone();
    clean_btn.connect_clicked(move |btn| {
        btn.set_sensitive(false);
        btn.set_label("Cleaning…");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::providers::UbuntuSystemProvider::connect()
                .and_then(|provider| provider.clean_package_cache());
            let _ = tx.send(result);
        });
        let btn_c = btn.clone();
        let apt_sub_cc = apt_sub_c.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
            Ok(Ok(recovered)) => {
                btn_c.set_label("Cleaned");
                apt_sub_cc.set_text(&format!(
                    "APT cache • Recovered {}",
                    format::bytes(recovered)
                ));
                glib::ControlFlow::Break
            }
            Ok(Err(error)) => {
                btn_c.set_label("Failed");
                btn_c.set_tooltip_text(Some(&error.to_string()));
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                btn_c.set_label("Failed");
                glib::ControlFlow::Break
            }
        });
    });
    apt_row.append(&clean_btn);
    card.append(&apt_row);

    let auto_row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    auto_row.add_css_class("finding-card");
    let auto_info = gtk::Box::new(gtk::Orientation::Vertical, 3);
    auto_info.set_hexpand(true);
    let auto_title = gtk::Label::new(Some("APT Autoremove"));
    auto_title.set_xalign(0.0);
    auto_title.add_css_class("heading");
    let auto_sub = gtk::Label::new(Some(&format!(
        "{} simulated candidate(s). LinuxCare never treats autoremove as mandatory.",
        report.autoremove_candidates.len()
    )));
    auto_sub.set_xalign(0.0);
    auto_sub.set_wrap(true);
    auto_sub.add_css_class("dim-label");
    auto_info.append(&auto_title);
    auto_info.append(&auto_sub);
    auto_row.append(&auto_info);

    let autoremove_btn = gtk::Button::with_label("Run Autoremove");
    autoremove_btn.add_css_class("pill");
    autoremove_btn.set_sensitive(helper_available && !report.autoremove_candidates.is_empty());
    let armed = Rc::new(Cell::new(false));
    let armed_c = armed.clone();
    let auto_sub_c = auto_sub.clone();
    autoremove_btn.connect_clicked(move |btn| {
        if !armed_c.replace(true) {
            btn.set_label("Confirm Autoremove");
            auto_sub_c.set_text(
                "Click again to authorize removal of packages from APT's current autoremove set.",
            );
            return;
        }

        armed_c.set(false);
        btn.set_sensitive(false);
        btn.set_label("Running…");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::providers::UbuntuSystemProvider::connect()
                .and_then(|provider| provider.autoremove_packages());
            let _ = tx.send(result);
        });
        let btn_c = btn.clone();
        let auto_sub_cc = auto_sub_c.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
            Ok(Ok(recovered)) => {
                btn_c.set_label("Done");
                let message = if recovered > 0 {
                    format!(
                        "Autoremove complete • Recovered {}",
                        format::bytes(recovered)
                    )
                } else {
                    "Autoremove complete. APT did not provide a reliable reclaimed-size total."
                        .to_string()
                };
                auto_sub_cc.set_text(&message);
                glib::ControlFlow::Break
            }
            Ok(Err(error)) => {
                btn_c.set_label("Failed");
                btn_c.set_tooltip_text(Some(&error.to_string()));
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                btn_c.set_label("Failed");
                glib::ControlFlow::Break
            }
        });
    });
    auto_row.append(&autoremove_btn);
    card.append(&auto_row);

    card.upcast()
}

fn metric_box(title: &str, value: &str) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 3);
    box_.add_css_class("package-metric");
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("caption-heading");
    let value_label = gtk::Label::new(Some(value));
    value_label.add_css_class("title-2");
    box_.append(&title_label);
    box_.append(&value_label);
    box_.upcast()
}
