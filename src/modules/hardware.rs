use crate::{
    format,
    hardware::{HardwareReport, StorageDevice, StorageHealthReading},
    providers::PrivilegedClient,
};
use adw::prelude::*;
use gtk::glib;
use std::{cell::RefCell, rc::Rc, sync::mpsc, time::Duration};

pub fn hardware_doctor_page() -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Hardware Doctor"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let refresh_btn = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh hardware inventory")
        .css_classes(["flat"])
        .build();
    let smart_btn = gtk::Button::with_label("Read SMART / NVMe Health");
    smart_btn.add_css_class("pill");
    smart_btn.add_css_class("suggested-action");
    title_row.append(&title);
    title_row.append(&refresh_btn);
    title_row.append(&smart_btn);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Local hardware inventory, thermal sensors, physical storage, and authenticated low-level drive health. SMART reads are explicit and never run automatically.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let auth_note = gtk::Label::new(Some(
        "SMART/NVMe health uses the LinuxCare D-Bus helper with a dedicated Polkit read action. Serial numbers and WWNs are not returned to the GUI. Sleeping drives are queried with a no-wake policy when supported.",
    ));
    auth_note.set_xalign(0.0);
    auth_note.set_wrap(true);
    auth_note.add_css_class("dim-label");
    page.append(&auth_note);

    let inventory = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let smart_results = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&inventory);

    let smart_heading = gtk::Label::new(Some("Storage Health"));
    smart_heading.set_xalign(0.0);
    smart_heading.add_css_class("title-2");
    page.append(&smart_heading);
    page.append(&smart_results);

    let devices_state: Rc<RefCell<Vec<StorageDevice>>> = Rc::new(RefCell::new(Vec::new()));

    let refresh_inventory: Rc<dyn Fn()> = {
        let inventory = inventory.clone();
        let devices_state = devices_state.clone();
        let refresh_btn = refresh_btn.clone();
        let smart_btn = smart_btn.clone();
        Rc::new(move || {
            refresh_btn.set_sensitive(false);
            smart_btn.set_sensitive(false);
            clear_box(&inventory);
            let loading = gtk::Label::new(Some("Reading hardware inventory…"));
            loading.set_xalign(0.0);
            loading.add_css_class("dim-label");
            inventory.append(&loading);

            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::hardware::collect_report());
            });

            let inventory_i = inventory.clone();
            let devices_i = devices_state.clone();
            let refresh_i = refresh_btn.clone();
            let smart_i = smart_btn.clone();
            glib::timeout_add_local(Duration::from_millis(70), move || match rx.try_recv() {
                Ok(report) => {
                    clear_box(&inventory_i);
                    *devices_i.borrow_mut() = report.storage.clone();
                    render_inventory(&inventory_i, report);
                    refresh_i.set_sensitive(true);
                    smart_i.set_sensitive(!devices_i.borrow().is_empty());
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    clear_box(&inventory_i);
                    inventory_i.append(&message_card(
                        "Hardware inventory unavailable",
                        "The inventory worker stopped before returning a report.",
                        true,
                    ));
                    refresh_i.set_sensitive(true);
                    smart_i.set_sensitive(false);
                    glib::ControlFlow::Break
                }
            });
        })
    };

    {
        let refresh = refresh_inventory.clone();
        refresh_btn.connect_clicked(move |_| refresh());
    }

    {
        let devices_state = devices_state.clone();
        let smart_results = smart_results.clone();
        smart_btn.connect_clicked(move |button| {
            let devices = devices_state.borrow().clone();
            if devices.is_empty() {
                return;
            }
            button.set_sensitive(false);
            button.set_label("Reading health…");
            clear_box(&smart_results);
            smart_results.append(&message_card(
                "Authenticated health scan",
                "LinuxCare is requesting SMART/NVMe data for the detected physical drives. An administrator authentication prompt may appear.",
                false,
            ));

            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let result = read_health_for_devices(&devices);
                let _ = tx.send(result);
            });

            let results_i = smart_results.clone();
            let button_i = button.clone();
            glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                Ok(results) => {
                    clear_box(&results_i);
                    if results.is_empty() {
                        results_i.append(&message_card(
                            "No physical drives",
                            "No supported physical block devices were found.",
                            false,
                        ));
                    } else {
                        for result in results {
                            match result {
                                Ok(reading) => results_i.append(&health_card(&reading)),
                                Err((device, error)) => results_i.append(&message_card(
                                    &format!("{device} — health unavailable"),
                                    &error,
                                    true,
                                )),
                            }
                        }
                    }
                    button_i.set_label("Read SMART / NVMe Health");
                    button_i.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    clear_box(&results_i);
                    results_i.append(&message_card(
                        "Health scan unavailable",
                        "The SMART/NVMe worker stopped before returning a result.",
                        true,
                    ));
                    button_i.set_label("Read SMART / NVMe Health");
                    button_i.set_sensitive(true);
                    glib::ControlFlow::Break
                }
            });
        });
    }

    refresh_inventory();

    let clamp = adw::Clamp::builder()
        .maximum_size(1080)
        .tightening_threshold(760)
        .child(&page)
        .build();
    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn read_health_for_devices(
    devices: &[StorageDevice],
) -> Vec<Result<StorageHealthReading, (String, String)>> {
    let client = match PrivilegedClient::connect() {
        Ok(client) => client,
        Err(error) => {
            return devices
                .iter()
                .map(|device| Err((device.name.clone(), error.to_string())))
                .collect();
        }
    };

    devices
        .iter()
        .map(
            |device| match client.read_storage_health_json(&device.name) {
                Ok(json) => serde_json::from_str::<StorageHealthReading>(&json).map_err(|error| {
                    (
                        device.name.clone(),
                        format!("Invalid helper reply: {error}"),
                    )
                }),
                Err(error) => Err((device.name.clone(), error.to_string())),
            },
        )
        .collect()
}

fn render_inventory(container: &gtk::Box, report: HardwareReport) {
    let identity = gtk::Box::new(gtk::Orientation::Vertical, 8);
    identity.add_css_class("finding-card");
    identity.append(&section_title("System Identity", "computer-symbolic"));
    identity.append(&kv_row(
        "System",
        &format!("{} {}", report.system.vendor, report.system.product),
    ));
    identity.append(&kv_row("Mainboard", &report.system.board));
    identity.append(&kv_row("BIOS / Firmware", &report.system.bios_version));
    container.append(&identity);

    let compute = gtk::Box::new(gtk::Orientation::Vertical, 8);
    compute.add_css_class("finding-card");
    compute.append(&section_title("Compute & Memory", "processor-symbolic"));
    compute.append(&kv_row("CPU", &report.cpu.model));
    let cpu_detail = match report.cpu.physical_packages {
        Some(packages) => format!(
            "{} logical processors • {} physical package{}",
            report.cpu.logical_cpus,
            packages,
            if packages == 1 { "" } else { "s" }
        ),
        None => format!("{} logical processors", report.cpu.logical_cpus),
    };
    compute.append(&kv_row("CPU topology", &cpu_detail));
    compute.append(&kv_row(
        "Memory",
        &format!(
            "{} total • {} currently available",
            format::bytes(report.memory.total_bytes),
            format::bytes(report.memory.available_bytes)
        ),
    ));
    container.append(&compute);

    let graphics = gtk::Box::new(gtk::Orientation::Vertical, 8);
    graphics.add_css_class("finding-card");
    graphics.append(&section_title("Graphics", "video-display-symbolic"));
    if report.gpus.is_empty() {
        graphics.append(&dim_label(
            "No DRM GPU inventory was exposed by the kernel.",
        ));
    } else {
        for gpu in report.gpus {
            graphics.append(&kv_row(
                &gpu.card,
                &format!("Driver {} • PCI {}", gpu.driver, gpu.pci_id),
            ));
        }
    }
    container.append(&graphics);

    let storage = gtk::Box::new(gtk::Orientation::Vertical, 8);
    storage.add_css_class("finding-card");
    storage.append(&section_title(
        "Physical Storage",
        "drive-harddisk-symbolic",
    ));
    if report.storage.is_empty() {
        storage.append(&dim_label(
            "No physical block devices were detected in /sys/block.",
        ));
    } else {
        for device in report.storage {
            let kind = match device.rotational {
                Some(true) => "Rotational",
                Some(false) => "Solid-state / non-rotational",
                None => "Media type unknown",
            };
            let removable = if device.removable {
                " • Removable"
            } else {
                ""
            };
            storage.append(&kv_row(
                &format!("/dev/{}", device.name),
                &format!(
                    "{} {} • {} • {} • {}{}",
                    device.vendor,
                    device.model,
                    format::bytes(device.size_bytes),
                    device.transport,
                    kind,
                    removable
                ),
            ));
        }
    }
    container.append(&storage);

    let thermals = gtk::Box::new(gtk::Orientation::Vertical, 8);
    thermals.add_css_class("finding-card");
    thermals.append(&section_title("Thermal Sensors", "temperature-symbolic"));
    if report.sensors.is_empty() {
        thermals.append(&dim_label(
            "No readable hwmon temperature sensors were found.",
        ));
    } else {
        for sensor in report.sensors {
            thermals.append(&kv_row(
                &format!("{} · {}", sensor.source, sensor.label),
                &format!("{:.1} °C", sensor.celsius),
            ));
        }
    }
    container.append(&thermals);
}

fn health_card(reading: &StorageHealthReading) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("finding-card");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let title = gtk::Label::new(Some(&format!(
        "/dev/{} · {}",
        reading.device, reading.protocol
    )));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("heading");
    let (badge_text, badge_class) = health_badge(reading);
    let badge = gtk::Label::new(Some(badge_text));
    badge.add_css_class("risk-badge");
    badge.add_css_class(badge_class);
    header.append(&title);
    header.append(&badge);
    card.append(&header);

    if let Some(temperature) = reading.temperature_c {
        card.append(&kv_row("Temperature", &format!("{temperature:.0} °C")));
    }
    if let Some(hours) = reading.power_on_hours {
        card.append(&kv_row("Power-on time", &format!("{hours} hours")));
    }
    if let Some(used) = reading.percentage_used {
        card.append(&kv_row(
            "NVMe endurance used",
            &format!("{used}% of vendor-rated endurance indicator"),
        ));
    }
    if let Some(warning) = reading.critical_warning {
        card.append(&kv_row(
            "NVMe critical warning",
            &format!("0x{warning:02x}"),
        ));
    }
    if let Some(errors) = reading.media_errors {
        card.append(&kv_row("NVMe media/data errors", &errors.to_string()));
    }
    if let Some(shutdowns) = reading.unsafe_shutdowns {
        card.append(&kv_row("Unsafe shutdowns", &shutdowns.to_string()));
    }
    if let Some(value) = reading.reallocated_sectors {
        card.append(&kv_row("Reallocated sectors", &value.to_string()));
    }
    if let Some(value) = reading.pending_sectors {
        card.append(&kv_row("Pending sectors", &value.to_string()));
    }
    if let Some(value) = reading.offline_uncorrectable {
        card.append(&kv_row("Offline uncorrectable", &value.to_string()));
    }
    if let Some(value) = reading.error_log_count {
        card.append(&kv_row("SMART error log entries", &value.to_string()));
    }

    let source = gtk::Label::new(Some(&format!(
        "Source: {} • SMART available: {} • SMART enabled: {}",
        reading.source,
        option_bool(reading.smart_supported),
        option_bool(reading.smart_enabled)
    )));
    source.set_xalign(0.0);
    source.set_wrap(true);
    source.add_css_class("dim-label");
    card.append(&source);

    if let Some(note) = &reading.note {
        card.append(&dim_label(note));
    }

    card.upcast()
}

fn health_badge(reading: &StorageHealthReading) -> (&'static str, &'static str) {
    if reading.passed == Some(false)
        || reading.critical_warning.unwrap_or(0) != 0
        || reading.media_errors.unwrap_or(0) != 0
        || reading.pending_sectors.unwrap_or(0) != 0
        || reading.offline_uncorrectable.unwrap_or(0) != 0
    {
        ("ATTENTION", "risk-dangerous")
    } else if reading.reallocated_sectors.unwrap_or(0) != 0
        || reading.percentage_used.is_some_and(|value| value >= 90)
    {
        ("REVIEW", "risk-review")
    } else if reading.passed == Some(true) {
        ("PASSED", "risk-safe")
    } else {
        ("UNKNOWN", "risk-review")
    }
}

fn section_title(title: &str, icon: &str) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(20);
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.add_css_class("title-3");
    row.append(&image);
    row.append(&label);
    row.upcast()
}

fn kv_row(name: &str, value: &str) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let key = gtk::Label::new(Some(name));
    key.set_xalign(0.0);
    key.set_hexpand(true);
    key.add_css_class("dim-label");
    let value = gtk::Label::new(Some(value));
    value.set_xalign(1.0);
    value.set_wrap(true);
    value.set_max_width_chars(72);
    row.append(&key);
    row.append(&value);
    row.upcast()
}

fn dim_label(text: &str) -> gtk::Widget {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.add_css_class("dim-label");
    label.upcast()
}

fn message_card(title: &str, detail: &str, warning: bool) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.add_css_class(if warning {
        "warning-card"
    } else {
        "finding-card"
    });
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("heading");
    let detail = gtk::Label::new(Some(detail));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    card.append(&heading);
    card.append(&detail);
    card.upcast()
}

fn option_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    }
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}
