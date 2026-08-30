use crate::format;
use gtk::prelude::*;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

struct AppInfo {
    name: &'static str,
    rel_path: &'static str,
    size: u64,
    exists: bool,
}

#[derive(Debug, Clone)]
struct LeftoverInfo {
    app_id: String,
    full_path: PathBuf,
    size: u64,
}

pub fn apps_page(home: PathBuf) -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Applications & Leftover Data"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let scan_btn = gtk::Button::with_label("Scan All Apps");
    scan_btn.add_css_class("pill");
    title_row.append(&title);
    title_row.append(&scan_btn);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Inspect application cache footprints and permanently remove leftover data from uninstalled apps.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    // Section 1: Active App Caches
    let sec1_title = gtk::Label::new(Some("Active Application Caches"));
    sec1_title.set_xalign(0.0);
    sec1_title.add_css_class("heading");
    sec1_title.set_margin_top(10);
    page.append(&sec1_title);

    let container_caches = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&container_caches);

    // Section 2: Uninstalled App Leftovers
    let sec2_title = gtk::Label::new(Some("🗑️ Uninstalled Application Leftovers"));
    sec2_title.set_xalign(0.0);
    sec2_title.add_css_class("heading");
    sec2_title.set_margin_top(16);
    page.append(&sec2_title);

    let sec2_sub = gtk::Label::new(Some(
        "Orphaned configuration and cache folders remaining in ~/.var/app or ~/.config after uninstalling Flatpaks or system packages.",
    ));
    sec2_sub.set_xalign(0.0);
    sec2_sub.set_wrap(true);
    sec2_sub.add_css_class("dim-label");
    page.append(&sec2_sub);

    let container_leftovers = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&container_leftovers);

    let home_c = home.clone();
    let container_caches_c = container_caches.clone();
    let container_leftovers_c = container_leftovers.clone();
    let scan_btn_c = scan_btn.clone();

    let trigger_scan = move || {
        scan_btn_c.set_sensitive(false);
        while let Some(child) = container_caches_c.first_child() {
            container_caches_c.remove(&child);
        }
        while let Some(child) = container_leftovers_c.first_child() {
            container_leftovers_c.remove(&child);
        }

        let loading1 = gtk::Label::new(Some("Scanning active application caches…"));
        loading1.add_css_class("dim-label");
        container_caches_c.append(&loading1);

        let loading2 = gtk::Label::new(Some("Detecting orphaned leftover directories…"));
        loading2.add_css_class("dim-label");
        container_leftovers_c.append(&loading2);

        let (tx, rx) = mpsc::channel();
        let home_thread = home_c.clone();

        std::thread::spawn(move || {
            let apps_list = [
                ("Firefox Web Browser", ".cache/mozilla/firefox"),
                ("Google Chrome", ".cache/google-chrome"),
                ("Brave Software", ".cache/BraveSoftware"),
                ("Chromium", ".cache/chromium"),
                ("Spotify Music", ".cache/spotify"),
                ("Telegram Desktop", ".cache/TelegramDesktop"),
                ("Discord", ".cache/discord"),
                ("Visual Studio Code", ".cache/Code"),
                ("GNOME Software", ".cache/gnome-software"),
                ("Rust Cargo Cache", ".cargo/registry/cache"),
            ];

            let mut app_results = Vec::new();
            for (name, rel_path) in apps_list {
                let path = home_thread.join(rel_path);
                let exists = path.exists();
                let size = if exists { dir_size(&path) } else { 0 };
                app_results.push(AppInfo {
                    name,
                    rel_path,
                    size,
                    exists,
                });
            }

            // Scan Leftovers in ~/.var/app/
            let installed_flatpaks = get_installed_flatpaks();
            let mut leftovers = Vec::new();
            let var_app = home_thread.join(".var/app");
            if let Ok(entries) = fs::read_dir(var_app) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        let id = entry.file_name().to_string_lossy().to_string();
                        if !installed_flatpaks.contains(&id) {
                            let size = dir_size(&p);
                            if size > 0 {
                                leftovers.push(LeftoverInfo {
                                    app_id: id,
                                    full_path: p,
                                    size,
                                });
                            }
                        }
                    }
                }
            }

            let _ = tx.send((app_results, leftovers));
        });

        let caches_ui = container_caches_c.clone();
        let leftovers_ui = container_leftovers_c.clone();
        let scan_btn_ui = scan_btn_c.clone();
        let home_ui = home_c.clone();

        glib::timeout_add_local(Duration::from_millis(50), move || {
            if let Ok((apps, leftovers)) = rx.try_recv() {
                while let Some(child) = caches_ui.first_child() {
                    caches_ui.remove(&child);
                }
                while let Some(child) = leftovers_ui.first_child() {
                    leftovers_ui.remove(&child);
                }
                scan_btn_ui.set_sensitive(true);

                // Populate Active Caches
                for app in apps {
                    let card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
                    card.add_css_class("finding-card");

                    let img = gtk::Image::from_icon_name("application-x-executable-symbolic");
                    img.set_pixel_size(24);
                    card.append(&img);

                    let info = gtk::Box::new(gtk::Orientation::Vertical, 4);
                    info.set_hexpand(true);

                    let app_title = gtk::Label::new(Some(app.name));
                    app_title.set_xalign(0.0);
                    app_title.add_css_class("heading");

                    let path_label = gtk::Label::new(Some(&format!(
                        "{} • {}",
                        app.rel_path,
                        if app.exists {
                            format::bytes(app.size)
                        } else {
                            "Not installed / No cache".into()
                        }
                    )));
                    path_label.set_xalign(0.0);
                    path_label.add_css_class("dim-label");

                    info.append(&app_title);
                    info.append(&path_label);
                    card.append(&info);

                    let full_path = home_ui.join(app.rel_path);
                    if app.exists && app.size > 0 {
                        let clean_btn = gtk::Button::with_label("Clear Cache");
                        clean_btn.add_css_class("pill");
                        let path_to_clean = full_path.clone();
                        let path_label_c = path_label.clone();
                        let rel_path_str = app.rel_path.to_string();

                        clean_btn.connect_clicked(move |btn| {
                            btn.set_sensitive(false);
                            btn.set_label("Clearing…");
                            let (clean_tx, clean_rx) = mpsc::channel();
                            let p = path_to_clean.clone();

                            std::thread::spawn(move || {
                                let _ = std::fs::remove_dir_all(&p);
                                let _ = std::fs::create_dir_all(&p);
                                let _ = clean_tx.send(());
                            });

                            let btn_c = btn.clone();
                            let path_label_cc = path_label_c.clone();
                            let rel_path_cc = rel_path_str.clone();

                            glib::timeout_add_local(Duration::from_millis(50), move || {
                                if clean_rx.try_recv().is_ok() {
                                    btn_c.set_label("Cleared");
                                    path_label_cc.set_text(&format!("{rel_path_cc} • 0 B"));
                                    glib::ControlFlow::Break
                                } else {
                                    glib::ControlFlow::Continue
                                }
                            });
                        });
                        card.append(&clean_btn);
                    } else {
                        let status_label = gtk::Label::new(Some("Clean"));
                        status_label.add_css_class("dim-label");
                        card.append(&status_label);
                    }

                    caches_ui.append(&card);
                }

                // Populate Leftovers
                if leftovers.is_empty() {
                    let clean_card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                    clean_card.add_css_class("finding-card");
                    let check_img = gtk::Image::from_icon_name("emblem-ok-symbolic");
                    check_img.set_pixel_size(20);
                    let clean_lbl = gtk::Label::new(Some(
                        "✨ No orphaned leftovers found. Your system is perfectly tidy!",
                    ));
                    clean_lbl.add_css_class("dim-label");
                    clean_card.append(&check_img);
                    clean_card.append(&clean_lbl);
                    leftovers_ui.append(&clean_card);
                } else {
                    for lo in leftovers {
                        let card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
                        card.add_css_class("finding-card");

                        let img = gtk::Image::from_icon_name("user-trash-symbolic");
                        img.set_pixel_size(24);
                        card.append(&img);

                        let info = gtk::Box::new(gtk::Orientation::Vertical, 4);
                        info.set_hexpand(true);

                        let app_title = gtk::Label::new(Some(&lo.app_id));
                        app_title.set_xalign(0.0);
                        app_title.add_css_class("heading");

                        let path_label = gtk::Label::new(Some(&format!(
                            "{} • Leftover size: {}",
                            lo.full_path.display(),
                            format::bytes(lo.size)
                        )));
                        path_label.set_xalign(0.0);
                        path_label.add_css_class("dim-label");

                        info.append(&app_title);
                        info.append(&path_label);
                        card.append(&info);

                        let del_btn = gtk::Button::with_label("Delete Leftover");
                        del_btn.add_css_class("pill");
                        del_btn.add_css_class("destructive-action");
                        del_btn.set_valign(gtk::Align::Center);

                        let p_del = lo.full_path.clone();
                        let card_c = card.clone();

                        del_btn.connect_clicked(move |btn| {
                            btn.set_sensitive(false);
                            btn.set_label("Deleting…");
                            let (del_tx, del_rx) = mpsc::channel();
                            let p = p_del.clone();

                            std::thread::spawn(move || {
                                let _ = std::fs::remove_dir_all(&p);
                                let _ = del_tx.send(());
                            });

                            let card_ui = card_c.clone();
                            glib::timeout_add_local(Duration::from_millis(50), move || {
                                if del_rx.try_recv().is_ok() {
                                    card_ui.set_visible(false);
                                    glib::ControlFlow::Break
                                } else {
                                    glib::ControlFlow::Continue
                                }
                            });
                        });

                        card.append(&del_btn);
                        leftovers_ui.append(&card);
                    }
                }

                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    };

    let trigger_scan_c = trigger_scan.clone();
    scan_btn.connect_clicked(move |_| trigger_scan_c());
    trigger_scan();

    let clamp = adw::Clamp::builder()
        .maximum_size(920)
        .tightening_threshold(700)
        .child(&page)
        .build();

    outer.set_child(Some(&clamp));
    outer.upcast()
}

fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum::<u64>()
}

fn get_installed_flatpaks() -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(output) = std::process::Command::new("flatpak")
        .args(["list", "--app", "--columns=application"])
        .output()
    {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                set.insert(trimmed.to_string());
            }
        }
    }
    set
}
