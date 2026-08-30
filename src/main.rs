use adw::prelude::*;

const APP_ID: &str = "net.milmit.LinuxCare";

fn main() -> glib::ExitCode {
    let mut args = std::env::args();
    let _program = args.next();
    if let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("LinuxCare {}", env!("CARGO_PKG_VERSION"));
                return glib::ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                println!(
                    "LinuxCare {}\n\nUsage: linuxcare [OPTION]\n\n  -h, --help       Show this help\n  -V, --version    Show version\n",
                    env!("CARGO_PKG_VERSION")
                );
                return glib::ExitCode::SUCCESS;
            }
            _ => {}
        }
    }

    let smoke_mode =
        std::env::var_os("LINUXCARE_GUI_SMOKE").as_deref() == Some(std::ffi::OsStr::new("1"));

    let mut builder = adw::Application::builder().application_id(APP_ID);
    if smoke_mode {
        // GUI smoke tests must not depend on owning the real desktop session-bus
        // application name. NON_UNIQUE keeps the test self-contained under Xvfb.
        builder = builder.flags(gtk::gio::ApplicationFlags::NON_UNIQUE);
    }

    let application = builder.build();
    application.connect_activate(linuxcare::app::build_ui);
    application.run()
}
