use crate::{
    executor::{self, CleanupEvent},
    format, history,
    model::{CleanupCandidate, CleanupRisk},
    quarantine,
    scan::{self, ScanEvent, ScanHandle},
    settings, timeline,
};
use adw::prelude::*;
use gtk::{cairo, glib};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

type RefreshAction = Rc<dyn Fn()>;
type RefreshSlot = Rc<RefCell<Option<RefreshAction>>>;

pub fn build_ui(app: &adw::Application) {
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("linuxcare-no-home"));
    let app_settings = settings::load(&home).unwrap_or_default();
    settings::apply(app_settings.appearance);
    load_css();

    let cleanup_policy = crate::cleanup_policy::load(&home).unwrap_or_default();
    if cleanup_policy.auto_purge_expired {
        if let Ok(purged) = quarantine::purge_expired(&home) {
            for item in purged {
                let mut event = timeline::MaintenanceEvent::new(
                    timeline::MaintenanceKind::Purge,
                    format!("Expired quarantine purged: {}", item.title),
                    "The undo window expired, so LinuxCare permanently removed the protected copy.",
                );
                event.actual_freed_bytes = item.bytes;
                let _ = timeline::append(&home, event);
            }
        }
    }

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("LinuxCare")
        .default_width(1100)
        .default_height(720)
        .build();

    let toast_overlay = adw::ToastOverlay::new();

    let flap = adw::Flap::builder()
        .fold_policy(adw::FlapFoldPolicy::Auto)
        .transition_type(adw::FlapTransitionType::Over)
        .swipe_to_open(true)
        .swipe_to_close(true)
        .build();

    let header = adw::HeaderBar::new();
    let window_title = adw::WindowTitle::new("LinuxCare", "Safety-First System Care");
    header.set_title_widget(Some(&window_title));

    // Toggle sidebar button for adaptive / narrow screens
    let toggle_sidebar = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Toggle Navigation Sidebar")
        .build();

    let flap_toggle = flap.clone();
    toggle_sidebar.connect_toggled(move |btn| {
        flap_toggle.set_reveal_flap(btn.is_active());
    });

    let toggle_sidebar_c = toggle_sidebar.clone();
    flap.connect_reveal_flap_notify(move |f| {
        toggle_sidebar_c.set_active(f.reveals_flap());
    });

    let toggle_sidebar_fold = toggle_sidebar.clone();
    flap.connect_folded_notify(move |f| {
        toggle_sidebar_fold.set_visible(f.is_folded());
    });
    toggle_sidebar.set_visible(flap.is_folded());
    header.pack_start(&toggle_sidebar);

    let sidebar = build_sidebar();
    let stack = gtk::Stack::builder()
        .hexpand(true)
        .vexpand(true)
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(220)
        .build();

    let toast_overlay_rc = Rc::new(toast_overlay.clone());

    let dashboard = Dashboard::new(home.clone(), toast_overlay_rc.clone(), true, None);
    let dashboard_health_refresh = dashboard.health_refresh.clone();
    stack.add_named(&dashboard.root, Some("dashboard"));
    stack.add_named(
        &crate::modules::maintenance::maintenance_center_page(home.clone()),
        Some("maintenance"),
    );
    let dashboard_cleaner = Dashboard::new(
        home.clone(),
        toast_overlay_rc.clone(),
        false,
        dashboard_health_refresh,
    );
    stack.add_named(&dashboard_cleaner.root, Some("cleaner"));
    stack.add_named(
        &crate::modules::vitals::vitals_page(toast_overlay_rc.clone()),
        Some("vitals"),
    );
    stack.add_named(
        &crate::modules::processes::process_intelligence_page(home.clone()),
        Some("processes"),
    );
    stack.add_named(
        &crate::modules::battery_lab::battery_lab_page(home.clone()),
        Some("battery"),
    );
    stack.add_named(
        &crate::modules::thermal::thermal_doctor_page(),
        Some("thermal"),
    );
    stack.add_named(&storage_analyzer_page(home.clone()), Some("storage"));
    stack.add_named(
        &crate::modules::filesystem::filesystem_doctor_page(),
        Some("filesystem"),
    );
    stack.add_named(
        &crate::modules::hardware::hardware_doctor_page(),
        Some("hardware"),
    );
    stack.add_named(&crate::modules::apps::apps_page(home.clone()), Some("apps"));
    stack.add_named(&crate::modules::packages::packages_page(), Some("packages"));
    stack.add_named(
        &crate::modules::startup::startup_page(home.clone()),
        Some("startup"),
    );
    stack.add_named(&boot_doctor_page(), Some("boot"));
    stack.add_named(&crate::modules::services::services_page(), Some("services"));
    stack.add_named(
        &crate::modules::network::network_doctor_page(),
        Some("network"),
    );
    stack.add_named(&crate::modules::security::security_page(), Some("security"));
    stack.add_named(
        &crate::modules::containers::container_analyzer_page(),
        Some("containers"),
    );
    stack.add_named(
        &crate::modules::privacy::privacy_page(home.clone()),
        Some("privacy"),
    );
    stack.add_named(
        &crate::modules::automation::automation_page(home.clone()),
        Some("automation"),
    );
    stack.add_named(
        &maintenance_page(home.clone(), toast_overlay_rc.clone()),
        Some("history"),
    );
    stack.add_named(
        &settings_page(home.clone(), toast_overlay_rc.clone()),
        Some("settings"),
    );

    let header_vitals =
        crate::modules::vitals::header_vitals_widget(stack.clone(), sidebar.clone());
    header_vitals.set_visible(!flap.is_folded());
    let header_vitals_fold = header_vitals.clone();
    flap.connect_folded_notify(move |f| {
        header_vitals_fold.set_visible(!f.is_folded());
    });
    header.pack_end(&header_vitals);

    let about_btn = gtk::Button::builder()
        .icon_name("help-about-symbolic")
        .tooltip_text("About LinuxCare (MilMit)")
        .css_classes(["flat"])
        .build();

    let win_about = window.clone();
    about_btn.connect_clicked(move |_| {
        show_about_window(Some(&win_about));
    });
    header.pack_end(&about_btn);

    let stack_clone = stack.clone();
    let flap_nav = flap.clone();
    sidebar.connect_row_selected(move |_, row| {
        let Some(row) = row else {
            return;
        };
        let page = match row.index() {
            0 => "dashboard",
            1 => "maintenance",
            2 => "cleaner",
            3 => "vitals",
            4 => "processes",
            5 => "battery",
            6 => "thermal",
            7 => "storage",
            8 => "filesystem",
            9 => "hardware",
            10 => "apps",
            11 => "packages",
            12 => "startup",
            13 => "boot",
            14 => "services",
            15 => "network",
            16 => "security",
            17 => "containers",
            18 => "privacy",
            19 => "automation",
            20 => "history",
            _ => "settings",
        };
        stack_clone.set_visible_child_name(page);
        if flap_nav.is_folded() {
            flap_nav.set_reveal_flap(false);
        }
    });
    if let Some(row) = sidebar.row_at_index(0) {
        sidebar.select_row(Some(&row));
    }

    let sidebar_scroll = gtk::ScrolledWindow::builder()
        .width_request(220)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&sidebar)
        .build();
    sidebar_scroll.add_css_class("sidebar");

    flap.set_flap(Some(&sidebar_scroll));

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_box.append(&header);
    content_box.append(&stack);

    flap.set_content(Some(&content_box));

    toast_overlay.set_child(Some(&flap));
    window.set_content(Some(&toast_overlay));
    window.present();

    // Deterministic GUI smoke mode for CI/Beta Gate. The timer is installed only
    // after the main window has been presented, so a successful exit proves the
    // GTK/Libadwaita startup path reached a visible application window.
    if std::env::var_os("LINUXCARE_GUI_SMOKE").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let app = app.clone();
        glib::timeout_add_local(Duration::from_millis(1200), move || {
            app.quit();
            glib::ControlFlow::Break
        });
    }
}

fn build_sidebar() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("navigation-sidebar");
    for (icon, label) in [
        ("view-dashboard-symbolic", "Dashboard"),
        ("emblem-system-symbolic", "Maintenance Center"),
        ("system-run-symbolic", "Smart Cleaner"),
        ("utilities-system-monitor-symbolic", "System Vitals"),
        ("view-list-symbolic", "Process Intelligence"),
        ("battery-symbolic", "Battery Lab"),
        ("weather-clear-symbolic", "Thermal & Power"),
        ("drive-harddisk-symbolic", "Storage Analyzer"),
        ("folder-symbolic", "Filesystem Doctor"),
        ("applications-engineering-symbolic", "Hardware Doctor"),
        ("view-app-grid-symbolic", "Applications"),
        ("package-x-generic-symbolic", "Package Doctor"),
        ("media-playback-start-symbolic", "Startup"),
        ("system-reboot-symbolic", "Boot Doctor"),
        ("system-run-symbolic", "Service Doctor"),
        ("network-workgroup-symbolic", "Network Doctor"),
        ("channel-secure-symbolic", "Security & Firewall"),
        ("drive-multidisk-symbolic", "Containers"),
        ("security-high-symbolic", "Privacy"),
        ("alarm-symbolic", "Automation"),
        ("document-open-recent-symbolic", "Safety & Timeline"),
        ("preferences-system-symbolic", "Settings"),
    ] {
        let row = gtk::ListBoxRow::new();
        let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        box_.set_margin_top(10);
        box_.set_margin_bottom(10);
        box_.set_margin_start(12);
        box_.set_margin_end(12);
        let img = gtk::Image::from_icon_name(icon);
        img.set_pixel_size(18);
        box_.append(&img);
        let text = gtk::Label::new(Some(label));
        text.set_xalign(0.0);
        text.set_hexpand(true);
        box_.append(&text);
        row.set_child(Some(&box_));
        list.append(&row);
    }
    list
}

fn gnome_integration_card(home: PathBuf) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("finding-card");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    title_box.set_hexpand(true);
    let title = gtk::Label::new(Some("GNOME Integration"));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    let subtitle = gtk::Label::new(Some(
        "Checks LinuxCare Vitals against the current GNOME Shell and manages only the user-visible enabled state. No Shell restart or privileged command is used.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    title_box.append(&title);
    title_box.append(&subtitle);
    header.append(&title_box);
    let badge = gtk::Label::new(Some("CHECKING"));
    badge.add_css_class("risk-badge");
    badge.add_css_class("health-neutral");
    header.append(&badge);
    card.append(&header);

    let detail = gtk::Label::new(Some("Reading GNOME Shell and extension state…"));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    card.append(&detail);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let refresh = gtk::Button::with_label("Refresh Status");
    refresh.add_css_class("pill");
    let enable = gtk::Button::with_label("Enable Extension");
    enable.add_css_class("pill");
    let disable = gtk::Button::with_label("Disable Extension");
    disable.add_css_class("pill");
    buttons.append(&refresh);
    buttons.append(&enable);
    buttons.append(&disable);
    card.append(&buttons);

    let do_refresh: Rc<dyn Fn()> = Rc::new({
        let home = home.clone();
        let detail = detail.clone();
        let badge = badge.clone();
        let enable = enable.clone();
        let disable = disable.clone();
        move || {
            let state = crate::gnome_integration::status(&home);
            let shell = state
                .shell_version
                .map(|version| format!("GNOME Shell {version}"))
                .unwrap_or_else(|| "GNOME Shell version unknown".to_string());
            let location = state
                .install_location
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not installed".to_string());
            detail.set_text(&format!(
                "{}\n{shell} • Extension path: {location}",
                state.detail
            ));

            for class in ["risk-safe", "risk-review", "health-neutral"] {
                badge.remove_css_class(class);
            }
            let (label, class) = if !state.extension_installed {
                ("NOT INSTALLED", "health-neutral")
            } else if state.shell_supported == Some(false) {
                ("INCOMPATIBLE", "risk-review")
            } else if state.extension_enabled == Some(true) {
                ("ENABLED", "risk-safe")
            } else if state.extension_enabled == Some(false) {
                ("DISABLED", "risk-review")
            } else {
                ("UNKNOWN", "health-neutral")
            };
            badge.set_text(label);
            badge.add_css_class(class);
            enable.set_sensitive(state.extension_installed && state.shell_supported != Some(false));
            disable.set_sensitive(state.extension_installed);
        }
    });

    {
        let do_refresh = do_refresh.clone();
        refresh.connect_clicked(move |_| do_refresh());
    }
    {
        let do_refresh = do_refresh.clone();
        let detail = detail.clone();
        enable.connect_clicked(move |_| match crate::gnome_integration::set_enabled(true) {
            Ok(()) => do_refresh(),
            Err(err) => detail.set_text(&format!("Could not enable extension: {err}")),
        });
    }
    {
        let do_refresh = do_refresh.clone();
        let detail = detail.clone();
        disable.connect_clicked(
            move |_| match crate::gnome_integration::set_enabled(false) {
                Ok(()) => do_refresh(),
                Err(err) => detail.set_text(&format!("Could not disable extension: {err}")),
            },
        );
    }
    do_refresh();
    card.upcast()
}

fn settings_page(home: PathBuf, toast_overlay: Rc<adw::ToastOverlay>) -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let title = gtk::Label::new(Some("Settings"));
    title.set_xalign(0.0);
    title.add_css_class("title-1");
    page.append(&title);

    let subtitle = gtk::Label::new(Some(
        "LinuxCare follows GNOME conventions and stores only small local preferences.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    card.add_css_class("finding-card");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text.set_hexpand(true);
    let heading = gtk::Label::new(Some("Appearance"));
    heading.set_xalign(0.0);
    heading.add_css_class("heading");
    let desc = gtk::Label::new(Some(
        "Use the system preference or force a light or dark appearance.",
    ));
    desc.set_xalign(0.0);
    desc.set_wrap(true);
    desc.add_css_class("dim-label");
    text.append(&heading);
    text.append(&desc);
    card.append(&text);

    let appearance_picker = gtk::DropDown::from_strings(&["System", "Light", "Dark"]);
    let current = settings::load(&home).unwrap_or_default();
    appearance_picker.set_selected(match current.appearance {
        settings::Appearance::System => 0,
        settings::Appearance::Light => 1,
        settings::Appearance::Dark => 2,
    });
    card.append(&appearance_picker);
    page.append(&card);

    let home_app = home.clone();
    appearance_picker.connect_selected_notify(move |picker| {
        let appearance = match picker.selected() {
            1 => settings::Appearance::Light,
            2 => settings::Appearance::Dark,
            _ => settings::Appearance::System,
        };
        settings::apply(appearance);
        let mut s = settings::load(&home_app).unwrap_or_default();
        s.appearance = appearance;
        let _ = settings::save(&home_app, &s);
    });

    // GNOME Top Bar Customization Card
    let topbar_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    topbar_card.add_css_class("finding-card");

    let topbar_title = gtk::Label::new(Some("GNOME Top Bar Indicators"));
    topbar_title.set_xalign(0.0);
    topbar_title.add_css_class("heading");
    let topbar_sub = gtk::Label::new(Some(
        "Customize which live hardware metrics are displayed directly in the top desktop panel.",
    ));
    topbar_sub.set_xalign(0.0);
    topbar_sub.set_wrap(true);
    topbar_sub.add_css_class("dim-label");
    topbar_card.append(&topbar_title);
    topbar_card.append(&topbar_sub);

    let switches_box = gtk::Box::new(gtk::Orientation::Vertical, 8);

    fn make_toggle_row(
        label_text: &str,
        initial_state: bool,
        home: &std::path::Path,
        on_change: impl Fn(&mut settings::AppSettings, bool) + 'static,
    ) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let lbl = gtk::Label::new(Some(label_text));
        lbl.set_xalign(0.0);
        lbl.set_hexpand(true);

        let sw = gtk::Switch::new();
        sw.set_active(initial_state);
        sw.set_valign(gtk::Align::Center);

        let h_clone = home.to_path_buf();
        sw.connect_state_set(move |_, state| {
            let mut s = settings::load(&h_clone).unwrap_or_default();
            on_change(&mut s, state);
            let _ = settings::save(&h_clone, &s);
            glib::Propagation::Proceed
        });

        row.append(&lbl);
        row.append(&sw);
        row
    }

    let cur_settings = settings::load(&home).unwrap_or_default();
    switches_box.append(&make_toggle_row(
        "Show CPU Usage (%)",
        cur_settings.top_bar.show_cpu,
        &home,
        |s, v| s.top_bar.show_cpu = v,
    ));
    switches_box.append(&make_toggle_row(
        "Show CPU Temperature (°C)",
        cur_settings.top_bar.show_temp,
        &home,
        |s, v| s.top_bar.show_temp = v,
    ));
    switches_box.append(&make_toggle_row(
        "Show RAM Usage (%)",
        cur_settings.top_bar.show_ram,
        &home,
        |s, v| s.top_bar.show_ram = v,
    ));
    switches_box.append(&make_toggle_row(
        "Show Network Speed (↓/↑)",
        cur_settings.top_bar.show_net,
        &home,
        |s, v| s.top_bar.show_net = v,
    ));
    switches_box.append(&make_toggle_row(
        "Show Battery & Power (%)",
        cur_settings.top_bar.show_battery,
        &home,
        |s, v| s.top_bar.show_battery = v,
    ));

    let interval_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let interval_label = gtk::Label::new(Some("Refresh interval"));
    interval_label.set_xalign(0.0);
    interval_label.set_hexpand(true);
    let interval_picker =
        gtk::DropDown::from_strings(&["1 second", "2 seconds", "5 seconds", "10 seconds"]);
    interval_picker.set_selected(match cur_settings.top_bar.interval_sec {
        1 => 0,
        5 => 2,
        10 => 3,
        _ => 1,
    });
    let home_interval = home.clone();
    interval_picker.connect_selected_notify(move |picker| {
        let seconds = match picker.selected() {
            0 => 1,
            2 => 5,
            3 => 10,
            _ => 2,
        };
        let mut app_settings = settings::load(&home_interval).unwrap_or_default();
        app_settings.top_bar.interval_sec = seconds;
        let _ = settings::save(&home_interval, &app_settings);
    });
    interval_row.append(&interval_label);
    interval_row.append(&interval_picker);
    topbar_card.append(&interval_row);

    page.append(&topbar_card);
    page.append(&gnome_integration_card(home.clone()));

    // Exclusions Management Card
    let excl_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    excl_card.add_css_class("finding-card");

    let excl_title = gtk::Label::new(Some("Scan Exclusions (Ignore List)"));
    excl_title.set_xalign(0.0);
    excl_title.add_css_class("heading");
    let excl_sub = gtk::Label::new(Some(
        "Paths and files specified here will be automatically skipped during Smart Scan.",
    ));
    excl_sub.set_xalign(0.0);
    excl_sub.set_wrap(true);
    excl_sub.add_css_class("dim-label");
    excl_card.append(&excl_title);
    excl_card.append(&excl_sub);

    let input_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some("e.g. /home/user/.cache/important"));
    entry.set_hexpand(true);
    let add_btn = gtk::Button::with_label("Add Path");
    add_btn.add_css_class("pill");
    add_btn.add_css_class("suggested-action");
    input_row.append(&entry);
    input_row.append(&add_btn);
    excl_card.append(&input_row);

    let list_container = gtk::Box::new(gtk::Orientation::Vertical, 6);
    excl_card.append(&list_container);
    page.append(&excl_card);

    let home_excl = home.clone();
    let list_c = list_container.clone();

    fn refresh_exclusions_list(container: &gtk::Box, home_dir: &std::path::Path) {
        while let Some(child) = container.first_child() {
            container.remove(&child);
        }
        let current_s = settings::load(home_dir).unwrap_or_default();
        if current_s.exclusions.is_empty() {
            let empty_lbl = gtk::Label::new(Some("No excluded paths configured."));
            empty_lbl.set_xalign(0.0);
            empty_lbl.add_css_class("dim-label");
            container.append(&empty_lbl);
        } else {
            for path_str in current_s.exclusions {
                let item_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let path_label = gtk::Label::new(Some(&path_str));
                path_label.set_xalign(0.0);
                path_label.set_hexpand(true);

                let remove_btn = gtk::Button::with_label("Remove");
                remove_btn.add_css_class("pill");

                let h_rem = home_dir.to_path_buf();
                let p_rem = path_str.clone();
                let cont_rem = container.clone();

                remove_btn.connect_clicked(move |_| {
                    let mut s = settings::load(&h_rem).unwrap_or_default();
                    s.exclusions.retain(|x| x != &p_rem);
                    let _ = settings::save(&h_rem, &s);
                    refresh_exclusions_list(&cont_rem, &h_rem);
                });

                item_row.append(&path_label);
                item_row.append(&remove_btn);
                container.append(&item_row);
            }
        }
    }

    refresh_exclusions_list(&list_c, &home_excl);

    let home_add = home.clone();
    let list_add = list_container.clone();
    let entry_add = entry.clone();
    let toast_add = toast_overlay.clone();

    add_btn.connect_clicked(move |_| {
        let val = entry_add.text().trim().to_string();
        if !val.is_empty() {
            let mut s = settings::load(&home_add).unwrap_or_default();
            if !s.exclusions.contains(&val) {
                s.exclusions.push(val.clone());
                let _ = settings::save(&home_add, &s);
                entry_add.set_text("");
                refresh_exclusions_list(&list_add, &home_add);
                toast_add.add_toast(adw::Toast::new(&format!("Added exclusion: {val}")));
            }
        }
    });

    // Developer & Community Card (MilMit)
    let dev_card = gtk::Box::new(gtk::Orientation::Vertical, 14);
    dev_card.add_css_class("hero-card");

    let dev_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let dev_icon = gtk::Image::from_icon_name("avatar-default-symbolic");
    dev_icon.set_pixel_size(28);

    let dev_title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    dev_title_box.set_hexpand(true);
    let dev_title = gtk::Label::new(Some("Developed by MilMit"));
    dev_title.set_xalign(0.0);
    dev_title.add_css_class("heading");
    let dev_desc = gtk::Label::new(Some("Creator & Lead Engineer • milmit.net"));
    dev_desc.set_xalign(0.0);
    dev_desc.add_css_class("dim-label");
    dev_title_box.append(&dev_title);
    dev_title_box.append(&dev_desc);

    dev_header.append(&dev_icon);
    dev_header.append(&dev_title_box);
    dev_card.append(&dev_header);

    let dev_bio = gtk::Label::new(Some(
        "LinuxCare is an open-source, safety-first system care suite engineered by MilMit. Delivering modern Libadwaita ergonomics, real-time hardware diagnostics, and deep cleaner utilities."
    ));
    dev_bio.set_xalign(0.0);
    dev_bio.set_wrap(true);
    dev_bio.add_css_class("dim-label");
    dev_card.append(&dev_bio);

    let links_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);

    let web_btn = gtk::Button::with_label("🌐 Website (milmit.net)");
    web_btn.add_css_class("pill");
    web_btn.add_css_class("suggested-action");
    web_btn.connect_clicked(|_| {
        let _ = gtk::gio::AppInfo::launch_default_for_uri(
            "https://milmit.net",
            None::<&gtk::gio::AppLaunchContext>,
        );
    });

    let tg_btn = gtk::Button::with_label("✈️ Telegram (@milmit)");
    tg_btn.add_css_class("pill");
    tg_btn.connect_clicked(|_| {
        let _ = gtk::gio::AppInfo::launch_default_for_uri(
            "https://t.me/milmit",
            None::<&gtk::gio::AppLaunchContext>,
        );
    });

    let ig_btn = gtk::Button::with_label("📸 Instagram (@milmit)");
    ig_btn.add_css_class("pill");
    ig_btn.connect_clicked(|_| {
        let _ = gtk::gio::AppInfo::launch_default_for_uri(
            "https://instagram.com/milmit",
            None::<&gtk::gio::AppLaunchContext>,
        );
    });

    let about_modal_btn = gtk::Button::with_label("About LinuxCare");
    about_modal_btn.add_css_class("pill");
    about_modal_btn.connect_clicked(|btn| {
        let win = btn.root().and_downcast::<gtk::Window>();
        show_about_window(win.as_ref());
    });

    links_row.append(&web_btn);
    links_row.append(&tg_btn);
    links_row.append(&ig_btn);
    links_row.append(&about_modal_btn);
    dev_card.append(&links_row);

    page.append(&dev_card);

    let clamp = adw::Clamp::builder()
        .maximum_size(920)
        .tightening_threshold(700)
        .child(&page)
        .build();

    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn make_donut_chart(usages: &[crate::storage::DirUsage]) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    card.add_css_class("hero-card");
    card.add_css_class("donut-card");
    card.set_margin_bottom(8);

    let area = gtk::DrawingArea::new();
    area.set_content_width(180);
    area.set_content_height(180);
    area.set_size_request(180, 180);

    let total: u64 = usages.iter().map(|u| u.bytes).sum();
    let usages_vec: Vec<(String, u64)> = usages.iter().map(|u| (u.name.clone(), u.bytes)).collect();
    let usages_rc = Rc::new(usages_vec);

    let hovered_idx: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));

    let motion = gtk::EventControllerMotion::new();
    let h_idx_c = hovered_idx.clone();
    let area_c = area.clone();
    let u_rc_m = usages_rc.clone();
    motion.connect_motion(move |_, x, y| {
        let w = area_c.width() as f64;
        let h = area_c.height() as f64;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let dx = x - cx;
        let dy = y - cy;
        let dist = (dx * dx + dy * dy).sqrt();
        let outer_r = w.min(h) * 0.44;
        let inner_r = outer_r * 0.55;

        if total > 0 && dist >= inner_r && dist <= outer_r + 12.0 {
            let mut angle = dy.atan2(dx) + std::f64::consts::FRAC_PI_2;
            if angle < 0.0 {
                angle += std::f64::consts::TAU;
            }
            let mut cur = 0.0;
            let mut found = None;
            for (i, (_, bytes)) in u_rc_m.iter().enumerate() {
                let sweep = (*bytes as f64 / total as f64) * std::f64::consts::TAU;
                if angle >= cur && angle <= cur + sweep {
                    found = Some(i);
                    break;
                }
                cur += sweep;
            }
            h_idx_c.set(found);
        } else {
            h_idx_c.set(None);
        }
        area_c.queue_draw();
    });

    let h_leave = hovered_idx.clone();
    let area_leave = area.clone();
    motion.connect_leave(move |_| {
        h_leave.set(None);
        area_leave.queue_draw();
    });
    area.add_controller(motion);

    let usages_draw = usages_rc.clone();
    let h_draw = hovered_idx.clone();
    area.set_draw_func(move |_, cr: &cairo::Context, width, height| {
        let w = width as f64;
        let h = height as f64;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let base_outer_r = w.min(h) * 0.44;
        let base_inner_r = base_outer_r * 0.58;

        if total == 0 {
            cr.set_line_width(base_outer_r - base_inner_r);
            cr.set_source_rgba(0.4, 0.45, 0.55, 0.18);
            cr.arc(
                cx,
                cy,
                (base_outer_r + base_inner_r) / 2.0,
                0.0,
                std::f64::consts::TAU,
            );
            let _ = cr.stroke();
            return;
        }

        let colors: &[(f64, f64, f64)] = &[
            (0.208, 0.518, 0.894), // Sapphire Blue
            (0.180, 0.761, 0.494), // Emerald Green
            (0.898, 0.647, 0.039), // Amber Gold
            (0.569, 0.255, 0.675), // Cyber Violet
            (0.878, 0.106, 0.141), // Crimson Red
            (0.110, 0.443, 0.847), // Indigo
            (0.0, 0.753, 0.941),   // Cyan
            (1.0, 0.471, 0.0),     // Orange Flame
            (0.467, 0.463, 0.482), // Slate
        ];

        let mut current_angle = -std::f64::consts::FRAC_PI_2;
        let hovered = h_draw.get();

        for (i, (_, bytes)) in usages_draw.iter().enumerate() {
            if *bytes == 0 {
                continue;
            }
            let sweep = (*bytes as f64 / total as f64) * std::f64::consts::TAU;
            let (r, g, b) = colors[i % colors.len()];

            let is_hovered = hovered == Some(i);
            let outer_r = if is_hovered {
                base_outer_r + 6.0
            } else {
                base_outer_r
            };
            let inner_r = if is_hovered {
                base_inner_r - 2.0
            } else {
                base_inner_r
            };
            let line_w = outer_r - inner_r;
            let mid_r = (outer_r + inner_r) / 2.0;

            cr.set_line_width(line_w);
            cr.set_line_cap(cairo::LineCap::Butt);
            if is_hovered {
                cr.set_source_rgb(
                    (r * 1.15).min(1.0),
                    (g * 1.15).min(1.0),
                    (b * 1.15).min(1.0),
                );
            } else {
                cr.set_source_rgb(r, g, b);
            }
            let gap = if usages_draw.len() > 1 { 0.035 } else { 0.0 };
            cr.arc(cx, cy, mid_r, current_angle, current_angle + sweep - gap);
            let _ = cr.stroke();

            current_angle += sweep;
        }

        // Inner Hub background circle
        cr.set_source_rgba(0.4, 0.45, 0.55, 0.08);
        cr.arc(cx, cy, base_inner_r - 4.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();

        // Inner Hub subtle border
        cr.set_line_width(1.0);
        cr.set_source_rgba(0.4, 0.45, 0.55, 0.20);
        cr.arc(cx, cy, base_inner_r - 4.0, 0.0, std::f64::consts::TAU);
        let _ = cr.stroke();
    });

    card.append(&area);

    let legend = gtk::Box::new(gtk::Orientation::Vertical, 8);
    legend.set_hexpand(true);
    legend.set_valign(gtk::Align::Center);

    let legend_title = gtk::Label::new(Some(&format!("Total Analyzed: {}", format::bytes(total))));
    legend_title.set_xalign(0.0);
    legend_title.add_css_class("heading");
    legend.append(&legend_title);

    let colors_hex = [
        "#3584e4", "#2ec27e", "#e5a50a", "#9141ac", "#e01b24", "#1c71d8", "#00c0f0", "#ff7800",
        "#77767b",
    ];

    let grid = gtk::Grid::new();
    grid.set_column_spacing(18);
    grid.set_row_spacing(6);

    for (i, (name, bytes)) in usages_rc.iter().take(8).enumerate() {
        let col_idx = (i % 2) as i32;
        let row_idx = (i / 2) as i32;

        let item_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let color_dot = gtk::Label::new(Some("●"));
        let color_hex = colors_hex[i % colors_hex.len()];
        let provider = gtk::CssProvider::new();
        provider.load_from_data(&format!("label {{ color: {color_hex}; font-size: 14px; }}"));
        color_dot
            .style_context()
            .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

        let pct = if total > 0 {
            (*bytes as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let lbl = gtk::Label::new(Some(&format!(
            "{name}: {} ({pct:.0}%)",
            format::bytes(*bytes)
        )));
        lbl.set_xalign(0.0);
        lbl.add_css_class("dim-label");

        item_box.append(&color_dot);
        item_box.append(&lbl);
        grid.attach(&item_box, col_idx, row_idx, 1, 1);
    }

    legend.append(&grid);
    card.append(&legend);
    card.upcast()
}

fn boot_doctor_page() -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 3);
    title_box.set_hexpand(true);
    let title = gtk::Label::new(Some("Boot Doctor"));
    title.set_xalign(0.0);
    title.add_css_class("title-1");
    let subtitle = gtk::Label::new(Some(
        "Explain boot time, failed services, and the startup critical path without blindly disabling system units.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    title_box.append(&title);
    title_box.append(&subtitle);
    title_row.append(&title_box);

    let refresh_btn = gtk::Button::with_label("Analyze Boot");
    refresh_btn.add_css_class("suggested-action");
    refresh_btn.add_css_class("pill");
    title_row.append(&refresh_btn);
    page.append(&title_row);

    let summary_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    summary_card.add_css_class("boot-card");

    let summary_top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let total_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    total_box.set_hexpand(true);
    let total_label = gtk::Label::new(Some("—"));
    total_label.set_xalign(0.0);
    total_label.add_css_class("hero-number");
    let total_caption = gtk::Label::new(Some("TOTAL BOOT TIME"));
    total_caption.set_xalign(0.0);
    total_caption.add_css_class("caption-heading");
    total_caption.add_css_class("dim-label");
    total_box.append(&total_label);
    total_box.append(&total_caption);

    let status_badge = gtk::Label::new(Some("ANALYZING"));
    status_badge.add_css_class("risk-badge");
    status_badge.add_css_class("health-neutral");
    status_badge.set_valign(gtk::Align::Center);
    summary_top.append(&total_box);
    summary_top.append(&status_badge);
    summary_card.append(&summary_top);

    let summary_label = gtk::Label::new(Some("Collecting systemd boot timing…"));
    summary_label.set_xalign(0.0);
    summary_label.set_wrap(true);
    summary_card.append(&summary_label);

    let phase_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    phase_box.set_homogeneous(true);
    summary_card.append(&phase_box);
    page.append(&summary_card);

    let recommendation_card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    recommendation_card.add_css_class("finding-card");
    let recommendation_icon = gtk::Image::from_icon_name("dialog-information-symbolic");
    recommendation_icon.set_pixel_size(24);
    let recommendation_label = gtk::Label::new(Some(
        "LinuxCare will recommend review targets after analysis.",
    ));
    recommendation_label.set_xalign(0.0);
    recommendation_label.set_wrap(true);
    recommendation_label.set_hexpand(true);
    recommendation_card.append(&recommendation_icon);
    recommendation_card.append(&recommendation_label);
    page.append(&recommendation_card);

    let sections = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    sections.set_homogeneous(true);

    let slow_card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    slow_card.add_css_class("finding-card");
    let slow_title = gtk::Label::new(Some("Slowest startup units"));
    slow_title.set_xalign(0.0);
    slow_title.add_css_class("title-3");
    let slow_units_box = gtk::Box::new(gtk::Orientation::Vertical, 7);
    slow_card.append(&slow_title);
    slow_card.append(&slow_units_box);

    let failed_card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    failed_card.add_css_class("finding-card");
    let failed_title = gtk::Label::new(Some("Failed services"));
    failed_title.set_xalign(0.0);
    failed_title.add_css_class("title-3");
    let failed_units_box = gtk::Box::new(gtk::Orientation::Vertical, 7);
    failed_card.append(&failed_title);
    failed_card.append(&failed_units_box);

    sections.append(&slow_card);
    sections.append(&failed_card);
    page.append(&sections);

    let critical_card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    critical_card.add_css_class("finding-card");
    let critical_title = gtk::Label::new(Some("Critical boot chain"));
    critical_title.set_xalign(0.0);
    critical_title.add_css_class("title-3");
    let critical_hint = gtk::Label::new(Some(
        "The critical chain shows dependencies that directly gate boot progress. A long unit is not automatically safe to disable.",
    ));
    critical_hint.set_xalign(0.0);
    critical_hint.set_wrap(true);
    critical_hint.add_css_class("dim-label");
    let critical_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    critical_card.append(&critical_title);
    critical_card.append(&critical_hint);
    critical_card.append(&critical_box);
    page.append(&critical_card);

    let refresh: Rc<dyn Fn()> = Rc::new({
        let refresh_btn = refresh_btn.clone();
        let total_label = total_label.clone();
        let status_badge = status_badge.clone();
        let summary_label = summary_label.clone();
        let phase_box = phase_box.clone();
        let recommendation_label = recommendation_label.clone();
        let slow_units_box = slow_units_box.clone();
        let failed_units_box = failed_units_box.clone();
        let critical_box = critical_box.clone();

        move || {
            refresh_btn.set_sensitive(false);
            total_label.set_text("…");
            summary_label.set_text("Collecting systemd boot timing and service data…");
            status_badge.set_text("ANALYZING");
            for class in [
                "risk-safe",
                "risk-review",
                "risk-dangerous",
                "health-neutral",
            ] {
                status_badge.remove_css_class(class);
            }
            status_badge.add_css_class("health-neutral");

            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let report = crate::boot::collect_report();
                let _ = tx.send(report);
            });

            let refresh_btn_i = refresh_btn.clone();
            let total_label_i = total_label.clone();
            let status_badge_i = status_badge.clone();
            let summary_label_i = summary_label.clone();
            let phase_box_i = phase_box.clone();
            let recommendation_label_i = recommendation_label.clone();
            let slow_units_box_i = slow_units_box.clone();
            let failed_units_box_i = failed_units_box.clone();
            let critical_box_i = critical_box.clone();

            glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                Ok(report) => {
                    total_label_i.set_text(&format_boot_duration(report.total_secs));
                    status_badge_i.set_text(&report.status_label.to_uppercase());
                    for class in [
                        "risk-safe",
                        "risk-review",
                        "risk-dangerous",
                        "health-neutral",
                    ] {
                        status_badge_i.remove_css_class(class);
                    }
                    status_badge_i.add_css_class(match report.status_label.as_str() {
                        "Fast" | "Normal" => "risk-safe",
                        "Slow" => "risk-review",
                        "Very slow" => "risk-dangerous",
                        _ => "health-neutral",
                    });
                    summary_label_i.set_text(&report.summary);
                    recommendation_label_i.set_text(&report.recommendation);

                    while let Some(child) = phase_box_i.first_child() {
                        phase_box_i.remove(&child);
                    }
                    for (name, value) in [
                        ("Firmware", report.firmware_secs),
                        ("Loader", report.loader_secs),
                        ("Kernel", report.kernel_secs),
                        ("Userspace", report.userspace_secs),
                        ("Graphical", report.graphical_secs),
                    ] {
                        phase_box_i.append(&boot_phase_metric(name, value));
                    }

                    while let Some(child) = slow_units_box_i.first_child() {
                        slow_units_box_i.remove(&child);
                    }
                    if report.slow_units.is_empty() {
                        slow_units_box_i
                            .append(&empty_state_label("No systemd blame data available."));
                    } else {
                        for unit in report.slow_units.iter().take(8) {
                            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                            let name = gtk::Label::new(Some(&unit.name));
                            name.set_xalign(0.0);
                            name.set_hexpand(true);
                            let duration =
                                gtk::Label::new(Some(&format!("{:.2}s", unit.duration_secs)));
                            duration.add_css_class("dim-label");
                            row.append(&name);
                            row.append(&duration);
                            slow_units_box_i.append(&row);
                        }
                    }

                    while let Some(child) = failed_units_box_i.first_child() {
                        failed_units_box_i.remove(&child);
                    }
                    if report.failed_units.is_empty() {
                        let healthy = gtk::Label::new(Some("No failed system services detected."));
                        healthy.set_xalign(0.0);
                        healthy.set_wrap(true);
                        healthy.add_css_class("status-badge-active");
                        failed_units_box_i.append(&healthy);
                    } else {
                        for unit in report.failed_units.iter().take(8) {
                            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                            let icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
                            let name = gtk::Label::new(Some(unit));
                            name.set_xalign(0.0);
                            name.set_hexpand(true);
                            name.set_wrap(true);
                            row.append(&icon);
                            row.append(&name);
                            failed_units_box_i.append(&row);
                        }
                    }

                    while let Some(child) = critical_box_i.first_child() {
                        critical_box_i.remove(&child);
                    }
                    if report.critical_chain.is_empty() {
                        critical_box_i
                            .append(&empty_state_label("Critical-chain data is unavailable."));
                    } else {
                        for (index, line) in report.critical_chain.iter().enumerate() {
                            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                            let step = gtk::Label::new(Some(&(index + 1).to_string()));
                            step.add_css_class("boot-chain-step");
                            let text = gtk::Label::new(Some(line));
                            text.set_xalign(0.0);
                            text.set_wrap(true);
                            text.set_hexpand(true);
                            row.append(&step);
                            row.append(&text);
                            critical_box_i.append(&row);
                        }
                    }

                    refresh_btn_i.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    total_label_i.set_text("—");
                    status_badge_i.set_text("UNAVAILABLE");
                    summary_label_i
                        .set_text("Boot analysis worker stopped before returning a report.");
                    refresh_btn_i.set_sensitive(true);
                    glib::ControlFlow::Break
                }
            });
        }
    });

    {
        let refresh_c = refresh.clone();
        refresh_btn.connect_clicked(move |_| refresh_c());
    }
    refresh();

    let clamp = adw::Clamp::builder()
        .maximum_size(1040)
        .tightening_threshold(760)
        .child(&page)
        .build();
    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn format_boot_duration(value: Option<f64>) -> String {
    match value {
        Some(seconds) if seconds >= 60.0 => {
            format!(
                "{}m {:.1}s",
                (seconds / 60.0).floor() as u64,
                seconds % 60.0
            )
        }
        Some(seconds) => format!("{seconds:.1}s"),
        None => "—".to_string(),
    }
}

fn boot_phase_metric(name: &str, value: Option<f64>) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 3);
    card.add_css_class("boot-phase-card");
    let value_label = gtk::Label::new(Some(&format_boot_duration(value)));
    value_label.add_css_class("title-3");
    let name_label = gtk::Label::new(Some(name));
    name_label.add_css_class("dim-label");
    name_label.add_css_class("caption");
    card.append(&value_label);
    card.append(&name_label);
    card.upcast()
}

fn empty_state_label(text: &str) -> gtk::Widget {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.add_css_class("dim-label");
    label.upcast()
}

fn storage_runway_panel(home: PathBuf) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 14);
    card.add_css_class("runway-card");

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 3);
    title_box.set_hexpand(true);
    let title = gtk::Label::new(Some("Storage Runway"));
    title.set_xalign(0.0);
    title.add_css_class("title-2");
    let subtitle = gtk::Label::new(Some(
        "Learns root-disk growth over time and estimates when capacity pressure may arrive.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    title_box.append(&title);
    title_box.append(&subtitle);
    top.append(&title_box);

    let status_badge = gtk::Label::new(Some("LEARNING"));
    status_badge.add_css_class("risk-badge");
    status_badge.add_css_class("health-neutral");
    status_badge.set_valign(gtk::Align::Center);
    let refresh_btn = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh Storage Runway")
        .css_classes(["flat", "circular"])
        .build();
    top.append(&status_badge);
    top.append(&refresh_btn);
    card.append(&top);

    let metrics = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    metrics.set_homogeneous(true);
    let usage_value = gtk::Label::new(Some("—"));
    let growth_value = gtk::Label::new(Some("Learning baseline"));
    let history_value = gtk::Label::new(Some("—"));
    metrics.append(&runway_metric("ROOT USED", &usage_value));
    metrics.append(&runway_metric("OBSERVED TREND", &growth_value));
    metrics.append(&runway_metric("HISTORY WINDOW", &history_value));
    card.append(&metrics);

    let usage_bar = gtk::ProgressBar::new();
    usage_bar.add_css_class("runway-progress");
    card.append(&usage_bar);

    let summary = gtk::Label::new(Some("LinuxCare is collecting the first capacity sample."));
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    card.append(&summary);

    let forecast_title = gtk::Label::new(Some("Capacity forecast"));
    forecast_title.set_xalign(0.0);
    forecast_title.add_css_class("heading");
    card.append(&forecast_title);

    let targets = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    targets.set_homogeneous(true);
    let target_80 = gtk::Label::new(Some("No forecast"));
    let target_90 = gtk::Label::new(Some("No forecast"));
    let target_95 = gtk::Label::new(Some("No forecast"));
    targets.append(&runway_target("80%", &target_80));
    targets.append(&runway_target("90%", &target_90));
    targets.append(&runway_target("95%", &target_95));
    card.append(&targets);

    let caveat = gtk::Label::new(Some(
        "Forecasts use local historical samples only. Package upgrades, large downloads, cleanup, snapshots, or partition changes can invalidate the trend.",
    ));
    caveat.set_xalign(0.0);
    caveat.set_wrap(true);
    caveat.add_css_class("dim-label");
    caveat.add_css_class("caption");
    card.append(&caveat);

    let refresh: Rc<dyn Fn()> = Rc::new({
        let home = home.clone();
        let refresh_btn = refresh_btn.clone();
        let status_badge = status_badge.clone();
        let usage_value = usage_value.clone();
        let growth_value = growth_value.clone();
        let history_value = history_value.clone();
        let usage_bar = usage_bar.clone();
        let summary = summary.clone();
        let target_80 = target_80.clone();
        let target_90 = target_90.clone();
        let target_95 = target_95.clone();

        move || {
            refresh_btn.set_sensitive(false);
            let (tx, rx) = mpsc::channel();
            let home_thread = home.clone();
            std::thread::spawn(move || {
                let report = crate::storage_runway::collect_report(&home_thread);
                let _ = tx.send(report);
            });

            let refresh_btn_i = refresh_btn.clone();
            let status_badge_i = status_badge.clone();
            let usage_value_i = usage_value.clone();
            let growth_value_i = growth_value.clone();
            let history_value_i = history_value.clone();
            let usage_bar_i = usage_bar.clone();
            let summary_i = summary.clone();
            let target_80_i = target_80.clone();
            let target_90_i = target_90.clone();
            let target_95_i = target_95.clone();

            glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                Ok(report) => {
                    usage_value_i.set_text(&format!(
                        "{} / {} ({:.0}%)",
                        format::bytes(report.used_bytes),
                        format::bytes(report.total_bytes),
                        report.used_percent
                    ));
                    growth_value_i.set_text(&crate::storage_runway::format_growth(
                        report.growth_bytes_per_day,
                    ));
                    history_value_i.set_text(&format!(
                        "{} samples • {:.1} days",
                        report.sample_count, report.span_days
                    ));
                    usage_bar_i.set_fraction((report.used_percent / 100.0).clamp(0.0, 1.0));
                    summary_i.set_text(&report.summary);

                    status_badge_i.set_text(&report.status_label.to_uppercase());
                    for class in [
                        "risk-safe",
                        "risk-review",
                        "risk-dangerous",
                        "health-neutral",
                    ] {
                        status_badge_i.remove_css_class(class);
                    }
                    status_badge_i.add_css_class(match report.status_label.as_str() {
                        "Stable" => "risk-safe",
                        "Growing" | "Pressure soon" | "High pressure" => "risk-review",
                        "Critical" => "risk-dangerous",
                        _ => "health-neutral",
                    });

                    for target in &report.targets {
                        let text = crate::storage_runway::format_days(target.days);
                        match target.percent {
                            80 => target_80_i.set_text(&text),
                            90 => target_90_i.set_text(&text),
                            95 => target_95_i.set_text(&text),
                            _ => {}
                        }
                    }

                    refresh_btn_i.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    summary_i.set_text("Storage Runway worker stopped before returning a report.");
                    refresh_btn_i.set_sensitive(true);
                    glib::ControlFlow::Break
                }
            });
        }
    });

    {
        let refresh_c = refresh.clone();
        refresh_btn.connect_clicked(move |_| refresh_c());
    }
    refresh();

    card.upcast()
}

fn runway_metric(title: &str, value: &gtk::Label) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
    box_.add_css_class("runway-metric");
    value.set_xalign(0.0);
    value.set_wrap(true);
    value.add_css_class("heading");
    let caption = gtk::Label::new(Some(title));
    caption.set_xalign(0.0);
    caption.add_css_class("caption-heading");
    caption.add_css_class("dim-label");
    box_.append(value);
    box_.append(&caption);
    box_.upcast()
}

fn runway_target(title: &str, value: &gtk::Label) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
    box_.add_css_class("runway-target");
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("caption-heading");
    title_label.add_css_class("dim-label");
    value.add_css_class("title-3");
    box_.append(&title_label);
    box_.append(value);
    box_.upcast()
}

fn storage_intelligence_panel(home: PathBuf) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("finding-card");

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 3);
    title_box.set_hexpand(true);
    let title = gtk::Label::new(Some("Storage Intelligence 3.0"));
    title.set_xalign(0.0);
    title.add_css_class("title-2");
    let subtitle = gtk::Label::new(Some(
        "Uses longer Smart Scan baselines to show growth rate, confidence, category churn, and the largest reclaimable-storage drivers without pretending short-term noise is a forecast.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    title_box.append(&title);
    title_box.append(&subtitle);
    top.append(&title_box);
    let refresh = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh Storage Intelligence")
        .css_classes(["flat", "circular"])
        .build();
    top.append(&refresh);
    card.append(&top);

    let summary = gtk::Label::new(Some("Reading Smart Scan history…"));
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    card.append(&summary);

    let metrics = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    metrics.set_homogeneous(true);
    let reclaimable = gtk::Label::new(Some("—"));
    let delta = gtk::Label::new(Some("—"));
    let rate = gtk::Label::new(Some("—"));
    let confidence = gtk::Label::new(Some("—"));
    metrics.append(&runway_metric("SAFE RECLAIMABLE", &reclaimable));
    metrics.append(&runway_metric("BASELINE CHANGE", &delta));
    metrics.append(&runway_metric("DAILY RATE", &rate));
    metrics.append(&runway_metric("CONFIDENCE", &confidence));
    card.append(&metrics);

    let context = gtk::Label::new(Some("Waiting for storage history…"));
    context.set_xalign(0.0);
    context.set_wrap(true);
    context.add_css_class("dim-label");
    card.append(&context);

    let growth = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.append(&growth);

    let do_refresh: Rc<dyn Fn()> = Rc::new({
        let home = home.clone();
        let refresh = refresh.clone();
        let summary = summary.clone();
        let reclaimable = reclaimable.clone();
        let delta = delta.clone();
        let rate = rate.clone();
        let confidence = confidence.clone();
        let context = context.clone();
        let growth = growth.clone();
        move || {
            refresh.set_sensitive(false);
            let (tx, rx) = mpsc::channel();
            let home_thread = home.clone();
            std::thread::spawn(move || {
                let _ = tx.send(crate::storage_intelligence::collect_report(&home_thread));
            });
            let refresh_i = refresh.clone();
            let summary_i = summary.clone();
            let reclaimable_i = reclaimable.clone();
            let delta_i = delta.clone();
            let rate_i = rate.clone();
            let confidence_i = confidence.clone();
            let context_i = context.clone();
            let growth_i = growth.clone();
            glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                Ok(report) => {
                    summary_i.set_text(&report.summary);
                    reclaimable_i.set_text(&format::bytes(report.current_reclaimable_bytes));
                    delta_i.set_text(&format_signed_bytes(report.reclaimable_delta_bytes));
                    rate_i.set_text(
                        &report
                            .daily_growth_bytes
                            .map(format_signed_bytes)
                            .unwrap_or_else(|| "Learning".to_string()),
                    );
                    confidence_i.set_text(report.confidence_label);
                    context_i.set_text(&format!(
                        "{} Smart Scan sample(s) • baseline age {} • category churn {}",
                        report.sample_count,
                        format_duration_compact(report.baseline_age_secs),
                        format::bytes(report.category_churn_bytes)
                    ));
                    while let Some(child) = growth_i.first_child() {
                        growth_i.remove(&child);
                    }
                    if report.growth_categories.is_empty() {
                        growth_i.append(&empty_state_label(
                            "No category moved by at least 32 MiB against the comparison scan.",
                        ));
                    } else {
                        for item in report.growth_categories.iter().take(5) {
                            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                            let name = gtk::Label::new(Some(&item.category));
                            name.set_xalign(0.0);
                            name.set_hexpand(true);
                            let current = gtk::Label::new(Some(&format::bytes(item.current_bytes)));
                            current.add_css_class("dim-label");
                            let change =
                                gtk::Label::new(Some(&format_signed_bytes(item.delta_bytes)));
                            change.add_css_class(if item.delta_bytes > 0 {
                                "change-attention"
                            } else {
                                "change-positive"
                            });
                            row.append(&name);
                            row.append(&current);
                            row.append(&change);
                            growth_i.append(&row);
                        }
                    }
                    refresh_i.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    summary_i.set_text("Storage Intelligence worker stopped unexpectedly.");
                    refresh_i.set_sensitive(true);
                    glib::ControlFlow::Break
                }
            });
        }
    });
    {
        let do_refresh = do_refresh.clone();
        refresh.connect_clicked(move |_| do_refresh());
    }
    do_refresh();
    card.upcast()
}

fn format_duration_compact(seconds: u64) -> String {
    if seconds >= 86_400 {
        format!("{}d", seconds / 86_400)
    } else if seconds >= 3_600 {
        format!("{}h", seconds / 3_600)
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}s", seconds)
    }
}

fn format_signed_bytes(value: i128) -> String {
    if value > 0 {
        format!("+{}", format::bytes(value as u64))
    } else if value < 0 {
        format!("−{}", format::bytes(value.unsigned_abs() as u64))
    } else {
        "No material change".to_string()
    }
}

fn storage_analyzer_page(home: PathBuf) -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Storage Analyzer"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");

    let analyze_btn = gtk::Button::with_label("Analyze Directories");
    analyze_btn.add_css_class("suggested-action");
    analyze_btn.add_css_class("pill");

    let large_files_btn = gtk::Button::with_label("Find Large Files (>100MB)");
    large_files_btn.add_css_class("pill");

    title_row.append(&title);
    title_row.append(&analyze_btn);
    title_row.append(&large_files_btn);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Breakdown of space-consuming directories and large files in your home environment.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);
    page.append(&storage_runway_panel(home.clone()));
    page.append(&storage_intelligence_panel(home.clone()));

    let container = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&container);

    let home_c = home.clone();
    let container_c = container.clone();
    let analyze_btn_c = analyze_btn.clone();
    let large_files_btn_c = large_files_btn.clone();

    analyze_btn.connect_clicked(move |_| {
        while let Some(child) = container_c.first_child() {
            container_c.remove(&child);
        }
        analyze_btn_c.set_sensitive(false);
        large_files_btn_c.set_sensitive(false);
        let loading = gtk::Label::new(Some("Analyzing home directories…"));
        loading.add_css_class("dim-label");
        container_c.append(&loading);

        let (tx, rx) = mpsc::channel();
        let home_thread = home_c.clone();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        std::thread::spawn(move || {
            let usages = crate::storage::analyze_home(&home_thread, cancel);
            let _ = tx.send(usages);
        });

        let container_ui = container_c.clone();
        let btn_ui = analyze_btn_c.clone();
        let large_btn_ui = large_files_btn_c.clone();

        glib::timeout_add_local(Duration::from_millis(100), move || {
            if let Ok(usages) = rx.try_recv() {
                while let Some(child) = container_ui.first_child() {
                    container_ui.remove(&child);
                }
                btn_ui.set_sensitive(true);
                large_btn_ui.set_sensitive(true);
                if usages.is_empty() {
                    let empty = gtk::Label::new(Some("No directory data found."));
                    empty.add_css_class("dim-label");
                    container_ui.append(&empty);
                } else {
                    let total_bytes: u64 = usages.iter().map(|u| u.bytes).sum();
                    // Append interactive Donut Chart above directory list
                    container_ui.append(&make_donut_chart(&usages));

                    for usage in usages {
                        let card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
                        card.add_css_class("finding-card");

                        let icon = gtk::Image::from_icon_name("folder-symbolic");
                        icon.set_pixel_size(28);
                        card.append(&icon);

                        let info = gtk::Box::new(gtk::Orientation::Vertical, 4);
                        info.set_hexpand(true);

                        let name = gtk::Label::new(Some(&usage.name));
                        name.set_xalign(0.0);
                        name.add_css_class("heading");

                        let sub = gtk::Label::new(Some(&format!(
                            "{} files  •  {}",
                            usage.files_count,
                            usage.path.display()
                        )));
                        sub.set_xalign(0.0);
                        sub.add_css_class("dim-label");

                        let pct = if total_bytes > 0 {
                            (usage.bytes as f64 / total_bytes as f64) as f32
                        } else {
                            0.0
                        };
                        let bar = gtk::ProgressBar::new();
                        bar.set_fraction(pct as f64);

                        info.append(&name);
                        info.append(&sub);
                        info.append(&bar);
                        card.append(&info);

                        let size_label = gtk::Label::new(Some(&format::bytes(usage.bytes)));
                        size_label.add_css_class("title-3");
                        size_label.set_valign(gtk::Align::Center);
                        card.append(&size_label);

                        container_ui.append(&card);
                    }
                }
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    });

    let home_l = home.clone();
    let container_l = container.clone();
    let analyze_btn_l = analyze_btn.clone();
    let large_files_btn_l = large_files_btn.clone();

    large_files_btn.connect_clicked(move |_| {
        while let Some(child) = container_l.first_child() {
            container_l.remove(&child);
        }
        analyze_btn_l.set_sensitive(false);
        large_files_btn_l.set_sensitive(false);
        let loading = gtk::Label::new(Some("Scanning for files larger than 100 MB…"));
        loading.add_css_class("dim-label");
        container_l.append(&loading);

        let (tx, rx) = mpsc::channel();
        let home_thread = home_l.clone();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        std::thread::spawn(move || {
            let files = crate::storage::find_large_files(&home_thread, 100 * 1024 * 1024, cancel);
            let _ = tx.send(files);
        });

        let container_ui = container_l.clone();
        let btn_ui = analyze_btn_l.clone();
        let large_btn_ui = large_files_btn_l.clone();

        glib::timeout_add_local(Duration::from_millis(100), move || {
            if let Ok(files) = rx.try_recv() {
                while let Some(child) = container_ui.first_child() {
                    container_ui.remove(&child);
                }
                btn_ui.set_sensitive(true);
                large_btn_ui.set_sensitive(true);

                if files.is_empty() {
                    let empty = gtk::Label::new(Some(
                        "No files larger than 100 MB found in home directory.",
                    ));
                    empty.add_css_class("dim-label");
                    container_ui.append(&empty);
                } else {
                    for f in files {
                        let card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
                        card.add_css_class("finding-card");

                        let icon = gtk::Image::from_icon_name("video-x-generic-symbolic");
                        icon.set_pixel_size(24);
                        card.append(&icon);

                        let info = gtk::Box::new(gtk::Orientation::Vertical, 4);
                        info.set_hexpand(true);

                        let name = gtk::Label::new(Some(&f.name));
                        name.set_xalign(0.0);
                        name.add_css_class("heading");

                        let sub = gtk::Label::new(Some(&f.path.display().to_string()));
                        sub.set_xalign(0.0);
                        sub.add_css_class("dim-label");

                        info.append(&name);
                        info.append(&sub);
                        card.append(&info);

                        let size_label = gtk::Label::new(Some(&format::bytes(f.bytes)));
                        size_label.add_css_class("title-3");
                        size_label.set_valign(gtk::Align::Center);
                        card.append(&size_label);

                        container_ui.append(&card);
                    }
                }
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    });

    let clamp = adw::Clamp::builder()
        .maximum_size(920)
        .tightening_threshold(700)
        .child(&page)
        .build();

    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn maintenance_page(home: PathBuf, toast_overlay: Rc<adw::ToastOverlay>) -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Safety & Maintenance Timeline"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let refresh = gtk::Button::with_label("Refresh");
    refresh.add_css_class("pill");
    title_row.append(&title);
    title_row.append(&refresh);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "User-space cleanup is protected by a short undo window. Quarantined data still occupies disk space until it is purged; privileged package/journal operations are recorded but are not presented as reversible.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let safety_title = gtk::Label::new(Some("Safety Quarantine"));
    safety_title.set_xalign(0.0);
    safety_title.add_css_class("title-2");
    page.append(&safety_title);

    let active = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&active);

    let timeline_title = gtk::Label::new(Some("Maintenance Timeline"));
    timeline_title.set_xalign(0.0);
    timeline_title.add_css_class("title-2");
    timeline_title.set_margin_top(8);
    page.append(&timeline_title);

    let timeline_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&timeline_box);

    let refresh_slot: RefreshSlot = Rc::new(RefCell::new(None));
    let active_c = active.clone();
    let timeline_c = timeline_box.clone();
    let home_c = home.clone();
    let toast_c = toast_overlay.clone();
    let slot_c = refresh_slot.clone();
    let refresh_action: RefreshAction = Rc::new(move || {
        render_maintenance(
            &active_c,
            &timeline_c,
            &home_c,
            toast_c.clone(),
            slot_c.clone(),
        );
    });
    *refresh_slot.borrow_mut() = Some(refresh_action.clone());
    refresh_action();

    let refresh_click = refresh_action.clone();
    refresh.connect_clicked(move |_| refresh_click());

    let refresh_map = refresh_action.clone();
    outer.connect_map(move |_| refresh_map());

    // Housekeeping while the page exists. Expired quarantine is also purged at app launch.
    let home_timer = home.clone();
    let toast_timer = toast_overlay.clone();
    let refresh_timer = refresh_action.clone();
    glib::timeout_add_local(Duration::from_secs(60), move || {
        let policy = crate::cleanup_policy::load(&home_timer).unwrap_or_default();
        if policy.auto_purge_expired {
            if let Ok(purged) = quarantine::purge_expired(&home_timer) {
                if !purged.is_empty() {
                    let mut total = 0u64;
                    for item in purged {
                        total = total.saturating_add(item.bytes);
                        let mut event = timeline::MaintenanceEvent::new(
                            timeline::MaintenanceKind::Purge,
                            format!("Undo window expired: {}", item.title),
                            "The protected copy was permanently removed after its undo window expired.",
                        );
                        event.actual_freed_bytes = item.bytes;
                        let _ = timeline::append(&home_timer, event);
                    }
                    toast_timer.add_toast(adw::Toast::new(&format!(
                        "Expired quarantine purged; {} actually freed.",
                        format::bytes(total)
                    )));
                    refresh_timer();
                }
            }
        }
        glib::ControlFlow::Continue
    });

    let clamp = adw::Clamp::builder()
        .maximum_size(920)
        .tightening_threshold(700)
        .child(&page)
        .build();
    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn render_maintenance(
    active: &gtk::Box,
    timeline_box: &gtk::Box,
    home: &std::path::Path,
    toast_overlay: Rc<adw::ToastOverlay>,
    refresh_slot: RefreshSlot,
) {
    while let Some(child) = active.first_child() {
        active.remove(&child);
    }
    while let Some(child) = timeline_box.first_child() {
        timeline_box.remove(&child);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let records = quarantine::load(home).unwrap_or_default();
    let protected: u64 = records.iter().map(|record| record.estimated_bytes).sum();

    let summary = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    summary.add_css_class("finding-card");
    let summary_text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    summary_text.set_hexpand(true);
    let headline = gtk::Label::new(Some(&format!(
        "{} protected for Undo",
        format::bytes(protected)
    )));
    headline.set_xalign(0.0);
    headline.add_css_class("heading");
    let detail = gtk::Label::new(Some(if records.is_empty() {
        "No active quarantine. User-space cleanup will create an undoable protected copy before permanent purge."
    } else {
        "Protected bytes are not yet free disk space. Purge is permanent; Undo restores only when the recreated target is still empty."
    }));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    summary_text.append(&headline);
    summary_text.append(&detail);
    summary.append(&summary_text);
    active.append(&summary);

    for record in records {
        let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        card.add_css_class("finding-card");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text.set_hexpand(true);
        let title = gtk::Label::new(Some(&record.title));
        title.set_xalign(0.0);
        title.add_css_class("heading");
        let remaining = record.expires_unix.saturating_sub(now);
        let time_label = if remaining == 0 {
            "eligible for purge now".to_string()
        } else {
            format!("{} min undo window remaining", remaining.div_ceil(60))
        };
        let detail = gtk::Label::new(Some(&format!(
            "{} protected • {time_label}",
            format::bytes(record.estimated_bytes)
        )));
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        detail.add_css_class("dim-label");
        text.append(&title);
        text.append(&detail);
        card.append(&text);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        actions.set_valign(gtk::Align::Center);
        let undo = gtk::Button::with_label("Undo");
        undo.add_css_class("suggested-action");
        let restore_aside = gtk::Button::with_label("Restore Aside");
        restore_aside.set_tooltip_text(Some(
            "Recover the quarantined data beside the recreated target instead of overwriting new data",
        ));
        let purge = gtk::Button::with_label("Purge Now");
        purge.add_css_class("destructive-action");
        actions.append(&undo);
        actions.append(&restore_aside);
        actions.append(&purge);
        card.append(&actions);
        active.append(&card);

        let home_undo = home.to_path_buf();
        let id_undo = record.id.clone();
        let title_undo = record.title.clone();
        let toast_undo = toast_overlay.clone();
        let slot_undo = refresh_slot.clone();
        undo.connect_clicked(move |button| {
            button.set_sensitive(false);
            match quarantine::restore(&home_undo, &id_undo) {
                Ok(bytes) => {
                    let mut event = timeline::MaintenanceEvent::new(
                        timeline::MaintenanceKind::Restore,
                        format!("Restored {title_undo}"),
                        "LinuxCare restored the quarantined user-space directory. No privileged system operation was reversed.",
                    );
                    event.restored_bytes = bytes;
                    let _ = timeline::append(&home_undo, event);
                    toast_undo.add_toast(adw::Toast::new(&format!(
                        "Undo complete: {} restored.",
                        format::bytes(bytes)
                    )));
                }
                Err(err) => {
                    toast_undo.add_toast(adw::Toast::new(&format!("Undo blocked: {err}")));
                }
            }
            let refresh = slot_undo.borrow().as_ref().cloned();
            if let Some(refresh) = refresh {
                refresh();
            }
        });

        let home_aside = home.to_path_buf();
        let id_aside = record.id.clone();
        let title_aside = record.title.clone();
        let toast_aside = toast_overlay.clone();
        let slot_aside = refresh_slot.clone();
        restore_aside.connect_clicked(move |button| {
            button.set_sensitive(false);
            match quarantine::restore_aside(&home_aside, &id_aside) {
                Ok((bytes, _destinations)) => {
                    let mut event = timeline::MaintenanceEvent::new(
                        timeline::MaintenanceKind::Restore,
                        format!("Recovered {title_aside} beside recreated data"),
                        "LinuxCare preserved the newer target and restored the quarantined copy to a separate sibling directory.",
                    );
                    event.restored_bytes = bytes;
                    let _ = timeline::append(&home_aside, event);
                    toast_aside.add_toast(adw::Toast::new(&format!(
                        "Recovered separately: {} restored.",
                        format::bytes(bytes)
                    )));
                }
                Err(err) => {
                    toast_aside.add_toast(adw::Toast::new(&format!("Separate recovery failed: {err}")));
                }
            }
            let refresh = slot_aside.borrow().as_ref().cloned();
            if let Some(refresh) = refresh {
                refresh();
            }
        });

        let home_purge = home.to_path_buf();
        let id_purge = record.id.clone();
        let title_purge = record.title.clone();
        let toast_purge = toast_overlay.clone();
        let slot_purge = refresh_slot.clone();
        let armed = Rc::new(Cell::new(false));
        let armed_purge = armed.clone();
        purge.connect_clicked(move |button| {
            if !armed_purge.get() {
                armed_purge.set(true);
                button.set_label("Click again to permanently purge");
                let armed_reset = armed_purge.clone();
                let button_reset = button.clone();
                glib::timeout_add_local_once(Duration::from_secs(5), move || {
                    armed_reset.set(false);
                    button_reset.set_label("Purge Now");
                });
                return;
            }
            button.set_sensitive(false);
            match quarantine::purge(&home_purge, &id_purge) {
                Ok(bytes) => {
                    let mut event = timeline::MaintenanceEvent::new(
                        timeline::MaintenanceKind::Purge,
                        format!("Purged {title_purge}"),
                        "The protected copy was permanently removed. This action cannot be undone by LinuxCare.",
                    );
                    event.actual_freed_bytes = bytes;
                    let _ = timeline::append(&home_purge, event);
                    toast_purge.add_toast(adw::Toast::new(&format!(
                        "Purge complete: about {} actually freed.",
                        format::bytes(bytes)
                    )));
                }
                Err(err) => {
                    toast_purge.add_toast(adw::Toast::new(&format!("Purge failed: {err}")));
                }
            }
            let refresh = slot_purge.borrow().as_ref().cloned();
            if let Some(refresh) = refresh {
                refresh();
            }
        });
    }

    let events = timeline::load(home).unwrap_or_default();
    if events.is_empty() {
        let empty = adw::StatusPage::builder()
            .title("No maintenance timeline yet")
            .description("Smart Scan, cleanup, Undo, and purge activity from alpha.9 onward will be recorded locally.")
            .icon_name("document-open-recent-symbolic")
            .build();
        timeline_box.append(&empty);
    } else {
        for event in events.into_iter().take(120) {
            let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            card.add_css_class("finding-card");
            let icon = gtk::Image::from_icon_name(event.kind.icon());
            icon.set_pixel_size(20);
            icon.set_valign(gtk::Align::Start);
            card.append(&icon);

            let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
            text.set_hexpand(true);
            let headline = gtk::Label::new(Some(&event.title));
            headline.set_xalign(0.0);
            headline.add_css_class("heading");
            let detail = gtk::Label::new(Some(&event.detail));
            detail.set_xalign(0.0);
            detail.set_wrap(true);
            detail.add_css_class("dim-label");
            let mut meta = vec![human_age(now.saturating_sub(event.timestamp_unix))];
            if event.actual_freed_bytes > 0 {
                meta.push(format!("{} freed", format::bytes(event.actual_freed_bytes)));
            }
            if event.protected_bytes > 0 {
                meta.push(format!(
                    "{} protected",
                    format::bytes(event.protected_bytes)
                ));
            }
            if event.restored_bytes > 0 {
                meta.push(format!("{} restored", format::bytes(event.restored_bytes)));
            }
            if event.observed_bytes > 0 {
                meta.push(format!(
                    "{} candidates",
                    format::bytes(event.observed_bytes)
                ));
            }
            if event.operations > 0 {
                meta.push(format!("{} operation(s)", event.operations));
            }
            if event.errors > 0 {
                meta.push(format!("{} error(s)", event.errors));
            }
            if event.privileged {
                meta.push("privileged".to_string());
            }
            let meta = gtk::Label::new(Some(&meta.join(" • ")));
            meta.set_xalign(0.0);
            meta.add_css_class("caption");
            text.append(&headline);
            text.append(&detail);
            text.append(&meta);
            card.append(&text);

            let badge = gtk::Label::new(Some(event.kind.label()));
            badge.add_css_class("risk-badge");
            badge.add_css_class(match event.kind {
                timeline::MaintenanceKind::Restore => "risk-safe",
                timeline::MaintenanceKind::Purge => "risk-advanced",
                timeline::MaintenanceKind::Cleanup => "risk-review",
                timeline::MaintenanceKind::Scan => "risk-safe",
            });
            badge.set_valign(gtk::Align::Center);
            card.append(&badge);
            timeline_box.append(&card);
        }
    }

    // Preserve visibility of pre-alpha.9 cleanup records without duplicating them into
    // the new timeline store or pretending they had quarantine semantics.
    let legacy: Vec<_> = history::load(home)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.version != env!("CARGO_PKG_VERSION"))
        .collect();
    if !legacy.is_empty() {
        let legacy_title = gtk::Label::new(Some("Earlier cleanup history"));
        legacy_title.set_xalign(0.0);
        legacy_title.add_css_class("heading");
        legacy_title.set_margin_top(8);
        timeline_box.append(&legacy_title);
        for entry in legacy.into_iter().take(20) {
            let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
            row.add_css_class("finding-card");
            let headline = gtk::Label::new(Some(&format!(
                "{} reported recovered",
                format::bytes(entry.recovered_bytes)
            )));
            headline.set_xalign(0.0);
            let detail = gtk::Label::new(Some(&format!(
                "Legacy record • {} operation(s) • {} error(s) • {}",
                entry.operations,
                entry.errors,
                human_age(now.saturating_sub(entry.timestamp_unix))
            )));
            detail.set_xalign(0.0);
            detail.add_css_class("dim-label");
            row.append(&headline);
            row.append(&detail);
            timeline_box.append(&row);
        }
    }
}

fn human_age(seconds: u64) -> String {
    match seconds {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", seconds / 60),
        3600..=86_399 => format!("{} h ago", seconds / 3600),
        _ => format!("{} d ago", seconds / 86_400),
    }
}

#[derive(Clone)]
struct FindingRow {
    widget: gtk::Widget,
    check_btn: Option<gtk::CheckButton>,
    title_lower: String,
    path_lower: String,
}

#[allow(clippy::too_many_arguments)]
fn apply_profile_filter(
    profile_name: &str,
    predicate: impl Fn(&CleanupCandidate) -> bool,
    all_candidates: &Rc<RefCell<Vec<CleanupCandidate>>>,
    selected_ids: &Rc<RefCell<HashSet<String>>>,
    findings: &gtk::Box,
    finding_rows: &Rc<RefCell<Vec<FindingRow>>>,
    preview_button: &gtk::Button,
    preview: &gtk::Box,
    preview_text: &gtk::Label,
    toast_overlay: &Rc<adw::ToastOverlay>,
) {
    let cands = all_candidates.borrow();
    if cands.is_empty() {
        toast_overlay.add_toast(adw::Toast::new(
            "Please run Smart Scan first to analyze your system.",
        ));
        return;
    }

    let mut sel = selected_ids.borrow_mut();
    sel.clear();
    for c in cands.iter() {
        if c.executable_now() && predicate(c) {
            sel.insert(c.id.clone());
        }
    }
    let count = sel.len();
    drop(sel);

    while let Some(child) = findings.first_child() {
        findings.remove(&child);
    }
    finding_rows.borrow_mut().clear();
    for c in cands.iter() {
        let (card, check_btn) = candidate_card(c, selected_ids.clone(), preview_button.clone());
        findings.append(&card);
        finding_rows.borrow_mut().push(FindingRow {
            widget: card,
            check_btn,
            title_lower: c.title.to_lowercase(),
            path_lower: c
                .path
                .as_ref()
                .map(|p| p.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
        });
    }

    let ids = selected_ids.borrow();
    let selected: Vec<_> = cands
        .iter()
        .filter(|x| ids.contains(&x.id) && x.executable_now())
        .collect();
    let bytes: u64 = selected.iter().map(|x| x.size_bytes).sum();
    let safe = selected
        .iter()
        .filter(|x| x.risk == CleanupRisk::Safe)
        .count();
    let review = selected
        .iter()
        .filter(|x| x.risk == CleanupRisk::Review)
        .count();
    let advanced = selected
        .iter()
        .filter(|x| x.risk == CleanupRisk::Advanced)
        .count();
    let auth = selected.iter().filter(|x| x.requires_privilege).count();

    preview_text.set_text(&format!(
        "{profile_name}: {} operation(s) selected • approximately {} involved • SAFE: {} • REVIEW: {} • ADVANCED: {} • {} require authentication.",
        selected.len(),
        format::bytes(bytes),
        safe,
        review,
        advanced,
        auth
    ));
    preview.set_visible(true);
    preview_button.set_sensitive(!ids.is_empty());
    toast_overlay.add_toast(adw::Toast::new(&format!(
        "Selected {count} item(s) for {profile_name} ({})",
        format::bytes(bytes)
    )));
}

struct Dashboard {
    root: gtk::Widget,
    health_refresh: Option<Rc<dyn Fn()>>,
}

impl Dashboard {
    fn new(
        home: PathBuf,
        toast_overlay: Rc<adw::ToastOverlay>,
        show_health_intelligence: bool,
        shared_health_refresh: Option<Rc<dyn Fn()>>,
    ) -> Self {
        let outer = gtk::ScrolledWindow::new();
        let page = gtk::Box::new(gtk::Orientation::Vertical, 20);
        page.set_margin_top(28);
        page.set_margin_bottom(28);
        page.set_margin_start(24);
        page.set_margin_end(24);
        page.set_hexpand(true);

        let intro = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let heading = gtk::Label::new(Some("System maintenance, without guesswork"));
        heading.set_xalign(0.0);
        heading.add_css_class("title-1");
        let sub = gtk::Label::new(Some("LinuxCare explains what it finds, how much space is involved, and the risk before anything is removed."));
        sub.set_xalign(0.0);
        sub.set_wrap(true);
        sub.add_css_class("dim-label");
        intro.append(&heading);
        intro.append(&sub);
        page.append(&intro);

        let own_health_refresh: Option<Rc<dyn Fn()>> = if show_health_intelligence {
            let health_panel = health_intelligence_panel(home.clone());
            page.append(&health_panel.root);
            Some(health_panel.refresh)
        } else {
            None
        };
        let health_refresh = own_health_refresh
            .clone()
            .or(shared_health_refresh)
            .unwrap_or_else(|| Rc::new(|| {}));
        page.append(&system_overview_card());

        let profiles_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        profiles_box.set_margin_top(2);
        profiles_box.set_margin_bottom(2);

        let p_label = gtk::Label::new(Some("1-Click Profiles:"));
        p_label.set_valign(gtk::Align::Center);
        p_label.add_css_class("caption-heading");
        p_label.add_css_class("dim-label");
        profiles_box.append(&p_label);

        let p_safe = gtk::Button::with_label("Safe Clean");
        p_safe.add_css_class("pill");
        p_safe.add_css_class("profile-pill");
        p_safe.set_tooltip_text(Some(
            "Select all Safe risk items (Trash, Caches, Thumbnails)",
        ));

        let p_dev = gtk::Button::with_label("💻 Developer Clean");
        p_dev.add_css_class("pill");
        p_dev.add_css_class("profile-pill");
        p_dev.set_tooltip_text(Some("Select Developer Caches (Cargo, NPM, Pip, Uv, Yarn)"));

        let p_privacy = gtk::Button::with_label("🔒 Privacy Wipe");
        p_privacy.add_css_class("pill");
        p_privacy.add_css_class("profile-pill");
        p_privacy.set_tooltip_text(Some(
            "Select Privacy & History items (Thumbnails, Crash Reports, Trash)",
        ));

        profiles_box.append(&p_safe);
        profiles_box.append(&p_dev);
        profiles_box.append(&p_privacy);
        page.append(&profiles_box);

        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 28);
        hero.add_css_class("hero-card");
        hero.set_margin_top(4);

        let progress_target = Rc::new(Cell::new(0.0f64));
        let progress_display = Rc::new(Cell::new(0.0f64));
        let spinning = Rc::new(Cell::new(false));
        let phase = Rc::new(Cell::new(0.0f64));
        let pulse_phase = Rc::new(Cell::new(0.0f64));
        let pulse_val = Rc::new(Cell::new(0.0f64));
        let success_flash = Rc::new(Cell::new(0.0f64));

        let scanner = make_scanner(
            progress_display.clone(),
            spinning.clone(),
            phase.clone(),
            pulse_val.clone(),
            success_flash.clone(),
        );
        hero.append(&scanner);

        let metrics = gtk::Box::new(gtk::Orientation::Vertical, 10);
        metrics.set_hexpand(true);
        metrics.set_valign(gtk::Align::Center);
        let reclaim = gtk::Label::new(Some("—"));
        reclaim.set_xalign(0.0);
        reclaim.add_css_class("hero-number");
        let reclaim_caption = gtk::Label::new(Some("SAFE CLEANUP CANDIDATES"));
        reclaim_caption.set_xalign(0.0);
        reclaim_caption.add_css_class("caption-heading");
        reclaim_caption.add_css_class("dim-label");
        let current = gtk::Label::new(Some("Ready for a safety-first scan"));
        current.set_xalign(0.0);
        current.set_wrap(true);
        let stats = gtk::Label::new(Some("0 files • 0 directories"));
        stats.set_xalign(0.0);
        stats.add_css_class("dim-label");
        let scan_button = gtk::Button::with_label("Start Smart Scan");
        scan_button.add_css_class("suggested-action");
        scan_button.add_css_class("pill");
        let preview_button = gtk::Button::with_label("Preview cleanup");
        preview_button.add_css_class("pill");
        preview_button.set_sensitive(false);
        let cancel_button = gtk::Button::with_label("Cancel");
        cancel_button.add_css_class("pill");
        cancel_button.set_sensitive(false);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.append(&scan_button);
        actions.append(&preview_button);
        actions.append(&cancel_button);
        metrics.append(&reclaim);
        metrics.append(&reclaim_caption);
        metrics.append(&current);
        metrics.append(&stats);
        metrics.append(&actions);
        hero.append(&metrics);
        page.append(&hero);

        let preview = gtk::Box::new(gtk::Orientation::Vertical, 10);
        preview.add_css_class("preview-card");
        preview.set_visible(false);
        let preview_title = gtk::Label::new(Some("Ready to clean"));
        preview_title.set_xalign(0.0);
        preview_title.add_css_class("title-3");
        let preview_text = gtk::Label::new(None);
        preview_text.set_xalign(0.0);
        preview_text.set_wrap(true);
        let cleanup_bar = gtk::ProgressBar::new();
        cleanup_bar.set_visible(false);

        let cleanup_actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let confirm = gtk::Button::with_label("Confirm selected cleanup");
        confirm.add_css_class("destructive-action");
        confirm.add_css_class("pill");
        confirm.set_halign(gtk::Align::Start);

        let stop_cleanup = gtk::Button::with_label("Stop Cleanup");
        stop_cleanup.add_css_class("pill");
        stop_cleanup.set_visible(false);

        cleanup_actions.append(&confirm);
        cleanup_actions.append(&stop_cleanup);

        preview.append(&preview_title);
        preview.append(&preview_text);
        preview.append(&cleanup_bar);
        preview.append(&cleanup_actions);
        page.append(&preview);

        let risk_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        risk_row.set_homogeneous(true);
        let safe_value = gtk::Label::new(Some("0 B"));
        let review_value = gtk::Label::new(Some("0 B"));
        let advanced_value = gtk::Label::new(Some("0 B"));
        risk_row.append(&risk_card(
            "SAFE",
            &safe_value,
            "Usually removable without meaningful consequences.",
            "risk-safe",
        ));
        risk_row.append(&risk_card(
            "REVIEW",
            &review_value,
            "Useful to inspect before clearing.",
            "risk-review",
        ));
        risk_row.append(&risk_card(
            "ADVANCED",
            &advanced_value,
            "Needs technical understanding.",
            "risk-advanced",
        ));
        page.append(&risk_row);

        let finding_rows: Rc<RefCell<Vec<FindingRow>>> = Rc::new(RefCell::new(Vec::new()));

        let findings_header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let section_title = gtk::Label::new(Some("Scan Findings"));
        section_title.set_xalign(0.0);
        section_title.set_hexpand(true);
        section_title.add_css_class("title-2");
        findings_header.append(&section_title);

        let select_all_btn = gtk::Button::with_label("Select All");
        select_all_btn.add_css_class("pill");
        let deselect_all_btn = gtk::Button::with_label("Deselect All");
        deselect_all_btn.add_css_class("pill");

        findings_header.append(&select_all_btn);
        findings_header.append(&deselect_all_btn);
        page.append(&findings_header);

        let search_bar = gtk::SearchEntry::new();
        search_bar.set_placeholder_text(Some("🔍 Filter findings by name or path…"));
        search_bar.set_margin_bottom(6);
        page.append(&search_bar);

        let findings = gtk::Box::new(gtk::Orientation::Vertical, 10);
        page.append(&findings);

        {
            let finding_rows_c = finding_rows.clone();
            search_bar.connect_search_changed(move |sb| {
                let text = sb.text().to_string().to_lowercase();
                for r in finding_rows_c.borrow().iter() {
                    let matches = text.is_empty()
                        || r.title_lower.contains(&text)
                        || r.path_lower.contains(&text);
                    r.widget.set_visible(matches);
                }
            });
        }

        {
            let finding_rows_c = finding_rows.clone();
            select_all_btn.connect_clicked(move |_| {
                for r in finding_rows_c.borrow().iter() {
                    if r.widget.is_visible() {
                        if let Some(ref cb) = r.check_btn {
                            cb.set_active(true);
                        }
                    }
                }
            });
        }

        {
            let finding_rows_c = finding_rows.clone();
            deselect_all_btn.connect_clicked(move |_| {
                for r in finding_rows_c.borrow().iter() {
                    if let Some(ref cb) = r.check_btn {
                        cb.set_active(false);
                    }
                }
            });
        }

        let clamp = adw::Clamp::builder()
            .maximum_size(940)
            .tightening_threshold(700)
            .child(&page)
            .build();

        outer.set_child(Some(&clamp));

        let all_candidates: Rc<RefCell<Vec<CleanupCandidate>>> = Rc::new(RefCell::new(Vec::new()));
        let selected_ids: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));
        let active_handle: Rc<RefCell<Option<ScanHandle>>> = Rc::new(RefCell::new(None));
        let active_cleanup_handle: Rc<RefCell<Option<executor::CleanupHandle>>> =
            Rc::new(RefCell::new(None));

        // Animation loop for scanner drawing area
        {
            let scanner = scanner.clone();
            let phase = phase.clone();
            let spinning = spinning.clone();
            let pulse_phase = pulse_phase.clone();
            let pulse_val = pulse_val.clone();
            let progress_display = progress_display.clone();
            let progress_target = progress_target.clone();
            let success_flash = success_flash.clone();

            glib::timeout_add_local(Duration::from_millis(25), move || {
                let mut redraw_needed = false;

                // Smoothly interpolate progress toward target
                let cur = progress_display.get();
                let tgt = progress_target.get();
                let diff = tgt - cur;
                if diff.abs() > 0.001 {
                    progress_display.set(cur + diff * 0.15);
                    redraw_needed = true;
                }

                if spinning.get() && adw::is_animations_enabled(&scanner) {
                    phase.set((phase.get() + 0.024) % 1.0);
                    let pp = (pulse_phase.get() + 0.07) % std::f64::consts::TAU;
                    pulse_phase.set(pp);
                    pulse_val.set((pp.sin() + 1.0) / 2.0);
                    redraw_needed = true;
                }

                if success_flash.get() > 0.0 {
                    success_flash.set((success_flash.get() - 0.025).max(0.0));
                    redraw_needed = true;
                }

                if redraw_needed {
                    scanner.queue_draw();
                }

                glib::ControlFlow::Continue
            });
        }

        {
            let home = home.clone();
            let findings = findings.clone();
            let current = current.clone();
            let stats = stats.clone();
            let reclaim = reclaim.clone();
            let safe_value = safe_value.clone();
            let review_value = review_value.clone();
            let advanced_value = advanced_value.clone();
            let all_candidates = all_candidates.clone();
            let selected_ids = selected_ids.clone();
            let finding_rows_c = finding_rows.clone();
            let active_handle = active_handle.clone();
            let progress_target = progress_target.clone();
            let spinning = spinning.clone();
            let scanner = scanner.clone();
            let scan_button_c = scan_button.clone();
            let preview_button_c = preview_button.clone();
            let cancel_button_c = cancel_button.clone();
            let preview = preview.clone();
            let success_flash_c = success_flash.clone();
            let toast_overlay_c = toast_overlay.clone();
            let health_refresh_c = health_refresh.clone();

            scan_button.connect_clicked(move |_| {
                while let Some(child) = findings.first_child() {
                    findings.remove(&child);
                }
                all_candidates.borrow_mut().clear();
                selected_ids.borrow_mut().clear();
                finding_rows_c.borrow_mut().clear();
                preview.set_visible(false);
                reclaim.set_text("0 B");
                safe_value.set_text("0 B");
                review_value.set_text("0 B");
                advanced_value.set_text("0 B");
                stats.set_text("0 files • 0 directories");
                current.set_text("Preparing scan modules…");
                progress_target.set(0.0);
                spinning.set(true);
                scanner.queue_draw();
                scan_button_c.set_sensitive(false);
                preview_button_c.set_sensitive(false);
                cancel_button_c.set_sensitive(true);

                let (tx, rx) = mpsc::channel();
                let current_exclusions = settings::load(&home).unwrap_or_default().exclusions;
                *active_handle.borrow_mut() = Some(scan::start_scan(home.clone(), current_exclusions, tx));
                let total_files = Rc::new(Cell::new(0u64));
                let total_dirs = Rc::new(Cell::new(0u64));
                let total_modules = Rc::new(Cell::new(1usize));
                let rx = Rc::new(RefCell::new(rx));

                let findings_i = findings.clone();
                let current_i = current.clone();
                let stats_i = stats.clone();
                let reclaim_i = reclaim.clone();
                let safe_i = safe_value.clone();
                let review_i = review_value.clone();
                let advanced_i = advanced_value.clone();
                let all_i = all_candidates.clone();
                let selected_i = selected_ids.clone();
                let finding_rows_i = finding_rows_c.clone();
                let progress_target_i = progress_target.clone();
                let spinning_i = spinning.clone();
                let scanner_i = scanner.clone();
                let scan_button_i = scan_button_c.clone();
                let preview_button_i = preview_button_c.clone();
                let cancel_button_i = cancel_button_c.clone();
                let active_i = active_handle.clone();
                let total_files_i = total_files.clone();
                let total_dirs_i = total_dirs.clone();
                let total_modules_i = total_modules.clone();
                let success_flash_i = success_flash_c.clone();
                let toast_i = toast_overlay_c.clone();
                let health_refresh_i = health_refresh_c.clone();
                let home_i = home.clone();

                glib::timeout_add_local(Duration::from_millis(60), move || {
                    loop {
                        let event = match rx.borrow().try_recv() {
                            Ok(event) => event,
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => {
                                current_i.set_text("Scan worker stopped unexpectedly. Partial results are preserved.");
                                spinning_i.set(false);
                                scan_button_i.set_sensitive(true);
                                preview_button_i.set_sensitive(!selected_i.borrow().is_empty());
                                cancel_button_i.set_sensitive(false);
                                active_i.borrow_mut().take();
                                return glib::ControlFlow::Break;
                            }
                        };
                        match event {
                            ScanEvent::Started { total_modules } => total_modules_i.set(total_modules.max(1)),
                            ScanEvent::ModuleStarted { index, name } => {
                                current_i.set_text(&format!("Scanning {name}…"));
                                progress_target_i.set(index as f64 / total_modules_i.get() as f64);
                                scanner_i.queue_draw();
                            }
                            ScanEvent::ModuleFinished { index, result } => {
                                total_files_i.set(total_files_i.get() + result.files_scanned);
                                total_dirs_i.set(total_dirs_i.get() + result.directories_scanned);
                                stats_i.set_text(&format!("{} files • {} directories", total_files_i.get(), total_dirs_i.get()));
                                for candidate in result.candidates {
                                    if candidate.selected_by_default() {
                                        selected_i.borrow_mut().insert(candidate.id.clone());
                                    }
                                    let (card, check_btn) = candidate_card(&candidate, selected_i.clone(), preview_button_i.clone());
                                    findings_i.append(&card);
                                    finding_rows_i.borrow_mut().push(FindingRow {
                                        widget: card,
                                        check_btn,
                                        title_lower: candidate.title.to_lowercase(),
                                        path_lower: candidate.path.as_ref().map(|p| p.to_string_lossy().to_lowercase()).unwrap_or_default(),
                                    });
                                    all_i.borrow_mut().push(candidate);
                                }
                                for warning in result.warnings {
                                    findings_i.append(&warning_card(&warning));
                                }
                                let cands = all_i.borrow();
                                let sum = |risk| cands.iter().filter(|c| c.risk == risk).map(|c| c.size_bytes).sum::<u64>();
                                let safe = sum(CleanupRisk::Safe);
                                let review = sum(CleanupRisk::Review);
                                let advanced = sum(CleanupRisk::Advanced);
                                safe_i.set_text(&format::bytes(safe));
                                review_i.set_text(&format::bytes(review));
                                advanced_i.set_text(&format::bytes(advanced));
                                let now: u64 = cands.iter().filter(|c| c.reclaimable_now()).map(|c| c.size_bytes).sum();
                                reclaim_i.set_text(&format::bytes(now));
                                progress_target_i.set((index + 1) as f64 / total_modules_i.get() as f64);
                                scanner_i.queue_draw();
                            }
                            ScanEvent::Cancelled => {
                                current_i.set_text("Scan cancelled. Partial results are preserved for review.");
                                spinning_i.set(false);
                                scan_button_i.set_sensitive(true);
                                preview_button_i.set_sensitive(!selected_i.borrow().is_empty());
                                cancel_button_i.set_sensitive(false);
                                active_i.borrow_mut().take();
                                toast_i.add_toast(adw::Toast::new("Scan was cancelled"));
                                return glib::ControlFlow::Break;
                            }
                            ScanEvent::Finished => {
                                current_i.set_text("Scan complete. Nothing is removed until you preview and confirm.");
                                progress_target_i.set(1.0);
                                spinning_i.set(false);
                                success_flash_i.set(1.0);
                                scanner_i.queue_draw();
                                scan_button_i.set_sensitive(true);
                                preview_button_i.set_sensitive(!selected_i.borrow().is_empty());
                                cancel_button_i.set_sensitive(false);
                                active_i.borrow_mut().take();

                                let cands = all_i.borrow();
                                let safe_now: u64 = cands.iter().filter(|c| c.reclaimable_now()).map(|c| c.size_bytes).sum();
                                if let Err(err) = crate::health::record_scan_snapshot(&home_i, cands.as_slice()) {
                                    eprintln!("LinuxCare: could not save Smart Scan baseline: {err}");
                                }
                                let mut event = timeline::MaintenanceEvent::new(
                                    timeline::MaintenanceKind::Scan,
                                    "Smart Scan completed",
                                    format!(
                                        "{} candidate(s) were reviewed; nothing was removed by the scan itself.",
                                        cands.len()
                                    ),
                                );
                                event.observed_bytes = safe_now;
                                let _ = timeline::append(&home_i, event);
                                health_refresh_i();
                                toast_i.add_toast(adw::Toast::new(&format!(
                                    "✨ Scan complete! Found {} of safe cleanup candidates.",
                                    format::bytes(safe_now)
                                )));
                                return glib::ControlFlow::Break;
                            }
                        }
                    }
                    glib::ControlFlow::Continue
                });
            });
        }

        {
            let active_handle = active_handle.clone();
            cancel_button.connect_clicked(move |_| {
                if let Some(handle) = active_handle.borrow().as_ref() {
                    handle.cancel();
                }
            });
        }

        {
            let active_cleanup_handle = active_cleanup_handle.clone();
            let stop_btn_c = stop_cleanup.clone();
            stop_cleanup.connect_clicked(move |_| {
                if let Some(handle) = active_cleanup_handle.borrow().as_ref() {
                    handle.cancel();
                }
                stop_btn_c.set_sensitive(false);
            });
        }

        {
            let candidates = all_candidates.clone();
            let selected_ids = selected_ids.clone();
            let preview = preview.clone();
            let preview_text = preview_text.clone();
            preview_button.connect_clicked(move |_| {
                let ids = selected_ids.borrow();
                let candidates = candidates.borrow();
                let selected: Vec<_> = candidates.iter().filter(|x| ids.contains(&x.id) && x.executable_now()).collect();
                let bytes: u64 = selected.iter().map(|x| x.size_bytes).sum();
                let safe = selected.iter().filter(|x| x.risk == CleanupRisk::Safe).count();
                let review = selected.iter().filter(|x| x.risk == CleanupRisk::Review).count();
                let advanced = selected.iter().filter(|x| x.risk == CleanupRisk::Advanced).count();
                let auth = selected.iter().filter(|x| x.requires_privilege).count();
                let undoable_bytes: u64 = selected
                    .iter()
                    .filter(|x| x.execution == crate::model::ExecutionKind::UserAllowedDirectory)
                    .map(|x| x.size_bytes)
                    .sum();
                let immediate_bytes = bytes.saturating_sub(undoable_bytes);
                preview_text.set_text(&format!(
                    "{} selected operation(s) • approximately {} involved • {} will enter Safety Quarantine for Undo and will NOT be free disk space until purge • up to {} may be reclaimed immediately by non-quarantine operations • SAFE: {} • REVIEW: {} • ADVANCED: {} • {} require authentication. Privileged items are not presented as undoable and are revalidated by the root helper immediately before execution.",
                    selected.len(),
                    format::bytes(bytes),
                    format::bytes(undoable_bytes),
                    format::bytes(immediate_bytes),
                    safe,
                    review,
                    advanced,
                    auth
                ));
                preview.set_visible(true);
            });
        }

        {
            let all_candidates = all_candidates.clone();
            let selected_ids = selected_ids.clone();
            let findings = findings.clone();
            let finding_rows = finding_rows.clone();
            let preview_button = preview_button.clone();
            let preview = preview.clone();
            let preview_text = preview_text.clone();
            let toast_overlay = toast_overlay.clone();
            p_safe.connect_clicked(move |_| {
                apply_profile_filter(
                    "Safe Clean",
                    |c| c.risk == CleanupRisk::Safe,
                    &all_candidates,
                    &selected_ids,
                    &findings,
                    &finding_rows,
                    &preview_button,
                    &preview,
                    &preview_text,
                    &toast_overlay,
                );
            });
        }

        {
            let all_candidates = all_candidates.clone();
            let selected_ids = selected_ids.clone();
            let findings = findings.clone();
            let finding_rows = finding_rows.clone();
            let preview_button = preview_button.clone();
            let preview = preview.clone();
            let preview_text = preview_text.clone();
            let toast_overlay = toast_overlay.clone();
            p_dev.connect_clicked(move |_| {
                apply_profile_filter(
                    "Developer Clean",
                    |c| c.category == crate::model::CleanupCategory::DeveloperCache,
                    &all_candidates,
                    &selected_ids,
                    &findings,
                    &finding_rows,
                    &preview_button,
                    &preview,
                    &preview_text,
                    &toast_overlay,
                );
            });
        }

        {
            let all_candidates = all_candidates.clone();
            let selected_ids = selected_ids.clone();
            let findings = findings.clone();
            let finding_rows = finding_rows.clone();
            let preview_button = preview_button.clone();
            let preview = preview.clone();
            let preview_text = preview_text.clone();
            let toast_overlay = toast_overlay.clone();
            p_privacy.connect_clicked(move |_| {
                apply_profile_filter(
                    "Privacy Wipe",
                    |c| {
                        c.category == crate::model::CleanupCategory::CrashReports
                            || c.category == crate::model::CleanupCategory::ThumbnailCache
                            || c.category == crate::model::CleanupCategory::Trash
                    },
                    &all_candidates,
                    &selected_ids,
                    &findings,
                    &finding_rows,
                    &preview_button,
                    &preview,
                    &preview_text,
                    &toast_overlay,
                );
            });
        }

        {
            let candidates = all_candidates.clone();
            let selected_ids = selected_ids.clone();
            let home = home.clone();
            let preview_text = preview_text.clone();
            let cleanup_bar = cleanup_bar.clone();
            let progress_target = progress_target.clone();
            let spinning = spinning.clone();
            let scanner = scanner.clone();
            let current = current.clone();
            let reclaim = reclaim.clone();
            let success_flash = success_flash.clone();
            let toast_overlay_c = toast_overlay.clone();
            let stop_cleanup_c = stop_cleanup.clone();
            let active_cleanup_handle = active_cleanup_handle.clone();

            confirm.connect_clicked(move |button| {
                let ids = selected_ids.borrow();
                let selected: Vec<CleanupCandidate> = candidates
                    .borrow()
                    .iter()
                    .filter(|x| ids.contains(&x.id) && x.executable_now())
                    .cloned()
                    .collect();
                drop(ids);

                if selected.is_empty() {
                    preview_text.set_text("No executable cleanup operations are selected.");
                    return;
                }

                button.set_sensitive(false);
                stop_cleanup_c.set_visible(true);
                stop_cleanup_c.set_sensitive(true);
                cleanup_bar.set_visible(true);
                cleanup_bar.set_fraction(0.0);
                spinning.set(true);
                progress_target.set(0.0);
                scanner.queue_draw();

                let operation_count = selected.len();
                let (tx, rx) = mpsc::channel();
                *active_cleanup_handle.borrow_mut() = Some(executor::start_cleanup(selected, home.clone(), tx));
                let rx = Rc::new(RefCell::new(rx));
                let preview_text_i = preview_text.clone();
                let button_i = button.clone();
                let home_i = home.clone();
                let cleanup_bar_i = cleanup_bar.clone();
                let progress_target_i = progress_target.clone();
                let spinning_i = spinning.clone();
                let scanner_i = scanner.clone();
                let current_i = current.clone();
                let reclaim_i = reclaim.clone();
                let success_flash_i = success_flash.clone();
                let toast_i = toast_overlay_c.clone();
                let stop_btn_i = stop_cleanup_c.clone();
                let active_clean_i = active_cleanup_handle.clone();

                glib::timeout_add_local(Duration::from_millis(80), move || {
                    loop {
                        let event = match rx.borrow().try_recv() {
                            Ok(event) => event,
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => {
                                preview_text_i.set_text("Cleanup worker stopped unexpectedly. Run Smart Scan again before retrying.");
                                button_i.set_sensitive(true);
                                stop_btn_i.set_visible(false);
                                spinning_i.set(false);
                                scanner_i.queue_draw();
                                active_clean_i.borrow_mut().take();
                                return glib::ControlFlow::Break;
                            }
                        };

                        match event {
                            CleanupEvent::Started { operations } => {
                                preview_text_i.set_text(&format!("Starting {operations} cleanup operation(s)…"));
                                current_i.set_text("Executing cleanup operations…");
                            }
                            CleanupEvent::ActionStarted { index, title } => {
                                let frac = index as f64 / operation_count as f64;
                                cleanup_bar_i.set_fraction(frac);
                                progress_target_i.set(frac);
                                scanner_i.queue_draw();
                                preview_text_i.set_text(&format!("Cleaning {} of {}: {title}…", index + 1, operation_count));
                                current_i.set_text(&format!("Processing: {title}"));
                            }
                            CleanupEvent::ActionFinished {
                                index,
                                title,
                                recovered_bytes,
                                error,
                                operation_result,
                            } => {
                                let frac = (index + 1) as f64 / operation_count as f64;
                                cleanup_bar_i.set_fraction(frac);
                                progress_target_i.set(frac);
                                scanner_i.queue_draw();
                                if let Some(error) = error {
                                    preview_text_i.set_text(&format!("Operation {} failed: {error}", index + 1));
                                } else if operation_result.quarantined_bytes > 0 {
                                    preview_text_i.set_text(&format!(
                                        "Completed {title}; {} protected for Undo. No disk space is reclaimed from that protected copy until purge.",
                                        format::bytes(operation_result.quarantined_bytes)
                                    ));
                                } else {
                                    preview_text_i.set_text(&format!(
                                        "Completed {title}; actually freed about {}.",
                                        format::bytes(recovered_bytes)
                                    ));
                                }
                            }
                            CleanupEvent::Cancelled {
                                recovered_bytes,
                                errors,
                                results,
                            } => {
                                spinning_i.set(false);
                                scanner_i.queue_draw();
                                button_i.set_sensitive(true);
                                stop_btn_i.set_visible(false);
                                active_clean_i.borrow_mut().take();
                                current_i.set_text("Cleanup stopped by user.");
                                let protected_bytes: u64 =
                                    results.iter().map(|r| r.quarantined_bytes).sum();
                                preview_text_i.set_text(&format!(
                                    "Cleanup stopped. {} actually freed • {} protected for Undo across {} completed operation(s).",
                                    format::bytes(recovered_bytes),
                                    format::bytes(protected_bytes),
                                    results.len()
                                ));
                                toast_i.add_toast(adw::Toast::new("Cleanup stopped by user"));

                                if !results.is_empty() {
                                    let privileged_used = results.iter().any(|r| r.requires_privilege);
                                    let estimated_bytes = results.iter().map(|r| r.estimated_bytes).sum();
                                    let _ = history::append(
                                        &home_i,
                                        history::HistoryEntry::with_details(
                                            recovered_bytes,
                                            estimated_bytes,
                                            results.len(),
                                            errors.len(),
                                            privileged_used,
                                            results.clone(),
                                        ),
                                    );
                                    let mut event = timeline::MaintenanceEvent::new(
                                        timeline::MaintenanceKind::Cleanup,
                                        "Cleanup stopped",
                                        "The user stopped the cleanup. Completed operations remain recorded; undoable user-space data stays in Safety Quarantine.",
                                    );
                                    event.actual_freed_bytes = recovered_bytes;
                                    event.protected_bytes = protected_bytes;
                                    event.operations = results.len();
                                    event.errors = errors.len();
                                    event.privileged = privileged_used;
                                    let _ = timeline::append(&home_i, event);
                                }
                                return glib::ControlFlow::Break;
                            }
                            CleanupEvent::Finished {
                                recovered_bytes,
                                errors,
                                results,
                            } => {
                                cleanup_bar_i.set_fraction(1.0);
                                progress_target_i.set(1.0);
                                spinning_i.set(false);
                                success_flash_i.set(1.0);
                                scanner_i.queue_draw();
                                current_i.set_text("Cleanup complete!");
                                stop_btn_i.set_visible(false);
                                active_clean_i.borrow_mut().take();

                                // Rolling number counter animation for freed bytes
                                let target_bytes = recovered_bytes;
                                let step_count = 25;
                                let current_step = Rc::new(Cell::new(0usize));
                                let reclaim_roll = reclaim_i.clone();
                                let roll_step = current_step.clone();

                                glib::timeout_add_local(Duration::from_millis(35), move || {
                                    let step = roll_step.get() + 1;
                                    roll_step.set(step);
                                    let progress = step as f64 / step_count as f64;
                                    let ease = 1.0 - (1.0 - progress).powi(3);
                                    let cur_val = (target_bytes as f64 * ease) as u64;
                                    reclaim_roll.set_text(&format::bytes(cur_val));

                                    if step >= step_count {
                                        reclaim_roll.set_text(&format::bytes(target_bytes));
                                        glib::ControlFlow::Break
                                    } else {
                                        glib::ControlFlow::Continue
                                    }
                                });

                                let privileged_used = results.iter().any(|r| r.requires_privilege);
                                let estimated_bytes = results.iter().map(|r| r.estimated_bytes).sum();
                                let protected_bytes: u64 =
                                    results.iter().map(|r| r.quarantined_bytes).sum();
                                let history_result = history::append(
                                    &home_i,
                                    history::HistoryEntry::with_details(
                                        recovered_bytes,
                                        estimated_bytes,
                                        operation_count,
                                        errors.len(),
                                        privileged_used,
                                        results.clone(),
                                    ),
                                );
                                let mut event = timeline::MaintenanceEvent::new(
                                    timeline::MaintenanceKind::Cleanup,
                                    if errors.is_empty() {
                                        "Cleanup completed"
                                    } else {
                                        "Cleanup completed with errors"
                                    },
                                    if protected_bytes > 0 {
                                        "User-space data was moved to Safety Quarantine for Undo. Protected bytes are still allocated until purge; privileged and provider operations are not presented as reversible."
                                    } else {
                                        "Cleanup completed without an active user-space quarantine transaction."
                                    },
                                );
                                event.actual_freed_bytes = recovered_bytes;
                                event.protected_bytes = protected_bytes;
                                event.operations = operation_count;
                                event.errors = errors.len();
                                event.privileged = privileged_used;
                                let _ = timeline::append(&home_i, event);

                                let history_note = if history_result.is_ok() {
                                    " Cleanup history was saved."
                                } else {
                                    " Cleanup finished, but legacy history could not be saved."
                                };
                                if errors.is_empty() {
                                    preview_text_i.set_text(&format!(
                                        "✨ Cleanup complete: {} actually freed • {} protected for Undo.{} Open Safety & Timeline to Undo or purge protected data.",
                                        format::bytes(recovered_bytes),
                                        format::bytes(protected_bytes),
                                        history_note
                                    ));
                                    toast_i.add_toast(adw::Toast::new(&format!(
                                        "Cleanup complete: {} freed • {} protected",
                                        format::bytes(recovered_bytes),
                                        format::bytes(protected_bytes)
                                    )));
                                } else {
                                    preview_text_i.set_text(&format!(
                                        "Cleanup completed with {} error(s): {} actually freed • {} protected for Undo.{}",
                                        errors.len(),
                                        format::bytes(recovered_bytes),
                                        format::bytes(protected_bytes),
                                        history_note
                                    ));
                                    toast_i.add_toast(adw::Toast::new(&format!(
                                        "Cleanup completed with {} error(s)",
                                        errors.len()
                                    )));
                                }
                                button_i.set_sensitive(true);
                                return glib::ControlFlow::Break;
                            }
                        }
                    }
                    glib::ControlFlow::Continue
                });
            });
        }

        Self {
            root: outer.upcast(),
            health_refresh: own_health_refresh,
        }
    }
}

fn make_scanner(
    progress: Rc<Cell<f64>>,
    spinning: Rc<Cell<bool>>,
    phase: Rc<Cell<f64>>,
    pulse_val: Rc<Cell<f64>>,
    success_flash: Rc<Cell<f64>>,
) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(240);
    area.set_content_height(240);
    area.set_size_request(240, 240);
    area.add_css_class("scanner-container");

    area.set_draw_func(move |area, cr: &cairo::Context, width, height| {
        let w = width as f64;
        let h = height as f64;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let radius = w.min(h) * 0.38;
        let p = progress.get().clamp(0.0, 1.0);
        let flash = success_flash.get();
        let is_spinning = spinning.get() && adw::is_animations_enabled(area);

        // 1. Ambient pulsing glow when active
        if is_spinning {
            let glow_radius = radius + 12.0 + 8.0 * pulse_val.get();
            let glow = cairo::RadialGradient::new(cx, cy, radius * 0.7, cx, cy, glow_radius);
            let alpha = 0.12 + 0.10 * pulse_val.get();
            glow.add_color_stop_rgba(0.0, 0.20, 0.52, 0.95, alpha);
            glow.add_color_stop_rgba(0.7, 0.45, 0.35, 0.90, alpha * 0.5);
            glow.add_color_stop_rgba(1.0, 0.45, 0.35, 0.90, 0.0);
            let _ = cr.set_source(&glow);
            cr.arc(cx, cy, glow_radius, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        } else if flash > 0.0 {
            // Success emerald burst glow
            let glow_radius = radius + 14.0;
            let glow = cairo::RadialGradient::new(cx, cy, radius * 0.6, cx, cy, glow_radius);
            let alpha = 0.25 * flash;
            glow.add_color_stop_rgba(0.0, 0.18, 0.76, 0.49, alpha);
            glow.add_color_stop_rgba(1.0, 0.18, 0.76, 0.49, 0.0);
            let _ = cr.set_source(&glow);
            cr.arc(cx, cy, glow_radius, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }

        // 2. Track circle (soft background ring)
        cr.set_line_width(12.0);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_source_rgba(0.42, 0.46, 0.54, 0.16);
        cr.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
        let _ = cr.stroke();

        // 3. Progress arc with vibrant gradient
        let start = -std::f64::consts::FRAC_PI_2;
        let end = start + std::f64::consts::TAU * p;

        if p > 0.001 {
            cr.set_line_width(13.0);
            cr.set_line_cap(cairo::LineCap::Round);

            if flash > 0.0 {
                // Success vibrant emerald green
                cr.set_source_rgba(0.18, 0.76, 0.49, 0.95);
            } else {
                // Sapphire -> Indigo -> Cyber Violet linear gradient
                let grad =
                    cairo::LinearGradient::new(cx - radius, cy - radius, cx + radius, cy + radius);
                grad.add_color_stop_rgb(0.0, 0.21, 0.52, 0.96); // Sapphire
                grad.add_color_stop_rgb(0.5, 0.38, 0.42, 0.95); // Indigo
                grad.add_color_stop_rgb(1.0, 0.56, 0.28, 0.88); // Cyber Violet
                let _ = cr.set_source(&grad);
            }

            cr.arc(cx, cy, radius, start, end);
            let _ = cr.stroke();
        }

        // 4. Orbital spinning spark / satellite highlight
        if is_spinning {
            let s = start + std::f64::consts::TAU * phase.get();
            cr.set_line_width(4.0);
            cr.set_line_cap(cairo::LineCap::Round);
            cr.set_source_rgba(0.48, 0.78, 1.0, 0.85);
            cr.arc(cx, cy, radius + 15.0, s, s + 0.75);
            let _ = cr.stroke();
        }

        // 5. Central Hub Backdrop
        cr.set_source_rgba(0.42, 0.46, 0.54, 0.06);
        cr.arc(cx, cy, radius - 24.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();

        // 6. Inner rim stroke
        cr.set_line_width(1.0);
        cr.set_source_rgba(0.42, 0.46, 0.54, 0.15);
        cr.arc(cx, cy, radius - 24.0, 0.0, std::f64::consts::TAU);
        let _ = cr.stroke();
    });

    area
}

fn risk_card(title: &str, value: &gtk::Label, description: &str, class: &str) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
    box_.add_css_class("metric-card");
    box_.add_css_class(class);
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.add_css_class("caption-heading");
    value.set_xalign(0.0);
    value.add_css_class("title-2");
    let desc = gtk::Label::new(Some(description));
    desc.set_xalign(0.0);
    desc.set_wrap(true);
    desc.add_css_class("dim-label");
    box_.append(&label);
    box_.append(value);
    box_.append(&desc);
    box_.upcast()
}

fn show_inspect_dialog(title: &str, path: &std::path::Path, size_bytes: u64) {
    let window = adw::Window::builder()
        .title(format!("Inspect: {title}"))
        .default_width(620)
        .default_height(480)
        .modal(true)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    content.set_margin_top(16);
    content.set_margin_bottom(20);
    content.set_margin_start(20);
    content.set_margin_end(20);

    let header = adw::HeaderBar::new();
    let win_title =
        adw::WindowTitle::new(&format!("Inspect: {title}"), &path.display().to_string());
    header.set_title_widget(Some(&win_title));

    // Summary Card
    let info_card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    info_card.add_css_class("hero-card");
    let icon = gtk::Image::from_icon_name("folder-symbolic");
    icon.set_pixel_size(32);
    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text_box.set_hexpand(true);
    let size_lbl = gtk::Label::new(Some(&format!("Total Size: {}", format::bytes(size_bytes))));
    size_lbl.set_xalign(0.0);
    size_lbl.add_css_class("heading");
    let path_lbl = gtk::Label::new(Some(&path.display().to_string()));
    path_lbl.set_xalign(0.0);
    path_lbl.set_wrap(true);
    path_lbl.add_css_class("dim-label");
    text_box.append(&size_lbl);
    text_box.append(&path_lbl);
    info_card.append(&icon);
    info_card.append(&text_box);

    let open_btn = gtk::Button::with_label("Open Folder");
    open_btn.add_css_class("pill");
    let p_open = path.to_path_buf();
    open_btn.connect_clicked(move |_| {
        let _ = std::process::Command::new("xdg-open").arg(&p_open).spawn();
    });
    info_card.append(&open_btn);
    content.append(&info_card);

    let list_heading = gtk::Label::new(Some("Contained Items"));
    list_heading.set_xalign(0.0);
    list_heading.add_css_class("heading");
    content.append(&list_heading);

    let list_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list_box)
        .build();

    if let Ok(entries) = std::fs::read_dir(path) {
        let mut items: Vec<(String, u64, bool)> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let meta = e.metadata().ok()?;
                let is_dir = meta.is_dir();
                let len = if is_dir { 0 } else { meta.len() };
                Some((name, len, is_dir))
            })
            .collect();
        items.sort_by_key(|(_, len, is_dir)| (!*is_dir, std::cmp::Reverse(*len)));

        if items.is_empty() {
            let empty_lbl = gtk::Label::new(Some("Directory is empty."));
            empty_lbl.add_css_class("dim-label");
            list_box.append(&empty_lbl);
        } else {
            for (name, len, is_dir) in items.into_iter().take(40) {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                row.add_css_class("finding-card");
                let row_icon = gtk::Image::from_icon_name(if is_dir {
                    "folder-symbolic"
                } else {
                    "text-x-generic-symbolic"
                });
                row_icon.set_pixel_size(18);
                let name_lbl = gtk::Label::new(Some(&name));
                name_lbl.set_xalign(0.0);
                name_lbl.set_hexpand(true);
                name_lbl.set_wrap(true);
                let size_str = if is_dir {
                    "Directory".to_string()
                } else {
                    format::bytes(len)
                };
                let size_lbl = gtk::Label::new(Some(&size_str));
                size_lbl.add_css_class("dim-label");

                row.append(&row_icon);
                row.append(&name_lbl);
                row.append(&size_lbl);
                list_box.append(&row);
            }
        }
    } else {
        let err_lbl = gtk::Label::new(Some(
            "Cannot list contents (permission denied or path not found).",
        ));
        err_lbl.add_css_class("dim-label");
        list_box.append(&err_lbl);
    }

    content.append(&scroll);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&header);
    root.append(&content);

    window.set_content(Some(&root));
    window.present();
}

fn candidate_card(
    candidate: &CleanupCandidate,
    selected_ids: Rc<RefCell<HashSet<String>>>,
    preview_button: gtk::Button,
) -> (gtk::Widget, Option<gtk::CheckButton>) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    row.add_css_class("finding-card");

    let mut check_btn = None;
    if candidate.executable_now() {
        let select = gtk::CheckButton::new();
        select.set_active(candidate.selected_by_default());
        select.set_valign(gtk::Align::Center);
        let id = candidate.id.clone();
        let selected = selected_ids.clone();
        select.connect_toggled(move |check| {
            if check.is_active() {
                selected.borrow_mut().insert(id.clone());
            } else {
                selected.borrow_mut().remove(&id);
            }
            preview_button.set_sensitive(!selected.borrow().is_empty());
        });
        row.append(&select);
        check_btn = Some(select);
    } else {
        let icon = gtk::Image::from_icon_name("changes-prevent-symbolic");
        icon.set_tooltip_text(Some("Analysis only in this release"));
        row.append(&icon);
    }

    let icon = gtk::Image::from_icon_name(match candidate.category {
        crate::model::CleanupCategory::Trash => "user-trash-symbolic",
        crate::model::CleanupCategory::AptCache => "package-x-generic-symbolic",
        crate::model::CleanupCategory::SystemJournal => "text-x-generic-symbolic",
        crate::model::CleanupCategory::SnapRevision => "package-x-generic-symbolic",
        crate::model::CleanupCategory::DeveloperCache => "applications-engineering-symbolic",
        _ => "folder-symbolic",
    });
    icon.set_pixel_size(28);
    row.append(&icon);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(&candidate.title));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    let desc = gtk::Label::new(Some(&candidate.description));
    desc.set_xalign(0.0);
    desc.set_wrap(true);
    desc.add_css_class("dim-label");
    let reason = gtk::Label::new(Some(&format!(
        "Why: {}  •  Consequence: {}",
        candidate.reason, candidate.consequence
    )));
    reason.set_xalign(0.0);
    reason.set_wrap(true);
    reason.add_css_class("caption");
    text.append(&title);
    text.append(&desc);
    text.append(&reason);
    row.append(&text);

    let right = gtk::Box::new(gtk::Orientation::Vertical, 5);
    right.set_halign(gtk::Align::End);
    let size = gtk::Label::new(Some(&format::bytes(candidate.size_bytes)));
    size.add_css_class("title-3");
    let risk = gtk::Label::new(Some(candidate.risk.label()));
    risk.add_css_class("risk-badge");
    risk.add_css_class(candidate.risk.css_class());
    right.append(&size);
    right.append(&risk);
    if candidate.requires_privilege {
        let auth = gtk::Label::new(Some("Authentication required"));
        auth.add_css_class("dim-label");
        right.append(&auth);
    } else if !candidate.executable_now() {
        let analysis = gtk::Label::new(Some("Analysis only"));
        analysis.add_css_class("dim-label");
        right.append(&analysis);
    }
    row.append(&right);

    if let Some(ref path) = candidate.path {
        let inspect_btn = gtk::Button::builder()
            .icon_name("document-properties-symbolic")
            .tooltip_text("Inspect folder contents")
            .css_classes(["flat", "circular"])
            .valign(gtk::Align::Center)
            .build();
        let path_clone = path.clone();
        let title_clone = candidate.title.clone();
        let size_b = candidate.size_bytes;
        inspect_btn.connect_clicked(move |_| {
            show_inspect_dialog(&title_clone, &path_clone, size_b);
        });
        row.append(&inspect_btn);
    }

    (row.upcast(), check_btn)
}

struct HealthIntelligencePanel {
    root: gtk::Widget,
    refresh: Rc<dyn Fn()>,
}

fn health_intelligence_panel(home: PathBuf) -> HealthIntelligencePanel {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 14);
    card.add_css_class("health-card");

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 3);
    title_box.set_hexpand(true);
    let title = gtk::Label::new(Some("System Health Intelligence"));
    title.set_xalign(0.0);
    title.add_css_class("title-2");
    let subtitle = gtk::Label::new(Some(
        "A conservative score built only from signals LinuxCare can verify. Unknown data is never treated as failure.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    title_box.append(&title);
    title_box.append(&subtitle);
    top.append(&title_box);

    let refresh_btn = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh health analysis")
        .css_classes(["flat", "circular"])
        .build();
    top.append(&refresh_btn);
    card.append(&top);

    let score_row = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    score_row.add_css_class("health-score-row");

    let score_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    score_box.set_valign(gtk::Align::Center);
    let score = gtk::Label::new(Some("—"));
    score.add_css_class("health-score");
    let score_caption = gtk::Label::new(Some("HEALTH SCORE"));
    score_caption.add_css_class("caption-heading");
    score_caption.add_css_class("dim-label");
    score_box.append(&score);
    score_box.append(&score_caption);
    score_row.append(&score_box);

    let score_info = gtk::Box::new(gtk::Orientation::Vertical, 5);
    score_info.set_hexpand(true);
    score_info.set_valign(gtk::Align::Center);
    let grade = gtk::Label::new(Some("Analyzing…"));
    grade.set_xalign(0.0);
    grade.add_css_class("title-3");
    let summary = gtk::Label::new(Some(
        "Collecting disk, service, firewall, and network signals…",
    ));
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.add_css_class("dim-label");
    let score_bar = gtk::ProgressBar::new();
    score_bar.add_css_class("health-score-bar");
    score_info.append(&grade);
    score_info.append(&summary);
    score_info.append(&score_bar);
    score_row.append(&score_info);
    card.append(&score_row);

    let domains_title = gtk::Label::new(Some("Health domains"));
    domains_title.set_xalign(0.0);
    domains_title.add_css_class("heading");
    card.append(&domains_title);
    let domains = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.append(&domains);

    let baseline = gtk::Label::new(Some("Creating first local baseline…"));
    baseline.set_xalign(0.0);
    baseline.add_css_class("dim-label");
    card.append(&baseline);

    let signal_title = gtk::Label::new(Some("Verified signals"));
    signal_title.set_xalign(0.0);
    signal_title.add_css_class("heading");
    card.append(&signal_title);
    let signals = gtk::Box::new(gtk::Orientation::Vertical, 7);
    card.append(&signals);

    let changes_title = gtk::Label::new(Some("What Changed?"));
    changes_title.set_xalign(0.0);
    changes_title.add_css_class("heading");
    card.append(&changes_title);
    let changes = gtk::Box::new(gtk::Orientation::Vertical, 7);
    card.append(&changes);

    let refresh: Rc<dyn Fn()> = {
        let home = home.clone();
        let score = score.clone();
        let grade = grade.clone();
        let summary = summary.clone();
        let score_bar = score_bar.clone();
        let baseline = baseline.clone();
        let domains = domains.clone();
        let signals = signals.clone();
        let changes = changes.clone();
        let refresh_btn = refresh_btn.clone();

        Rc::new(move || {
            refresh_btn.set_sensitive(false);
            grade.set_text("Analyzing…");
            summary.set_text("Refreshing verified health signals without making assumptions…");

            let (tx, rx) = mpsc::channel();
            let home_worker = home.clone();
            std::thread::spawn(move || {
                let report = crate::health::collect_report(&home_worker);
                let _ = tx.send(report);
            });

            let score_i = score.clone();
            let grade_i = grade.clone();
            let summary_i = summary.clone();
            let score_bar_i = score_bar.clone();
            let baseline_i = baseline.clone();
            let domains_i = domains.clone();
            let signals_i = signals.clone();
            let changes_i = changes.clone();
            let refresh_btn_i = refresh_btn.clone();

            glib::timeout_add_local(Duration::from_millis(70), move || match rx.try_recv() {
                Ok(report) => {
                    score_i.set_text(&report.score.to_string());
                    grade_i.set_text(&report.grade);
                    summary_i.set_text(&report.summary);
                    score_bar_i.set_fraction(report.score as f64 / 100.0);

                    for class in [
                        "health-excellent",
                        "health-good",
                        "health-attention",
                        "health-critical",
                    ] {
                        score_i.remove_css_class(class);
                    }
                    score_i.add_css_class(match report.score {
                        90..=100 => "health-excellent",
                        75..=89 => "health-good",
                        55..=74 => "health-attention",
                        _ => "health-critical",
                    });

                    let mut baseline_text = report.baseline_label;
                    if let Some(scan_label) = report.scan_baseline_label {
                        baseline_text.push_str(" • ");
                        baseline_text.push_str(&scan_label);
                    }
                    baseline_i.set_text(&baseline_text);

                    while let Some(child) = domains_i.first_child() {
                        domains_i.remove(&child);
                    }
                    for domain in report.domains {
                        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                        row.add_css_class("health-domain-card");
                        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
                        text.set_hexpand(true);
                        let name = gtk::Label::new(Some(&domain.name));
                        name.set_xalign(0.0);
                        name.add_css_class("heading");
                        let detail = gtk::Label::new(Some(&domain.detail));
                        detail.set_xalign(0.0);
                        detail.set_wrap(true);
                        detail.add_css_class("dim-label");
                        let bar = gtk::ProgressBar::new();
                        bar.set_fraction(domain.score.unwrap_or(0) as f64 / 100.0);
                        bar.set_tooltip_text(Some(if domain.score.is_some() {
                            "Verified domain score"
                        } else {
                            "Domain data unavailable; excluded from overall score"
                        }));
                        text.append(&name);
                        text.append(&detail);
                        text.append(&bar);
                        row.append(&text);
                        let value = gtk::Label::new(Some(
                            &domain
                                .score
                                .map(|score| score.to_string())
                                .unwrap_or_else(|| "—".to_string()),
                        ));
                        value.add_css_class("risk-badge");
                        value.add_css_class(domain.state.css_class());
                        value.set_valign(gtk::Align::Center);
                        row.append(&value);
                        domains_i.append(&row);
                    }

                    while let Some(child) = signals_i.first_child() {
                        signals_i.remove(&child);
                    }
                    for signal in report.signals {
                        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                        row.add_css_class("health-signal-row");
                        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
                        text.set_hexpand(true);
                        let name = gtk::Label::new(Some(&signal.title));
                        name.set_xalign(0.0);
                        name.add_css_class("heading");
                        let detail = gtk::Label::new(Some(&signal.detail));
                        detail.set_xalign(0.0);
                        detail.set_wrap(true);
                        detail.add_css_class("dim-label");
                        text.append(&name);
                        text.append(&detail);
                        row.append(&text);
                        let badge = gtk::Label::new(Some(signal.state.label()));
                        badge.add_css_class("risk-badge");
                        badge.add_css_class(signal.state.css_class());
                        badge.set_valign(gtk::Align::Center);
                        row.append(&badge);
                        signals_i.append(&row);
                    }

                    while let Some(child) = changes_i.first_child() {
                        changes_i.remove(&child);
                    }
                    if report.changes.is_empty() {
                        let empty = gtk::Label::new(Some(
                                "A baseline has been captured. Meaningful changes will appear here after future launches or Smart Scans.",
                            ));
                        empty.set_xalign(0.0);
                        empty.set_wrap(true);
                        empty.add_css_class("dim-label");
                        changes_i.append(&empty);
                    } else {
                        for change in report.changes {
                            let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
                            row.add_css_class("change-row");
                            row.add_css_class(change.tone.css_class());
                            let name = gtk::Label::new(Some(&change.title));
                            name.set_xalign(0.0);
                            name.add_css_class("heading");
                            let detail = gtk::Label::new(Some(&change.detail));
                            detail.set_xalign(0.0);
                            detail.set_wrap(true);
                            detail.add_css_class("dim-label");
                            row.append(&name);
                            row.append(&detail);
                            changes_i.append(&row);
                        }
                    }

                    refresh_btn_i.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    grade_i.set_text("Health analysis unavailable");
                    summary_i.set_text("The health worker stopped before returning a report.");
                    refresh_btn_i.set_sensitive(true);
                    glib::ControlFlow::Break
                }
            });
        })
    };

    {
        let refresh_c = refresh.clone();
        refresh_btn.connect_clicked(move |_| refresh_c());
    }
    refresh();

    HealthIntelligencePanel {
        root: card.upcast(),
        refresh,
    }
}

fn system_overview_card() -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("finding-card");

    let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let os_icon = gtk::Image::from_icon_name("computer-symbolic");
    os_icon.set_pixel_size(24);
    top_row.append(&os_icon);

    let os_label = gtk::Label::new(None);
    os_label.set_xalign(0.0);
    os_label.set_hexpand(true);
    os_label.add_css_class("heading");
    top_row.append(&os_label);

    let uptime_label = gtk::Label::new(None);
    uptime_label.add_css_class("dim-label");
    top_row.append(&uptime_label);
    card.append(&top_row);

    let meters_grid = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    meters_grid.set_homogeneous(true);

    // RAM meter
    let ram_box = gtk::Box::new(gtk::Orientation::Vertical, 5);
    let ram_title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let ram_title = gtk::Label::new(Some("MEMORY (RAM)"));
    ram_title.set_xalign(0.0);
    ram_title.add_css_class("caption-heading");
    let ram_val = gtk::Label::new(None);
    ram_val.set_xalign(1.0);
    ram_val.set_hexpand(true);
    ram_val.add_css_class("dim-label");
    ram_title_row.append(&ram_title);
    ram_title_row.append(&ram_val);
    let ram_bar = gtk::ProgressBar::new();
    ram_box.append(&ram_title_row);
    ram_box.append(&ram_bar);
    meters_grid.append(&ram_box);

    // Root disk meter
    let root_box = gtk::Box::new(gtk::Orientation::Vertical, 5);
    let root_title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let root_title = gtk::Label::new(Some("SYSTEM DISK (/)"));
    root_title.set_xalign(0.0);
    root_title.add_css_class("caption-heading");
    let root_val = gtk::Label::new(None);
    root_val.set_xalign(1.0);
    root_val.set_hexpand(true);
    root_val.add_css_class("dim-label");
    root_title_row.append(&root_title);
    root_title_row.append(&root_val);
    let root_bar = gtk::ProgressBar::new();
    root_box.append(&root_title_row);
    root_box.append(&root_bar);
    meters_grid.append(&root_box);

    // Home disk meter
    let home_box = gtk::Box::new(gtk::Orientation::Vertical, 5);
    let home_title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let home_title = gtk::Label::new(Some("HOME STORAGE"));
    home_title.set_xalign(0.0);
    home_title.add_css_class("caption-heading");
    let home_val = gtk::Label::new(None);
    home_val.set_xalign(1.0);
    home_val.set_hexpand(true);
    home_val.add_css_class("dim-label");
    home_title_row.append(&home_title);
    home_title_row.append(&home_val);
    let home_bar = gtk::ProgressBar::new();
    home_box.append(&home_title_row);
    home_box.append(&home_bar);
    meters_grid.append(&home_box);

    card.append(&meters_grid);

    let refresh_stats = {
        let os_label = os_label.clone();
        let uptime_label = uptime_label.clone();
        let ram_val = ram_val.clone();
        let ram_bar = ram_bar.clone();
        let root_val = root_val.clone();
        let root_bar = root_bar.clone();
        let home_val = home_val.clone();
        let home_bar = home_bar.clone();

        move || {
            let stats = crate::system_info::collect_system_stats();
            os_label.set_text(&format!(
                "{} • Kernel {}",
                stats.os_name, stats.kernel_version
            ));
            uptime_label.set_text(&format!("Uptime: {}", stats.uptime_str));

            ram_val.set_text(&format!(
                "{} / {} ({:.0}%)",
                format::bytes(stats.ram_used_bytes),
                format::bytes(stats.ram_total_bytes),
                stats.ram_percent * 100.0
            ));
            ram_bar.set_fraction(stats.ram_percent as f64);

            root_val.set_text(&format!(
                "{} / {} ({:.0}%)",
                format::bytes(stats.disk_root_used_bytes),
                format::bytes(stats.disk_root_total_bytes),
                stats.disk_root_percent * 100.0
            ));
            root_bar.set_fraction(stats.disk_root_percent as f64);

            home_val.set_text(&format!(
                "{} / {} ({:.0}%)",
                format::bytes(stats.disk_home_used_bytes),
                format::bytes(stats.disk_home_total_bytes),
                stats.disk_home_percent * 100.0
            ));
            home_bar.set_fraction(stats.disk_home_percent as f64);
        }
    };

    refresh_stats();

    let refresh_timer = refresh_stats.clone();
    glib::timeout_add_local(Duration::from_secs(3), move || {
        refresh_timer();
        glib::ControlFlow::Continue
    });

    card.upcast()
}

fn warning_card(message: &str) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("warning-card");
    row.append(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
    let l = gtk::Label::new(Some(message));
    l.set_wrap(true);
    l.set_xalign(0.0);
    row.append(&l);
    row.upcast()
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("../assets/style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn show_about_window(parent: Option<&impl IsA<gtk::Window>>) {
    let about = gtk::AboutDialog::builder()
        .program_name("LinuxCare")
        .logo_icon_name("net.milmit.LinuxCare")
        .version(env!("CARGO_PKG_VERSION"))
        .website("https://milmit.net")
        .website_label("milmit.net (Official Website)")
        .copyright("© 2026 MilMit. All rights reserved.")
        .license_type(gtk::License::Unknown)
        .comments("Professional Safety-First System Care, Hardware Diagnostics & Total Cleaner Suite for Linux.\n\nDeveloped & Engineered by MilMit.\nWebsite: https://milmit.net\nTelegram: https://t.me/milmit\nInstagram: https://instagram.com/milmit")
        .authors(vec!["MilMit <info@milmit.net>"])
        .artists(vec!["MilMit"])
        .modal(true)
        .build();

    if let Some(p) = parent {
        about.set_transient_for(Some(p.as_ref()));
    }

    about.present();
}
