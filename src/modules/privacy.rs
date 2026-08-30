use crate::format;
use gtk::prelude::*;
use std::{fs, path::PathBuf, sync::mpsc, time::Duration};

struct PrivacyMetrics {
    recent_size: u64,
    recent_exists: bool,
    thumb_size: u64,
    thumb_exists: bool,
}

pub fn privacy_page(home: PathBuf) -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Privacy & Diagnostic Traces"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");
    let refresh_btn = gtk::Button::with_label("Refresh Privacy Traces");
    refresh_btn.add_css_class("pill");
    title_row.append(&title);
    title_row.append(&refresh_btn);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Review and clear recent document history, crash report dumps, and thumbnail caches.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&container);

    let home_c = home.clone();
    let container_c = container.clone();
    let refresh_btn_c = refresh_btn.clone();

    let render_page = move || {
        refresh_btn_c.set_sensitive(false);
        while let Some(child) = container_c.first_child() {
            container_c.remove(&child);
        }
        let loading = gtk::Label::new(Some("Calculating privacy trace sizes…"));
        loading.add_css_class("dim-label");
        container_c.append(&loading);

        let (tx, rx) = mpsc::channel();
        let home_thread = home_c.clone();

        std::thread::spawn(move || {
            let recent_path = home_thread.join(".local/share/recently-used.xbel");
            let recent_exists = recent_path.exists();
            let recent_size = if recent_exists {
                fs::metadata(&recent_path).map(|m| m.len()).unwrap_or(0)
            } else {
                0
            };

            let thumb_path = home_thread.join(".cache/thumbnails");
            let thumb_exists = thumb_path.exists();
            let thumb_size = if thumb_exists {
                walkdir::WalkDir::new(&thumb_path)
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.metadata().ok())
                    .filter(|m| m.is_file())
                    .map(|m| m.len())
                    .sum::<u64>()
            } else {
                0
            };

            let _ = tx.send(PrivacyMetrics {
                recent_size,
                recent_exists,
                thumb_size,
                thumb_exists,
            });
        });

        let container_ui = container_c.clone();
        let refresh_btn_ui = refresh_btn_c.clone();
        let home_ui = home_c.clone();

        glib::timeout_add_local(Duration::from_millis(50), move || {
            if let Ok(metrics) = rx.try_recv() {
                while let Some(child) = container_ui.first_child() {
                    container_ui.remove(&child);
                }
                refresh_btn_ui.set_sensitive(true);

                // Recent Files Card
                let recent_path = home_ui.join(".local/share/recently-used.xbel");
                let r_card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
                r_card.add_css_class("finding-card");
                let r_img = gtk::Image::from_icon_name("document-open-recent-symbolic");
                r_img.set_pixel_size(24);
                r_card.append(&r_img);

                let r_info = gtk::Box::new(gtk::Orientation::Vertical, 4);
                r_info.set_hexpand(true);
                let r_title = gtk::Label::new(Some("Recent Files History"));
                r_title.set_xalign(0.0);
                r_title.add_css_class("heading");
                let r_sub = gtk::Label::new(Some(&format!(
                    "recently-used.xbel • {}",
                    format::bytes(metrics.recent_size)
                )));
                r_sub.set_xalign(0.0);
                r_sub.add_css_class("dim-label");
                r_info.append(&r_title);
                r_info.append(&r_sub);
                r_card.append(&r_info);

                if metrics.recent_exists && metrics.recent_size > 0 {
                    let r_btn = gtk::Button::with_label("Clear Recent History");
                    r_btn.add_css_class("pill");
                    let path_c = recent_path.clone();
                    let r_sub_c = r_sub.clone();
                    r_btn.connect_clicked(move |btn| {
                        btn.set_sensitive(false);
                        btn.set_label("Clearing…");
                        let path_to_rem = path_c.clone();
                        let (tx_del, rx_del) = mpsc::channel();
                        std::thread::spawn(move || {
                            let _ = fs::remove_file(&path_to_rem);
                            let _ = tx_del.send(());
                        });
                        let btn_c = btn.clone();
                        let r_sub_cc = r_sub_c.clone();
                        glib::timeout_add_local(Duration::from_millis(50), move || {
                            if rx_del.try_recv().is_ok() {
                                btn_c.set_label("Cleared");
                                r_sub_cc.set_text("recently-used.xbel • 0 B");
                                glib::ControlFlow::Break
                            } else {
                                glib::ControlFlow::Continue
                            }
                        });
                    });
                    r_card.append(&r_btn);
                } else {
                    let r_lbl = gtk::Label::new(Some("Clean"));
                    r_lbl.add_css_class("dim-label");
                    r_card.append(&r_lbl);
                }
                container_ui.append(&r_card);

                // Thumbnail Cache Card
                let thumb_path = home_ui.join(".cache/thumbnails");
                let t_card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
                t_card.add_css_class("finding-card");
                let t_img = gtk::Image::from_icon_name("image-x-generic-symbolic");
                t_img.set_pixel_size(24);
                t_card.append(&t_img);

                let t_info = gtk::Box::new(gtk::Orientation::Vertical, 4);
                t_info.set_hexpand(true);
                let t_title = gtk::Label::new(Some("Desktop Thumbnail Cache"));
                t_title.set_xalign(0.0);
                t_title.add_css_class("heading");
                let t_sub = gtk::Label::new(Some(&format!(
                    "~/.cache/thumbnails • {}",
                    format::bytes(metrics.thumb_size)
                )));
                t_sub.set_xalign(0.0);
                t_sub.add_css_class("dim-label");
                t_info.append(&t_title);
                t_info.append(&t_sub);
                t_card.append(&t_info);

                if metrics.thumb_exists && metrics.thumb_size > 0 {
                    let t_btn = gtk::Button::with_label("Clear Thumbnails");
                    t_btn.add_css_class("pill");
                    let path_c = thumb_path.clone();
                    let t_sub_c = t_sub.clone();
                    t_btn.connect_clicked(move |btn| {
                        btn.set_sensitive(false);
                        btn.set_label("Clearing…");
                        let path_to_rem = path_c.clone();
                        let (tx_del, rx_del) = mpsc::channel();
                        std::thread::spawn(move || {
                            let _ = fs::remove_dir_all(&path_to_rem);
                            let _ = fs::create_dir_all(&path_to_rem);
                            let _ = tx_del.send(());
                        });
                        let btn_c = btn.clone();
                        let t_sub_cc = t_sub_c.clone();
                        glib::timeout_add_local(Duration::from_millis(50), move || {
                            if rx_del.try_recv().is_ok() {
                                btn_c.set_label("Cleared");
                                t_sub_cc.set_text("~/.cache/thumbnails • 0 B");
                                glib::ControlFlow::Break
                            } else {
                                glib::ControlFlow::Continue
                            }
                        });
                    });
                    t_card.append(&t_btn);
                } else {
                    let t_lbl = gtk::Label::new(Some("Clean"));
                    t_lbl.add_css_class("dim-label");
                    t_card.append(&t_lbl);
                }
                container_ui.append(&t_card);

                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    };

    let render_c = render_page.clone();
    refresh_btn.connect_clicked(move |_| render_c());
    render_page();

    let clamp = adw::Clamp::builder()
        .maximum_size(920)
        .tightening_threshold(700)
        .child(&page)
        .build();

    outer.set_child(Some(&clamp));
    outer.upcast()
}
