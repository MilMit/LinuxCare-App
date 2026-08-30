use adw::prelude::*;
use gtk::{cairo, glib};
use std::{
    cell::{Cell, RefCell},
    ffi::CString,
    fs,
    mem::MaybeUninit,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Default)]
pub struct CpuStats {
    pub usage_pct: f32,
    pub temp_c: Option<f32>,
    pub model_name: String,
    pub core_count: usize,
    pub freq_mhz: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    pub ram_used: u64,
    pub ram_total: u64,
    pub ram_pct: f32,
    pub swap_used: u64,
    pub swap_total: u64,
    pub swap_pct: f32,
}

#[derive(Debug, Clone, Default)]
pub struct BatteryStats {
    pub present: bool,
    pub percentage: u8,
    pub status: String,
    pub power_watts: Option<f32>,
    pub health_pct: Option<u8>,
    pub time_remaining_str: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    pub download_rate: u64,
    pub upload_rate: u64,
    pub active_iface: String,
    pub total_rx: u64,
    pub total_tx: u64,
}

#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    pub root_used: u64,
    pub root_total: u64,
    pub root_pct: f32,
    pub home_used: u64,
    pub home_total: u64,
    pub home_pct: f32,
}

#[derive(Debug, Clone)]
pub struct ProcessItem {
    pub pid: u32,
    pub name: String,
    pub ram_bytes: u64,
}

pub fn vitals_page(toast_overlay: Rc<adw::ToastOverlay>) -> gtk::Widget {
    let outer = gtk::ScrolledWindow::new();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_top(28);
    page.set_margin_bottom(28);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("System Vitals"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-1");

    let live_badge = gtk::Label::new(Some("● LIVE"));
    live_badge.add_css_class("status-badge-active");
    live_badge.add_css_class("caption-heading");
    live_badge.set_valign(gtk::Align::Center);

    title_row.append(&title);
    title_row.append(&live_badge);
    page.append(&title_row);

    let subtitle = gtk::Label::new(Some(
        "Real-time diagnostic metrics for CPU, thermals, battery power, memory, and task optimization.",
    ));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    // Grid of cards
    let grid = gtk::Box::new(gtk::Orientation::Vertical, 14);
    page.append(&grid);

    let cpu_history = Rc::new(RefCell::new(vec![0.0; 30]));
    let ram_history = Rc::new(RefCell::new(vec![0.0; 30]));
    let net_history = Rc::new(RefCell::new(vec![0.0; 30]));

    // 1. CPU & Thermals Card
    let cpu_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    cpu_card.add_css_class("finding-card");

    let cpu_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let cpu_icon = gtk::Image::from_icon_name("processor-symbolic");
    cpu_icon.set_pixel_size(24);
    let cpu_title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    cpu_title_box.set_hexpand(true);
    let cpu_title = gtk::Label::new(Some("Processor & Thermals"));
    cpu_title.set_xalign(0.0);
    cpu_title.add_css_class("heading");
    let cpu_model_lbl = gtk::Label::new(Some("Detecting CPU…"));
    cpu_model_lbl.set_xalign(0.0);
    cpu_model_lbl.add_css_class("dim-label");
    cpu_title_box.append(&cpu_title);
    cpu_title_box.append(&cpu_model_lbl);

    let cpu_temp_lbl = gtk::Label::new(Some("— °C"));
    cpu_temp_lbl.add_css_class("risk-badge");
    cpu_temp_lbl.add_css_class("risk-safe");
    cpu_temp_lbl.set_valign(gtk::Align::Center);

    cpu_header.append(&cpu_icon);
    cpu_header.append(&cpu_title_box);
    cpu_header.append(&cpu_temp_lbl);
    cpu_card.append(&cpu_header);

    let cpu_bar_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let cpu_bar = gtk::ProgressBar::new();
    cpu_bar.set_hexpand(true);
    cpu_bar.set_valign(gtk::Align::Center);
    let cpu_pct_lbl = gtk::Label::new(Some("0%"));
    cpu_pct_lbl.add_css_class("heading");
    cpu_bar_row.append(&cpu_bar);
    cpu_bar_row.append(&cpu_pct_lbl);
    cpu_card.append(&cpu_bar_row);

    let cpu_wave = make_sparkline_wave(cpu_history.clone(), (0.20, 0.60, 1.0), (0.65, 0.35, 1.0));
    cpu_card.append(&cpu_wave);
    grid.append(&cpu_card);

    // 2. Power Profiles & Battery Saver Card
    let power_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    power_card.add_css_class("finding-card");

    let power_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let power_icon = gtk::Image::from_icon_name("power-profile-balanced-symbolic");
    power_icon.set_pixel_size(24);
    let power_title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    power_title_box.set_hexpand(true);
    let power_title = gtk::Label::new(Some("Power & Performance Profile"));
    power_title.set_xalign(0.0);
    power_title.add_css_class("heading");
    let power_sub = gtk::Label::new(Some(
        "Select system CPU frequency and power management policy",
    ));
    power_sub.set_xalign(0.0);
    power_sub.add_css_class("dim-label");
    power_title_box.append(&power_title);
    power_title_box.append(&power_sub);
    power_header.append(&power_icon);
    power_header.append(&power_title_box);
    power_card.append(&power_header);

    let profiles_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let btn_perf = gtk::Button::with_label("Performance");
    btn_perf.add_css_class("pill");
    let btn_bal = gtk::Button::with_label("Balanced");
    btn_bal.add_css_class("pill");
    let btn_save = gtk::Button::with_label("Power Saver");
    btn_save.add_css_class("pill");

    let active_p = get_active_power_profile();
    if active_p == "performance" {
        btn_perf.add_css_class("suggested-action");
    } else if active_p == "power-saver" {
        btn_save.add_css_class("suggested-action");
    } else {
        btn_bal.add_css_class("suggested-action");
    }

    let p_toast = toast_overlay.clone();
    let b_p = btn_perf.clone();
    let b_b = btn_bal.clone();
    let b_s = btn_save.clone();

    let update_buttons = move |sel: &str| {
        b_p.remove_css_class("suggested-action");
        b_b.remove_css_class("suggested-action");
        b_s.remove_css_class("suggested-action");
        if sel == "performance" {
            b_p.add_css_class("suggested-action");
        } else if sel == "power-saver" {
            b_s.add_css_class("suggested-action");
        } else {
            b_b.add_css_class("suggested-action");
        }
    };

    {
        let ub = update_buttons.clone();
        let t = p_toast.clone();
        btn_perf.connect_clicked(move |_| {
            set_power_profile("performance");
            ub("performance");
            t.add_toast(adw::Toast::new("Switched to Performance Mode"));
        });
    }
    {
        let ub = update_buttons.clone();
        let t = p_toast.clone();
        btn_bal.connect_clicked(move |_| {
            set_power_profile("balanced");
            ub("balanced");
            t.add_toast(adw::Toast::new("Switched to Balanced Mode"));
        });
    }
    {
        let ub = update_buttons.clone();
        let t = p_toast.clone();
        btn_save.connect_clicked(move |_| {
            set_power_profile("power-saver");
            ub("power-saver");
            t.add_toast(adw::Toast::new("Switched to Power Saver Mode"));
        });
    }

    profiles_row.append(&btn_perf);
    profiles_row.append(&btn_bal);
    profiles_row.append(&btn_save);
    power_card.append(&profiles_row);
    grid.append(&power_card);

    // 3. Memory & Swap Card + RAM Flush
    let mem_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    mem_card.add_css_class("finding-card");

    let mem_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let mem_icon = gtk::Image::from_icon_name("drive-harddisk-symbolic");
    mem_icon.set_pixel_size(24);
    let mem_title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    mem_title_box.set_hexpand(true);
    let mem_title = gtk::Label::new(Some("Memory & Swap"));
    mem_title.set_xalign(0.0);
    mem_title.add_css_class("heading");
    let mem_detail_lbl = gtk::Label::new(Some("RAM: — / — • Swap: —"));
    mem_detail_lbl.set_xalign(0.0);
    mem_detail_lbl.add_css_class("dim-label");
    mem_title_box.append(&mem_title);
    mem_title_box.append(&mem_detail_lbl);

    let mem_pct_badge = gtk::Label::new(Some("0% RAM"));
    mem_pct_badge.add_css_class("risk-badge");
    mem_pct_badge.add_css_class("risk-safe");
    mem_pct_badge.set_valign(gtk::Align::Center);

    mem_header.append(&mem_icon);
    mem_header.append(&mem_title_box);
    mem_header.append(&mem_pct_badge);
    mem_card.append(&mem_header);

    let mem_bars_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let ram_bar = gtk::ProgressBar::new();
    let swap_bar = gtk::ProgressBar::new();
    mem_bars_box.append(&ram_bar);
    mem_bars_box.append(&swap_bar);
    mem_card.append(&mem_bars_box);

    let ram_wave = make_sparkline_wave(ram_history.clone(), (0.10, 0.80, 0.60), (0.20, 0.85, 0.45));
    mem_card.append(&ram_wave);
    grid.append(&mem_card);

    let mem_note = gtk::Label::new(Some(
        "Linux automatically reclaims filesystem cache when applications need memory; LinuxCare does not force-drop kernel caches.",
    ));
    mem_note.set_xalign(0.0);
    mem_note.set_wrap(true);
    mem_note.add_css_class("dim-label");
    mem_card.append(&mem_note);

    // 4. Top Active User Processes Card
    let proc_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    proc_card.add_css_class("finding-card");

    let proc_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let proc_icon = gtk::Image::from_icon_name("utilities-system-monitor-symbolic");
    proc_icon.set_pixel_size(24);
    let proc_title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    proc_title_box.set_hexpand(true);
    let proc_title = gtk::Label::new(Some("Top Active Applications & Processes"));
    proc_title.set_xalign(0.0);
    proc_title.add_css_class("heading");
    let proc_sub = gtk::Label::new(Some(
        "Inspect heavy applications and terminate unneeded processes",
    ));
    proc_sub.set_xalign(0.0);
    proc_sub.add_css_class("dim-label");
    proc_title_box.append(&proc_title);
    proc_title_box.append(&proc_sub);
    proc_header.append(&proc_icon);
    proc_header.append(&proc_title_box);
    proc_card.append(&proc_header);

    let proc_list_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    proc_card.append(&proc_list_box);
    grid.append(&proc_card);

    // 5. Battery & Power Card
    let bat_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    bat_card.add_css_class("finding-card");

    let bat_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let bat_icon = gtk::Image::from_icon_name("battery-symbolic");
    bat_icon.set_pixel_size(24);
    let bat_title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    bat_title_box.set_hexpand(true);
    let bat_title = gtk::Label::new(Some("Battery & Energy Draw"));
    bat_title.set_xalign(0.0);
    bat_title.add_css_class("heading");
    let bat_detail_lbl = gtk::Label::new(Some("Checking power supply…"));
    bat_detail_lbl.set_xalign(0.0);
    bat_detail_lbl.add_css_class("dim-label");
    bat_title_box.append(&bat_title);
    bat_title_box.append(&bat_detail_lbl);

    let bat_badge = gtk::Label::new(Some("—%"));
    bat_badge.add_css_class("risk-badge");
    bat_badge.add_css_class("risk-safe");
    bat_badge.set_valign(gtk::Align::Center);

    bat_header.append(&bat_icon);
    bat_header.append(&bat_title_box);
    bat_header.append(&bat_badge);
    bat_card.append(&bat_header);

    let bat_bar = gtk::ProgressBar::new();
    bat_card.append(&bat_bar);
    grid.append(&bat_card);

    // 6. Live Network Throughput Card
    let net_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    net_card.add_css_class("finding-card");

    let net_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let net_icon = gtk::Image::from_icon_name("network-wireless-symbolic");
    net_icon.set_pixel_size(24);
    let net_title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    net_title_box.set_hexpand(true);
    let net_title = gtk::Label::new(Some("Network Throughput"));
    net_title.set_xalign(0.0);
    net_title.add_css_class("heading");
    let net_iface_lbl = gtk::Label::new(Some("Interface: —"));
    net_iface_lbl.set_xalign(0.0);
    net_iface_lbl.add_css_class("dim-label");
    net_title_box.append(&net_title);
    net_title_box.append(&net_iface_lbl);

    let net_rates_box = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    let net_down_lbl = gtk::Label::new(Some("↓ 0 B/s"));
    net_down_lbl.add_css_class("heading");
    let net_up_lbl = gtk::Label::new(Some("↑ 0 B/s"));
    net_up_lbl.add_css_class("heading");
    net_rates_box.append(&net_down_lbl);
    net_rates_box.append(&net_up_lbl);

    let net_wave = make_sparkline_wave(net_history.clone(), (0.95, 0.55, 0.15), (0.98, 0.30, 0.40));
    net_card.append(&net_wave);

    net_header.append(&net_icon);
    net_header.append(&net_title_box);
    net_header.append(&net_rates_box);
    net_card.append(&net_header);
    grid.append(&net_card);

    // 7. Storage Partitions Card
    let stor_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    stor_card.add_css_class("finding-card");

    let stor_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let stor_icon = gtk::Image::from_icon_name("drive-multidisk-symbolic");
    stor_icon.set_pixel_size(24);
    let stor_title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    stor_title_box.set_hexpand(true);
    let stor_title = gtk::Label::new(Some("System Storage Partitions"));
    stor_title.set_xalign(0.0);
    stor_title.add_css_class("heading");
    let stor_detail_lbl = gtk::Label::new(Some("Root (/) and Home (/home)"));
    stor_detail_lbl.set_xalign(0.0);
    stor_detail_lbl.add_css_class("dim-label");
    stor_title_box.append(&stor_title);
    stor_title_box.append(&stor_detail_lbl);

    stor_header.append(&stor_icon);
    stor_header.append(&stor_title_box);
    stor_card.append(&stor_header);

    let stor_meters_box = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    stor_meters_box.set_homogeneous(true);

    let root_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let root_lbl = gtk::Label::new(Some("Root (/): —"));
    root_lbl.set_xalign(0.0);
    root_lbl.add_css_class("dim-label");
    let root_bar = gtk::ProgressBar::new();
    root_box.append(&root_lbl);
    root_box.append(&root_bar);

    let home_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let home_lbl = gtk::Label::new(Some("Home (/home): —"));
    home_lbl.set_xalign(0.0);
    home_lbl.add_css_class("dim-label");
    let home_bar = gtk::ProgressBar::new();
    home_box.append(&home_lbl);
    home_box.append(&home_bar);

    stor_meters_box.append(&root_box);
    stor_meters_box.append(&home_box);
    stor_card.append(&stor_meters_box);
    grid.append(&stor_card);

    // 8. Storage Speed Benchmark & Health Card
    let bench_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    bench_card.add_css_class("finding-card");

    let bench_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let bench_icon = gtk::Image::from_icon_name("drive-harddisk-solidstate-symbolic");
    bench_icon.set_pixel_size(24);
    let bench_title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    bench_title_box.set_hexpand(true);
    let bench_title = gtk::Label::new(Some("Storage Speed Benchmark"));
    bench_title.set_xalign(0.0);
    bench_title.add_css_class("heading");
    let bench_sub = gtk::Label::new(Some("Sequential filesystem throughput using a temporary 256 MiB test file. Direct I/O is preferred; unsupported or virtual-memory filesystems are never presented as physical disk speed."));
    bench_sub.set_xalign(0.0);
    bench_sub.add_css_class("dim-label");
    bench_title_box.append(&bench_title);
    bench_title_box.append(&bench_sub);

    let run_bench_btn = gtk::Button::with_label("🏎️ Run Speed Test");
    run_bench_btn.add_css_class("pill");
    run_bench_btn.add_css_class("suggested-action");
    run_bench_btn.set_valign(gtk::Align::Center);

    bench_header.append(&bench_icon);
    bench_header.append(&bench_title_box);
    bench_header.append(&run_bench_btn);
    bench_card.append(&bench_header);

    let bench_body = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    let speed_mb_rc = Rc::new(RefCell::new(0.0));
    let gauge_max_rc = Rc::new(RefCell::new(1000.0));
    let gauge = make_speedometer_gauge(speed_mb_rc.clone(), gauge_max_rc.clone());
    bench_body.append(&gauge);

    let bench_metrics = gtk::Box::new(gtk::Orientation::Vertical, 6);
    bench_metrics.set_hexpand(true);
    bench_metrics.set_valign(gtk::Align::Center);

    let write_lbl = gtk::Label::new(Some("Write Speed: — MB/s"));
    write_lbl.set_xalign(0.0);
    write_lbl.add_css_class("heading");
    let read_lbl = gtk::Label::new(Some("Read Speed: — MB/s"));
    read_lbl.set_xalign(0.0);
    read_lbl.add_css_class("heading");
    let mode_lbl = gtk::Label::new(Some("Mode: —"));
    mode_lbl.set_xalign(0.0);
    mode_lbl.set_wrap(true);
    mode_lbl.add_css_class("dim-label");

    bench_metrics.append(&write_lbl);
    bench_metrics.append(&read_lbl);
    bench_metrics.append(&mode_lbl);
    bench_body.append(&bench_metrics);
    bench_card.append(&bench_body);
    grid.append(&bench_card);

    let (tx_bench, rx_bench) = std::sync::mpsc::channel();
    let rx_bench_rc = Rc::new(RefCell::new(rx_bench));
    let toast_bench = toast_overlay.clone();
    let gauge_c = gauge.clone();
    let speed_mb_c = speed_mb_rc.clone();
    let gauge_max_c = gauge_max_rc.clone();
    let write_lbl_c = write_lbl.clone();
    let read_lbl_c = read_lbl.clone();
    let mode_lbl_c = mode_lbl.clone();

    run_bench_btn.connect_clicked(move |btn| {
        btn.set_sensitive(false);
        btn.set_label("Testing speed…");
        let tx = tx_bench.clone();
        std::thread::spawn(move || {
            let res = run_disk_benchmark();
            let _ = tx.send(res);
        });

        let btn_c = btn.clone();
        let toast_c = toast_bench.clone();
        let rx_c = rx_bench_rc.clone();
        let g_c = gauge_c.clone();
        let sp_c = speed_mb_c.clone();
        let max_c = gauge_max_c.clone();
        let w_c = write_lbl_c.clone();
        let r_c = read_lbl_c.clone();
        let m_c = mode_lbl_c.clone();

        glib::timeout_add_local(Duration::from_millis(50), move || {
            if let Ok(result) = rx_c.borrow().try_recv() {
                btn_c.set_sensitive(true);
                btn_c.set_label("🏎️ Run Speed Test");
                match (result.write_mib_s, result.read_mib_s) {
                    (Some(write_sp), Some(read_sp)) => {
                        w_c.set_text(&format!("Write Speed: {:.1} MiB/s", write_sp));
                        r_c.set_text(&format!("Read Speed: {:.1} MiB/s", read_sp));
                        m_c.set_text(&format!(
                            "Mode: {} • filesystem: {} • {} MiB test{}",
                            result.mode,
                            result.filesystem,
                            result.tested_mib,
                            result
                                .error
                                .as_deref()
                                .map(|warning| format!(" • {warning}"))
                                .unwrap_or_default()
                        ));
                        let max_sp = write_sp.max(read_sp);
                        *sp_c.borrow_mut() = max_sp;
                        *max_c.borrow_mut() = benchmark_gauge_scale(max_sp);
                        g_c.queue_draw();
                        toast_c.add_toast(adw::Toast::new(&format!(
                            "Storage test: Read {:.0} MiB/s • Write {:.0} MiB/s ({})",
                            read_sp, write_sp, result.mode
                        )));
                    }
                    _ => {
                        w_c.set_text("Write Speed: —");
                        r_c.set_text("Read Speed: —");
                        *sp_c.borrow_mut() = 0.0;
                        *max_c.borrow_mut() = 1000.0;
                        g_c.queue_draw();
                        m_c.set_text(&format!(
                            "{} • filesystem: {}{}",
                            result.mode,
                            result.filesystem,
                            result
                                .error
                                .as_deref()
                                .map(|error| format!(" • {error}"))
                                .unwrap_or_default()
                        ));
                        toast_c.add_toast(adw::Toast::new(
                            result
                                .error
                                .as_deref()
                                .unwrap_or("Storage benchmark unavailable"),
                        ));
                    }
                }
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    });

    // Live updater state
    let prev_cpu_stat = Rc::new(RefCell::new(None));
    let prev_net_stat = Rc::new(RefCell::new(None));
    let proc_container_rc = proc_list_box.clone();
    let toast_proc = toast_overlay.clone();

    let cpu_h = cpu_history.clone();
    let ram_h = ram_history.clone();
    let net_h = net_history.clone();
    let cpu_w = cpu_wave.clone();
    let ram_w = ram_wave.clone();
    let net_w = net_wave.clone();

    // 1-second live timer
    glib::timeout_add_local(Duration::from_millis(1000), move || {
        // 1. CPU & Temp
        let cpu = read_cpu_usage(&mut prev_cpu_stat.borrow_mut());
        cpu_model_lbl.set_text(&format!("{} ({} cores)", cpu.model_name, cpu.core_count));
        cpu_bar.set_fraction(cpu.usage_pct as f64 / 100.0);
        cpu_pct_lbl.set_text(&format!("{:.1}%", cpu.usage_pct));

        {
            let mut h = cpu_h.borrow_mut();
            h.push(cpu.usage_pct as f64);
            if h.len() > 30 {
                h.remove(0);
            }
        }
        cpu_w.queue_draw();

        if let Some(temp) = cpu.temp_c {
            cpu_temp_lbl.set_text(&format!("{temp:.1} °C"));
            if temp > 75.0 {
                cpu_temp_lbl.set_css_classes(&["risk-badge", "risk-dangerous"]);
            } else if temp > 60.0 {
                cpu_temp_lbl.set_css_classes(&["risk-badge", "risk-review"]);
            } else {
                cpu_temp_lbl.set_css_classes(&["risk-badge", "risk-safe"]);
            }
        } else {
            cpu_temp_lbl.set_text("N/A");
        }

        // 2. Memory & Swap
        let mem = read_memory();
        mem_detail_lbl.set_text(&format!(
            "RAM: {} / {} ({:.0}%)  •  Swap: {} / {} ({:.0}%)",
            crate::format::bytes(mem.ram_used),
            crate::format::bytes(mem.ram_total),
            mem.ram_pct * 100.0,
            crate::format::bytes(mem.swap_used),
            crate::format::bytes(mem.swap_total),
            mem.swap_pct * 100.0
        ));
        mem_pct_badge.set_text(&format!("{:.0}% RAM", mem.ram_pct * 100.0));
        ram_bar.set_fraction(mem.ram_pct as f64);
        swap_bar.set_fraction(mem.swap_pct as f64);

        {
            let mut h = ram_h.borrow_mut();
            h.push((mem.ram_pct * 100.0) as f64);
            if h.len() > 30 {
                h.remove(0);
            }
        }
        ram_w.queue_draw();

        // 3. Top Processes Refresh
        let procs = list_top_processes();
        while let Some(c) = proc_container_rc.first_child() {
            proc_container_rc.remove(&c);
        }
        for p in procs {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row.add_css_class("finding-card");
            row.set_margin_bottom(2);

            let icon = gtk::Image::from_icon_name("application-x-executable-symbolic");
            icon.set_pixel_size(18);

            let p_info = gtk::Box::new(gtk::Orientation::Vertical, 2);
            p_info.set_hexpand(true);
            let name_lbl = gtk::Label::new(Some(&p.name));
            name_lbl.set_xalign(0.0);
            name_lbl.add_css_class("heading");
            let pid_lbl = gtk::Label::new(Some(&format!(
                "PID: {} • RAM: {}",
                p.pid,
                crate::format::bytes(p.ram_bytes)
            )));
            pid_lbl.set_xalign(0.0);
            pid_lbl.add_css_class("dim-label");
            p_info.append(&name_lbl);
            p_info.append(&pid_lbl);

            let kill_btn = gtk::Button::with_label("End Task");
            kill_btn.add_css_class("pill");
            kill_btn.add_css_class("destructive-action");
            kill_btn.set_valign(gtk::Align::Center);

            let pid_to_kill = p.pid;
            let p_name = p.name.clone();
            let toast_k = toast_proc.clone();
            let armed = Rc::new(Cell::new(false));
            let armed_c = armed.clone();
            kill_btn.connect_clicked(move |btn| {
                if !armed_c.replace(true) {
                    btn.set_label("Confirm End Task");
                    toast_k.add_toast(adw::Toast::new(&format!(
                        "Click again to send SIGTERM to {p_name} ({pid_to_kill})"
                    )));
                    return;
                }

                armed_c.set(false);
                btn.set_label("End Task");
                let rc = unsafe { libc::kill(pid_to_kill as i32, libc::SIGTERM) };
                if rc == 0 {
                    toast_k.add_toast(adw::Toast::new(&format!(
                        "Sent terminate signal to {p_name} ({pid_to_kill})"
                    )));
                } else {
                    toast_k.add_toast(adw::Toast::new(&format!(
                        "Could not terminate {p_name} ({pid_to_kill})"
                    )));
                }
            });

            row.append(&icon);
            row.append(&p_info);
            row.append(&kill_btn);
            proc_container_rc.append(&row);
        }

        // 4. Battery
        let bat = read_battery();
        if bat.present {
            bat_badge.set_text(&format!("{}%", bat.percentage));
            bat_bar.set_fraction(bat.percentage as f64 / 100.0);
            let power_str = bat
                .power_watts
                .map(|w| format!(" • {w:.1} W"))
                .unwrap_or_default();
            let health_str = bat
                .health_pct
                .map(|h| format!(" • {h}% Health"))
                .unwrap_or_default();
            let time_str = bat
                .time_remaining_str
                .map(|t| format!(" • {t}"))
                .unwrap_or_default();
            bat_detail_lbl.set_text(&format!(
                "Status: {}{power_str}{health_str}{time_str}",
                bat.status
            ));
        } else {
            bat_badge.set_text("AC");
            bat_detail_lbl.set_text("Desktop / AC Power Connected (No battery detected)");
            bat_bar.set_fraction(1.0);
        }

        // 5. Network
        let net = read_network(&mut prev_net_stat.borrow_mut());
        net_iface_lbl.set_text(&format!("Active Interface: {}", net.active_iface));
        net_down_lbl.set_text(&format!("↓ {}/s", crate::format::bytes(net.download_rate)));
        net_up_lbl.set_text(&format!("↑ {}/s", crate::format::bytes(net.upload_rate)));

        {
            let mut h = net_h.borrow_mut();
            let mb_rate = (net.download_rate as f64 / 1024.0 / 1024.0 * 10.0).clamp(0.0, 100.0);
            h.push(mb_rate);
            if h.len() > 30 {
                h.remove(0);
            }
        }
        net_w.queue_draw();

        // 6. Storage
        let stor = read_storage();
        root_lbl.set_text(&format!(
            "Root (/): {} / {} ({:.0}%)",
            crate::format::bytes(stor.root_used),
            crate::format::bytes(stor.root_total),
            stor.root_pct * 100.0
        ));
        root_bar.set_fraction(stor.root_pct as f64);

        home_lbl.set_text(&format!(
            "Home (/home): {} / {} ({:.0}%)",
            crate::format::bytes(stor.home_used),
            crate::format::bytes(stor.home_total),
            stor.home_pct * 100.0
        ));
        home_bar.set_fraction(stor.home_pct as f64);

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

pub fn header_vitals_widget(stack: gtk::Stack, sidebar: gtk::ListBox) -> gtk::Widget {
    let btn = gtk::Button::new();
    btn.add_css_class("flat");
    btn.add_css_class("vitals-header-pill");
    btn.set_tooltip_text(Some("Live System Vitals (Click to open full dashboard)"));

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);

    let cpu_icon = gtk::Image::from_icon_name("processor-symbolic");
    cpu_icon.set_pixel_size(14);
    let cpu_lbl = gtk::Label::new(Some("CPU —%"));

    let mem_icon = gtk::Image::from_icon_name("drive-harddisk-symbolic");
    mem_icon.set_pixel_size(14);
    let mem_lbl = gtk::Label::new(Some("RAM —%"));

    let temp_lbl = gtk::Label::new(Some("—°C"));
    let net_lbl = gtk::Label::new(Some("↓ —"));

    hbox.append(&cpu_icon);
    hbox.append(&cpu_lbl);
    hbox.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    hbox.append(&mem_icon);
    hbox.append(&mem_lbl);
    hbox.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    hbox.append(&temp_lbl);
    hbox.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    hbox.append(&net_lbl);

    btn.set_child(Some(&hbox));

    btn.connect_clicked(move |_| {
        stack.set_visible_child_name("vitals");
        if let Some(row) = sidebar.row_at_index(3) {
            sidebar.select_row(Some(&row));
        }
    });

    let prev_cpu = Rc::new(RefCell::new(None));
    let prev_net = Rc::new(RefCell::new(None));
    let cpu_c = cpu_lbl.clone();
    let mem_c = mem_lbl.clone();
    let temp_c = temp_lbl.clone();
    let net_c = net_lbl.clone();

    glib::timeout_add_local(Duration::from_millis(1500), move || {
        let cpu = read_cpu_usage(&mut prev_cpu.borrow_mut());
        cpu_c.set_text(&format!("CPU {:.0}%", cpu.usage_pct));

        let mem = read_memory();
        mem_c.set_text(&format!("RAM {:.0}%", mem.ram_pct * 100.0));

        if let Some(temp) = cpu.temp_c {
            temp_c.set_text(&format!("{temp:.0}°C"));
        } else {
            temp_c.set_text("—°C");
        }

        let net = read_network(&mut prev_net.borrow_mut());
        net_c.set_text(&format!("↓ {}", crate::format::bytes(net.download_rate)));

        glib::ControlFlow::Continue
    });

    btn.upcast()
}

pub fn get_active_power_profile() -> String {
    std::process::Command::new("gdbus")
        .args([
            "call",
            "--system",
            "--dest",
            "net.hadess.PowerProfiles",
            "--object-path",
            "/net/hadess/PowerProfiles",
            "--method",
            "org.freedesktop.DBus.Properties.Get",
            "net.hadess.PowerProfiles",
            "ActiveProfile",
        ])
        .output()
        .ok()
        .map(|out| {
            let s = String::from_utf8_lossy(&out.stdout);
            if s.contains("'performance'") {
                "performance".to_string()
            } else if s.contains("'power-saver'") {
                "power-saver".to_string()
            } else {
                "balanced".to_string()
            }
        })
        .unwrap_or_else(|| "balanced".to_string())
}

pub fn set_power_profile(profile: &str) {
    let arg = format!("<'{profile}'>");
    let _ = std::process::Command::new("gdbus")
        .args([
            "call",
            "--system",
            "--dest",
            "net.hadess.PowerProfiles",
            "--object-path",
            "/net/hadess/PowerProfiles",
            "--method",
            "org.freedesktop.DBus.Properties.Set",
            "net.hadess.PowerProfiles",
            "ActiveProfile",
            &arg,
        ])
        .spawn();
}

pub fn list_top_processes() -> Vec<ProcessItem> {
    let mut procs = Vec::new();
    let my_uid = unsafe { libc::getuid() };

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if let Ok(pid) = fname.parse::<u32>() {
                if pid <= 1 {
                    continue;
                }
                let status_path = entry.path().join("status");
                if let Ok(status) = fs::read_to_string(status_path) {
                    let mut is_my_user = false;
                    let mut name = String::new();
                    let mut vmrss_kb = 0u64;

                    for line in status.lines() {
                        if line.starts_with("Name:") {
                            if let Some((_, n)) = line.split_once(':') {
                                name = n.trim().to_string();
                            }
                        } else if line.starts_with("Uid:") {
                            if let Some(uid_str) = line.split_whitespace().nth(1) {
                                if let Ok(u) = uid_str.parse::<u32>() {
                                    is_my_user = u == my_uid;
                                }
                            }
                        } else if line.starts_with("VmRSS:") {
                            if let Some(kb_str) = line.split_whitespace().nth(1) {
                                vmrss_kb = kb_str.parse().unwrap_or(0);
                            }
                        }
                    }

                    if is_my_user && vmrss_kb > 20_000 && !name.is_empty() {
                        procs.push(ProcessItem {
                            pid,
                            name,
                            ram_bytes: vmrss_kb * 1024,
                        });
                    }
                }
            }
        }
    }

    procs.sort_by_key(|p| std::cmp::Reverse(p.ram_bytes));
    procs.truncate(5);
    procs
}

fn read_cpu_usage(prev: &mut Option<(u64, u64)>) -> CpuStats {
    let mut stats = CpuStats::default();

    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        let mut cores = 0;
        for line in cpuinfo.lines() {
            if line.starts_with("model name") && stats.model_name.is_empty() {
                if let Some((_, model)) = line.split_once(':') {
                    stats.model_name = model.trim().to_string();
                }
            } else if line.starts_with("processor") {
                cores += 1;
            }
        }
        stats.core_count = cores.max(1);
    }
    if stats.model_name.is_empty() {
        stats.model_name = "Generic CPU".into();
    }

    if let Ok(stat) = fs::read_to_string("/proc/stat") {
        if let Some(cpu_line) = stat.lines().find(|l| l.starts_with("cpu ")) {
            let parts: Vec<u64> = cpu_line
                .split_whitespace()
                .skip(1)
                .filter_map(|s| s.parse().ok())
                .collect();
            if parts.len() >= 4 {
                let user = parts[0];
                let nice = parts[1];
                let system = parts[2];
                let idle = parts[3];
                let iowait = parts.get(4).copied().unwrap_or(0);
                let irq = parts.get(5).copied().unwrap_or(0);
                let softirq = parts.get(6).copied().unwrap_or(0);
                let steal = parts.get(7).copied().unwrap_or(0);

                let total_idle = idle + iowait;
                let total_time = user + nice + system + total_idle + irq + softirq + steal;

                if let Some((prev_total, prev_idle)) = *prev {
                    let total_diff = total_time.saturating_sub(prev_total);
                    let idle_diff = total_idle.saturating_sub(prev_idle);
                    if total_diff > 0 {
                        stats.usage_pct = ((total_diff.saturating_sub(idle_diff)) as f32
                            / total_diff as f32)
                            * 100.0;
                    }
                }
                *prev = Some((total_time, total_idle));
            }
        }
    }

    stats.temp_c = read_temperature();
    stats
}

fn read_temperature() -> Option<f32> {
    let mut best_candidate: Option<(u8, f32)> = None; // (priority, temp_c)

    // 1. Scan hwmon devices
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = fs::read_to_string(path.join("name"))
                .unwrap_or_default()
                .trim()
                .to_lowercase();

            let base_priority: u8 = match name.as_str() {
                "coretemp" | "k10temp" | "zenpower" => 90,
                "cpu_thermal" | "soc_thermal" => 85,
                "dell_smm" | "thinkpad" | "applesmc" => 70,
                "acpitz" => 10,
                "nvme" | "iwlwifi" | "amdgpu" | "nouveau" => 5,
                _ => 30,
            };

            if let Ok(files) = fs::read_dir(&path) {
                for file in files.flatten() {
                    let fname = file.file_name().to_string_lossy().to_string();
                    if fname.starts_with("temp") && fname.ends_with("_input") {
                        let prefix = fname.trim_end_matches("_input");
                        let label_path = path.join(format!("{prefix}_label"));
                        let label = fs::read_to_string(&label_path)
                            .unwrap_or_default()
                            .trim()
                            .to_lowercase();

                        let mut prio = base_priority;
                        if label.contains("package")
                            || label.contains("tctl")
                            || label.contains("tdie")
                        {
                            prio = prio.saturating_add(10);
                        } else if label.contains("core") {
                            prio = prio.saturating_add(5);
                        }

                        if let Ok(content) = fs::read_to_string(file.path()) {
                            if let Ok(milli) = content.trim().parse::<f32>() {
                                if (15_000.0..=125_000.0).contains(&milli) {
                                    let temp_c = milli / 1000.0;
                                    match best_candidate {
                                        Some((cur_prio, cur_temp)) => {
                                            if prio > cur_prio
                                                || (prio == cur_prio && temp_c > cur_temp)
                                            {
                                                best_candidate = Some((prio, temp_c));
                                            }
                                        }
                                        None => {
                                            best_candidate = Some((prio, temp_c));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Scan thermal_zones
    if best_candidate
        .as_ref()
        .map(|(p, _)| *p < 70)
        .unwrap_or(true)
    {
        if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
            for entry in entries.flatten() {
                let path = entry.path();
                let type_name = fs::read_to_string(path.join("type"))
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase();

                let prio: u8 = match type_name.as_str() {
                    "x86_pkg_temp" | "cpu-thermal" | "k10temp" | "coretemp" => 95,
                    "b0d4" | "soc_thermal" => 75,
                    "acpitz" => 10,
                    _ => 20,
                };

                if let Ok(content) = fs::read_to_string(path.join("temp")) {
                    if let Ok(milli) = content.trim().parse::<f32>() {
                        if (15_000.0..=125_000.0).contains(&milli) {
                            let temp_c = milli / 1000.0;
                            match best_candidate {
                                Some((cur_prio, cur_temp)) => {
                                    if prio > cur_prio || (prio == cur_prio && temp_c > cur_temp) {
                                        best_candidate = Some((prio, temp_c));
                                    }
                                }
                                None => {
                                    best_candidate = Some((prio, temp_c));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    best_candidate.map(|(_, temp)| temp)
}

fn read_memory() -> MemoryStats {
    let mut stats = MemoryStats::default();
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        let mut mem_total = 0u64;
        let mut mem_avail = 0u64;
        let mut swap_total = 0u64;
        let mut swap_free = 0u64;

        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                mem_total = parse_kb_line(line) * 1024;
            } else if line.starts_with("MemAvailable:") {
                mem_avail = parse_kb_line(line) * 1024;
            } else if line.starts_with("SwapTotal:") {
                swap_total = parse_kb_line(line) * 1024;
            } else if line.starts_with("SwapFree:") {
                swap_free = parse_kb_line(line) * 1024;
            }
        }

        stats.ram_total = mem_total;
        stats.ram_used = mem_total.saturating_sub(mem_avail);
        stats.ram_pct = if mem_total > 0 {
            stats.ram_used as f32 / mem_total as f32
        } else {
            0.0
        };

        stats.swap_total = swap_total;
        stats.swap_used = swap_total.saturating_sub(swap_free);
        stats.swap_pct = if swap_total > 0 {
            stats.swap_used as f32 / swap_total as f32
        } else {
            0.0
        };
    }
    stats
}

fn parse_kb_line(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
}

fn read_battery() -> BatteryStats {
    let report = crate::battery::collect_report();
    let battery = report
        .batteries
        .iter()
        .find(|battery| battery.scope.as_deref() == Some("System"))
        .or_else(|| report.batteries.first());

    let Some(battery) = battery else {
        return BatteryStats::default();
    };

    BatteryStats {
        present: true,
        percentage: battery.capacity_percent.unwrap_or(0),
        status: battery.status.clone(),
        power_watts: battery.power_w.map(|value| value as f32),
        health_pct: battery
            .health_percent
            .map(|value| value.min(100.0).round() as u8),
        time_remaining_str: battery
            .time_remaining_minutes
            .map(|minutes| crate::battery::format_minutes(Some(minutes), &battery.status)),
    }
}

fn read_network(prev: &mut Option<(u64, u64, Instant)>) -> NetworkStats {
    let mut stats = NetworkStats::default();
    let mut total_rx = 0u64;
    let mut total_tx = 0u64;
    let mut best_iface = "none".to_string();
    let mut best_iface_traffic = 0u64;

    if let Ok(dev) = fs::read_to_string("/proc/net/dev") {
        for line in dev.lines().skip(2) {
            if let Some((iface, counts)) = line.split_once(':') {
                let iface_name = iface.trim();
                if iface_name == "lo" {
                    continue;
                }
                let cols: Vec<u64> = counts
                    .split_whitespace()
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if cols.len() >= 9 {
                    let rx = cols[0];
                    let tx = cols[8];
                    total_rx += rx;
                    total_tx += tx;
                    if rx + tx > best_iface_traffic {
                        best_iface_traffic = rx + tx;
                        best_iface = iface_name.to_string();
                    }
                }
            }
        }
    }

    stats.total_rx = total_rx;
    stats.total_tx = total_tx;
    stats.active_iface = best_iface;

    let now = Instant::now();
    if let Some((p_rx, p_tx, p_time)) = *prev {
        let elapsed = now.duration_since(p_time).as_secs_f64();
        if elapsed > 0.001 {
            let rx_diff = total_rx.saturating_sub(p_rx);
            let tx_diff = total_tx.saturating_sub(p_tx);
            stats.download_rate = (rx_diff as f64 / elapsed) as u64;
            stats.upload_rate = (tx_diff as f64 / elapsed) as u64;
        }
    }
    *prev = Some((total_rx, total_tx, now));

    stats
}

fn read_storage() -> StorageStats {
    let mut stats = StorageStats::default();
    let (root_u, root_t, root_p) = read_disk_stat("/");
    let (home_u, home_t, home_p) = read_disk_stat("/home");

    stats.root_used = root_u;
    stats.root_total = root_t;
    stats.root_pct = root_p;

    stats.home_used = home_u;
    stats.home_total = home_t;
    stats.home_pct = home_p;

    stats
}

fn read_disk_stat(path: &str) -> (u64, u64, f32) {
    let Ok(c_path) = CString::new(path) else {
        return (0, 0, 0.0);
    };
    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
    if unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) } == 0 {
        let stat = unsafe { stat.assume_init() };
        let block_size = stat.f_frsize;
        let total_bytes = stat.f_blocks * block_size;
        let free_bytes = stat.f_bavail * block_size;
        let used_bytes = total_bytes.saturating_sub(free_bytes);
        let pct = if total_bytes > 0 {
            (used_bytes as f64 / total_bytes as f64) as f32
        } else {
            0.0
        };
        (used_bytes, total_bytes, pct)
    } else {
        (0, 0, 0.0)
    }
}

pub fn make_sparkline_wave(
    history: Rc<RefCell<Vec<f64>>>,
    color_start: (f64, f64, f64),
    color_end: (f64, f64, f64),
) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_height(48);
    area.set_hexpand(true);

    let hist_c = history.clone();
    area.set_draw_func(move |_, cr, width, height| {
        let w = width as f64;
        let h = height as f64;
        let data = hist_c.borrow();
        if data.len() < 2 {
            return;
        }

        let n = data.len();
        let step = w / (n - 1) as f64;

        let mut points: Vec<(f64, f64)> = Vec::with_capacity(n);
        for (i, &val) in data.iter().enumerate() {
            let x = i as f64 * step;
            let clamped = val.clamp(0.0, 100.0) / 100.0;
            let y = h - (clamped * (h - 10.0) + 5.0);
            points.push((x, y));
        }

        // Fill area under curve
        cr.move_to(0.0, h);
        for (i, pt) in points.iter().enumerate() {
            if i == 0 {
                cr.line_to(pt.0, pt.1);
            } else {
                let prev = points[i - 1];
                let mid_x = (prev.0 + pt.0) / 2.0;
                cr.curve_to(mid_x, prev.1, mid_x, pt.1, pt.0, pt.1);
            }
        }
        cr.line_to(w, h);
        cr.close_path();

        let fill_grad = cairo::LinearGradient::new(0.0, 0.0, 0.0, h);
        fill_grad.add_color_stop_rgba(0.0, color_start.0, color_start.1, color_start.2, 0.32);
        fill_grad.add_color_stop_rgba(1.0, color_end.0, color_end.1, color_end.2, 0.02);
        let _ = cr.set_source(&fill_grad);
        let _ = cr.fill();

        // Stroke line with glow
        cr.move_to(points[0].0, points[0].1);
        for (i, pt) in points.iter().enumerate().skip(1) {
            let prev = points[i - 1];
            let mid_x = (prev.0 + pt.0) / 2.0;
            cr.curve_to(mid_x, prev.1, mid_x, pt.1, pt.0, pt.1);
        }

        let stroke_grad = cairo::LinearGradient::new(0.0, 0.0, w, 0.0);
        stroke_grad.add_color_stop_rgba(0.0, color_start.0, color_start.1, color_start.2, 0.85);
        stroke_grad.add_color_stop_rgba(1.0, color_end.0, color_end.1, color_end.2, 0.95);
        let _ = cr.set_source(&stroke_grad);
        cr.set_line_width(2.5);
        cr.set_line_cap(cairo::LineCap::Round);
        let _ = cr.stroke();

        // Pulsing head beacon
        if let Some(last) = points.last() {
            cr.set_source_rgba(color_end.0, color_end.1, color_end.2, 1.0);
            cr.arc(last.0, last.1, 3.5, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
    });

    area
}

pub fn make_speedometer_gauge(
    speed_mb: Rc<RefCell<f64>>,
    max_mb: Rc<RefCell<f64>>,
) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(180);
    area.set_content_height(100);

    let sp_c = speed_mb.clone();
    let max_c = max_mb.clone();
    area.set_draw_func(move |_, cr, width, height| {
        let cx = width as f64 / 2.0;
        let cy = height as f64 - 10.0;
        let radius = (width.min(height * 2) as f64 / 2.0) - 16.0;

        let start_angle = std::f64::consts::PI;
        let end_angle = std::f64::consts::TAU;

        // Background track arc
        cr.set_line_width(10.0);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_source_rgba(0.35, 0.40, 0.50, 0.20);
        cr.arc(cx, cy, radius, start_angle, end_angle);
        let _ = cr.stroke();

        // Speed active arc
        let speed = *sp_c.borrow();
        let max_speed = (*max_c.borrow()).max(1.0);
        let fraction = (speed / max_speed).clamp(0.0, 1.0);
        let cur_angle = start_angle + fraction * (end_angle - start_angle);

        let grad = cairo::LinearGradient::new(cx - radius, cy, cx + radius, cy);
        grad.add_color_stop_rgb(0.0, 0.20, 0.65, 1.0);
        grad.add_color_stop_rgb(0.6, 0.15, 0.85, 0.60);
        grad.add_color_stop_rgb(1.0, 0.95, 0.30, 0.35);
        let _ = cr.set_source(&grad);

        cr.arc(cx, cy, radius, start_angle, cur_angle);
        let _ = cr.stroke();

        // Needle
        let needle_len = radius - 8.0;
        let nx = cx + needle_len * cur_angle.cos();
        let ny = cy + needle_len * cur_angle.sin();

        cr.set_line_width(3.0);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        cr.move_to(cx, cy);
        cr.line_to(nx, ny);
        let _ = cr.stroke();

        // Hub circle
        cr.set_source_rgba(0.20, 0.65, 1.0, 1.0);
        cr.arc(cx, cy, 6.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    });

    area
}

fn benchmark_gauge_scale(speed_mib_s: f64) -> f64 {
    for scale in [600.0, 1200.0, 2500.0, 5000.0, 10_000.0, 20_000.0] {
        if speed_mib_s <= scale {
            return scale;
        }
    }
    (speed_mib_s * 1.15).max(20_000.0)
}

#[derive(Debug, Clone)]
pub struct DiskBenchmarkResult {
    pub write_mib_s: Option<f64>,
    pub read_mib_s: Option<f64>,
    pub mode: String,
    pub filesystem: String,
    pub tested_mib: u64,
    pub error: Option<String>,
}

pub fn run_disk_benchmark() -> DiskBenchmarkResult {
    use std::alloc::{alloc_zeroed, dealloc, Layout};
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    use std::ptr::NonNull;

    const TOTAL_BYTES: usize = 256 * 1024 * 1024;
    const CHUNK_BYTES: usize = 4 * 1024 * 1024;
    const ALIGNMENT: usize = 4096;
    const TOTAL_MIB: f64 = TOTAL_BYTES as f64 / (1024.0 * 1024.0);

    struct AlignedBuffer {
        ptr: NonNull<u8>,
        layout: Layout,
    }

    impl AlignedBuffer {
        fn new(len: usize, alignment: usize) -> std::io::Result<Self> {
            let layout = Layout::from_size_align(len, alignment)
                .map_err(|_| std::io::Error::other("invalid benchmark buffer layout"))?;
            let raw = unsafe { alloc_zeroed(layout) };
            let ptr = NonNull::new(raw)
                .ok_or_else(|| std::io::Error::other("could not allocate benchmark buffer"))?;
            Ok(Self { ptr, layout })
        }

        fn as_slice(&self) -> &[u8] {
            unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.layout.size()) }
        }

        fn as_mut_slice(&mut self) -> &mut [u8] {
            unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.layout.size()) }
        }
    }

    impl Drop for AlignedBuffer {
        fn drop(&mut self) {
            unsafe { dealloc(self.ptr.as_ptr(), self.layout) };
        }
    }

    fn filesystem_type_for_path(path: &std::path::Path) -> String {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let target = canonical.to_string_lossy();
        let Ok(mountinfo) = std::fs::read_to_string("/proc/self/mountinfo") else {
            return "unknown".into();
        };
        let mut best: Option<(usize, String)> = None;
        for line in mountinfo.lines() {
            let Some((left, right)) = line.split_once(" - ") else {
                continue;
            };
            let mut left_fields = left.split_whitespace();
            let mountpoint = left_fields.nth(4).unwrap_or("");
            let mountpoint = mountpoint
                .replace("\\040", " ")
                .replace("\\011", "\t")
                .replace("\\134", "\\");
            let fs_type = right.split_whitespace().next().unwrap_or("unknown");
            let matches = mountpoint == "/"
                || target == mountpoint
                || target
                    .strip_prefix(&mountpoint)
                    .is_some_and(|rest| rest.starts_with('/'));
            if matches {
                let len = mountpoint.len();
                if best.as_ref().is_none_or(|(best_len, _)| len > *best_len) {
                    best = Some((len, fs_type.to_string()));
                }
            }
        }
        best.map(|(_, fs_type)| fs_type)
            .unwrap_or_else(|| "unknown".into())
    }

    fn free_bytes(path: &std::path::Path) -> Option<u64> {
        let c_path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
        let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
        if unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) } != 0 {
            return None;
        }
        let stat = unsafe { stat.assume_init() };
        Some(stat.f_bavail.saturating_mul(stat.f_frsize))
    }

    fn run_direct(
        path: &std::path::Path,
        total_bytes: usize,
        chunk_bytes: usize,
        alignment: usize,
    ) -> std::io::Result<(f64, f64)> {
        let mut buffer = AlignedBuffer::new(chunk_bytes, alignment)?;
        buffer.as_mut_slice().fill(0x55);
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .custom_flags(libc::O_DIRECT)
            .open(path)?;

        let write_started = Instant::now();
        let mut written = 0usize;
        while written < total_bytes {
            file.write_all(buffer.as_slice())?;
            written += chunk_bytes;
        }
        file.sync_data()?;
        let write_secs = write_started.elapsed().as_secs_f64();
        drop(file);

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECT)
            .open(path)?;
        let read_started = Instant::now();
        let mut read_total = 0usize;
        while read_total < total_bytes {
            let count = file.read(buffer.as_mut_slice())?;
            if count == 0 {
                break;
            }
            read_total += count;
        }
        let read_secs = read_started.elapsed().as_secs_f64();
        if read_total != total_bytes || write_secs <= 0.0 || read_secs <= 0.0 {
            return Err(std::io::Error::other("incomplete direct-I/O benchmark"));
        }
        Ok((
            total_bytes as f64 / (1024.0 * 1024.0) / write_secs,
            total_bytes as f64 / (1024.0 * 1024.0) / read_secs,
        ))
    }

    fn run_buffered(
        path: &std::path::Path,
        total_bytes: usize,
        chunk_bytes: usize,
    ) -> std::io::Result<(f64, f64)> {
        let buffer = vec![0x55u8; chunk_bytes];
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(path)?;

        let write_started = Instant::now();
        let mut written = 0usize;
        while written < total_bytes {
            file.write_all(&buffer)?;
            written += chunk_bytes;
        }
        file.sync_data()?;
        let write_secs = write_started.elapsed().as_secs_f64();
        let advise =
            unsafe { libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
        drop(file);
        if advise != 0 {
            return Err(std::io::Error::from_raw_os_error(advise));
        }

        let mut file = std::fs::File::open(path)?;
        let advise =
            unsafe { libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_SEQUENTIAL) };
        if advise != 0 {
            return Err(std::io::Error::from_raw_os_error(advise));
        }
        let mut read_buffer = vec![0u8; chunk_bytes];
        let read_started = Instant::now();
        let mut read_total = 0usize;
        loop {
            let count = file.read(&mut read_buffer)?;
            if count == 0 {
                break;
            }
            read_total += count;
        }
        let read_secs = read_started.elapsed().as_secs_f64();
        if read_total != total_bytes || write_secs <= 0.0 || read_secs <= 0.0 {
            return Err(std::io::Error::other("incomplete buffered benchmark"));
        }
        Ok((
            total_bytes as f64 / (1024.0 * 1024.0) / write_secs,
            total_bytes as f64 / (1024.0 * 1024.0) / read_secs,
        ))
    }

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let cache_dir = home.join(".cache").join("linuxcare");
    if let Err(error) = std::fs::create_dir_all(&cache_dir) {
        return DiskBenchmarkResult {
            write_mib_s: None,
            read_mib_s: None,
            mode: "Unavailable".into(),
            filesystem: "unknown".into(),
            tested_mib: 0,
            error: Some(format!("Could not create benchmark directory: {error}")),
        };
    }

    let filesystem = filesystem_type_for_path(&cache_dir);
    if [
        "tmpfs", "ramfs", "overlay", "squashfs", "proc", "sysfs", "devtmpfs",
    ]
    .contains(&filesystem.as_str())
    {
        return DiskBenchmarkResult {
            write_mib_s: None,
            read_mib_s: None,
            mode: "Refused".into(),
            filesystem: filesystem.clone(),
            tested_mib: 0,
            error: Some(format!(
                "Benchmark refused on {filesystem}: this would not represent physical storage throughput."
            )),
        };
    }

    let reserve = TOTAL_BYTES as u64 + 512 * 1024 * 1024;
    if free_bytes(&cache_dir).is_some_and(|available| available < reserve) {
        return DiskBenchmarkResult {
            write_mib_s: None,
            read_mib_s: None,
            mode: "Refused".into(),
            filesystem,
            tested_mib: 0,
            error: Some("Benchmark refused because less than ~768 MiB is safely available on the target filesystem.".into()),
        };
    }

    let bench_file = cache_dir.join(format!("benchmark-{}.tmp", std::process::id()));
    let direct = run_direct(&bench_file, TOTAL_BYTES, CHUNK_BYTES, ALIGNMENT);
    let _ = std::fs::remove_file(&bench_file);

    match direct {
        Ok((write_mib_s, read_mib_s)) => DiskBenchmarkResult {
            write_mib_s: Some(write_mib_s),
            read_mib_s: Some(read_mib_s),
            mode: "Direct I/O".into(),
            filesystem,
            tested_mib: TOTAL_MIB as u64,
            error: None,
        },
        Err(direct_error) => {
            let buffered = run_buffered(&bench_file, TOTAL_BYTES, CHUNK_BYTES);
            let _ = std::fs::remove_file(&bench_file);
            match buffered {
                Ok((write_mib_s, read_mib_s)) => DiskBenchmarkResult {
                    write_mib_s: Some(write_mib_s),
                    read_mib_s: Some(read_mib_s),
                    mode: "Buffered fallback".into(),
                    filesystem,
                    tested_mib: TOTAL_MIB as u64,
                    error: Some(format!(
                        "Direct I/O was unavailable ({direct_error}); page-cache eviction was requested for the fallback read."
                    )),
                },
                Err(buffered_error) => DiskBenchmarkResult {
                    write_mib_s: None,
                    read_mib_s: None,
                    mode: "Failed".into(),
                    filesystem,
                    tested_mib: 0,
                    error: Some(format!(
                        "Direct I/O failed ({direct_error}); buffered fallback also failed ({buffered_error})."
                    )),
                },
            }
        }
    }
}
