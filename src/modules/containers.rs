use crate::{container_analyzer::ContainerEngineReport, format};
use gtk::prelude::*;
use std::{sync::mpsc, time::Duration};

pub fn container_analyzer_page() -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Docker / Podman Analyzer"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let refresh_btn = gtk::Button::with_label("Refresh Containers");
    refresh_btn.add_css_class("pill");
    title_row.append(&title);
    title_row.append(&refresh_btn);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Read-only container storage analysis. LinuxCare does not run prune, remove images, delete volumes, or stop containers from this page.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.append(&container);

    let container_c = container.clone();
    let refresh_btn_c = refresh_btn.clone();
    let render = move || {
        refresh_btn_c.set_sensitive(false);
        refresh_btn_c.set_label("Analyzing…");
        while let Some(child) = container_c.first_child() {
            container_c.remove(&child);
        }
        let loading = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        loading.add_css_class("finding-card");
        let spinner = gtk::Spinner::new();
        spinner.start();
        loading.append(&spinner);
        let label = gtk::Label::new(Some("Querying Docker and Podman storage metadata…"));
        label.set_xalign(0.0);
        loading.append(&label);
        container_c.append(&loading);

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::container_analyzer::collect_report());
        });

        let container_ui = container_c.clone();
        let button_ui = refresh_btn_c.clone();
        glib::timeout_add_local(Duration::from_millis(70), move || match rx.try_recv() {
            Ok(report) => {
                while let Some(child) = container_ui.first_child() {
                    container_ui.remove(&child);
                }
                button_ui.set_sensitive(true);
                button_ui.set_label("Refresh Containers");

                let installed = report
                    .engines
                    .iter()
                    .filter(|engine| engine.installed)
                    .count();
                let reachable = report
                    .engines
                    .iter()
                    .filter(|engine| engine.reachable)
                    .count();
                let reclaimable: u64 = report
                    .engines
                    .iter()
                    .filter(|engine| engine.reachable)
                    .map(ContainerEngineReport::reclaimable_bytes)
                    .sum();
                container_ui.append(&summary_card(installed, reachable, reclaimable));
                for engine in &report.engines {
                    container_ui.append(&engine_card(engine));
                }
                container_ui.append(&safety_card());
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                while let Some(child) = container_ui.first_child() {
                    container_ui.remove(&child);
                }
                button_ui.set_sensitive(true);
                button_ui.set_label("Refresh Containers");
                let warning = gtk::Label::new(Some(
                    "Container Analyzer worker stopped before returning a report.",
                ));
                warning.set_xalign(0.0);
                warning.add_css_class("dim-label");
                container_ui.append(&warning);
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

fn summary_card(installed: usize, reachable: usize, reclaimable: u64) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("hero-card");
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let icon = gtk::Image::from_icon_name("drive-harddisk-symbolic");
    icon.set_pixel_size(32);
    header.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some("Container Storage Overview"));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    let detail = gtk::Label::new(Some(&format!(
        "{installed} engine(s) installed • {reachable} reachable • about {} reported reclaimable",
        format::bytes(reclaimable)
    )));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    text.append(&title);
    text.append(&detail);
    header.append(&text);
    card.append(&header);
    card.upcast()
}

fn engine_card(engine: &ContainerEngineReport) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("finding-card");
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(engine.kind.label()));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    let version = engine.version.as_deref().unwrap_or("version unknown");
    let root = engine
        .storage_root
        .as_deref()
        .unwrap_or("storage root unknown");
    let detail = gtk::Label::new(Some(&format!("{version} • {root}")));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    text.append(&title);
    text.append(&detail);
    header.append(&text);

    let (badge_text, badge_class) = if !engine.installed {
        ("NOT INSTALLED", "health-neutral")
    } else if engine.reachable {
        ("READY", "risk-safe")
    } else {
        ("UNAVAILABLE", "risk-review")
    };
    let badge = gtk::Label::new(Some(badge_text));
    badge.add_css_class("risk-badge");
    badge.add_css_class(badge_class);
    badge.set_valign(gtk::Align::Center);
    header.append(&badge);
    card.append(&header);

    if !engine.installed {
        card.append(&dim_label(&format!(
            "{} CLI was not detected. Nothing is treated as an error if you do not use this engine.",
            engine.kind.label()
        )));
        return card.upcast();
    }

    if !engine.reachable {
        card.append(&dim_label(engine.error.as_deref().unwrap_or(
            "The engine is installed, but LinuxCare could not query its storage service.",
        )));
        return card.upcast();
    }

    let metrics = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    metrics.set_homogeneous(true);
    metrics.append(&metric(
        "REPORTED SIZE",
        &format::bytes(engine.total_size_bytes()),
    ));
    metrics.append(&metric(
        "RECLAIMABLE EST.",
        &format::bytes(engine.reclaimable_bytes()),
    ));
    metrics.append(&metric("CATEGORIES", &engine.categories.len().to_string()));
    card.append(&metrics);

    for category in &engine.categories {
        let total = category
            .total_items
            .map(|value| value.to_string())
            .unwrap_or_else(|| "?".to_string());
        let active = category
            .active_items
            .map(|value| value.to_string())
            .unwrap_or_else(|| "?".to_string());
        let size = category
            .size_bytes
            .map(format::bytes)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| category.raw_size.clone());
        let reclaimable = category
            .reclaimable_bytes
            .map(format::bytes)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| category.raw_reclaimable.clone());
        card.append(&detail_row(
            if category.name.is_empty() {
                "Container storage"
            } else {
                &category.name
            },
            &format!(
                "{total} total • {active} active • {size} used • {reclaimable} reclaimable estimate"
            ),
        ));
    }
    card.upcast()
}

fn safety_card() -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 7);
    card.add_css_class("finding-card");
    let title = gtk::Label::new(Some("Safety Boundary"));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    let detail = gtk::Label::new(Some(
        "Reclaimable values are engine estimates, not a promise of bytes that a prune will free. Shared image layers and active references can change the real result. LinuxCare alpha.6 intentionally provides no prune button.",
    ));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    card.append(&title);
    card.append(&detail);
    card.upcast()
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
