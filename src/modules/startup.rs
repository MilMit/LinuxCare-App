use gtk::prelude::*;
use std::{fs, path::PathBuf};

pub fn startup_page(home: PathBuf) -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Startup Applications"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    title_row.append(&title);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some("Manage applications configured to launch automatically when you log in to your desktop session."));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&container);

    let autostart_dir = home.join(".config/autostart");
    if autostart_dir.exists() {
        if let Ok(entries) = fs::read_dir(&autostart_dir) {
            let mut count = 0;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "desktop") {
                    count += 1;
                    let filename = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let content = fs::read_to_string(&path).unwrap_or_default();
                    let name_str = content
                        .lines()
                        .find(|l| l.starts_with("Name="))
                        .map(|l| l.trim_start_matches("Name="))
                        .unwrap_or(&filename)
                        .to_string();

                    let card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
                    card.add_css_class("finding-card");
                    let img = gtk::Image::from_icon_name("system-run-symbolic");
                    img.set_pixel_size(24);
                    card.append(&img);

                    let info = gtk::Box::new(gtk::Orientation::Vertical, 4);
                    info.set_hexpand(true);
                    let item_title = gtk::Label::new(Some(&name_str));
                    item_title.set_xalign(0.0);
                    item_title.add_css_class("heading");
                    let item_sub =
                        gtk::Label::new(Some(&format!("~/.config/autostart/{filename}")));
                    item_sub.set_xalign(0.0);
                    item_sub.add_css_class("dim-label");
                    info.append(&item_title);
                    info.append(&item_sub);
                    card.append(&info);

                    let switch = gtk::Switch::new();
                    switch.set_active(true);
                    switch.set_valign(gtk::Align::Center);
                    let path_c = path.clone();
                    switch.connect_state_set(move |_, state| {
                        if !state {
                            let disabled_path = path_c.with_extension("desktop.disabled");
                            let _ = fs::rename(&path_c, &disabled_path);
                        } else {
                            let enabled_path = path_c.with_extension("desktop");
                            let _ = fs::rename(&path_c, &enabled_path);
                        }
                        glib::Propagation::Proceed
                    });
                    card.append(&switch);
                    container.append(&card);
                }
            }

            if count == 0 {
                let empty = adw::StatusPage::builder()
                    .title("No autostart applications")
                    .description("Your user session has no custom startup .desktop launchers in ~/.config/autostart.")
                    .icon_name("media-playback-start-symbolic")
                    .build();
                container.append(&empty);
            }
        }
    } else {
        let empty = adw::StatusPage::builder()
            .title("No autostart directory")
            .description("No autostart entries found at ~/.config/autostart.")
            .icon_name("media-playback-start-symbolic")
            .build();
        container.append(&empty);
    }

    let clamp = adw::Clamp::builder()
        .maximum_size(920)
        .tightening_threshold(700)
        .child(&page)
        .build();

    outer.set_child(Some(&clamp));
    outer.upcast()
}
