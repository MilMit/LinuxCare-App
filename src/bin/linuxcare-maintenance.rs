use std::path::PathBuf;

fn main() {
    let scheduled = std::env::args().any(|arg| arg == "--scheduled");
    if !scheduled {
        eprintln!(
            "linuxcare-maintenance is an internal scheduled-maintenance runner; use --scheduled"
        );
        std::process::exit(2);
    }
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        eprintln!("LinuxCare scheduled maintenance requires HOME to be set");
        std::process::exit(1);
    };
    match linuxcare::automation_engine::run_scheduled(&home) {
        Ok(report) => {
            println!(
                "LinuxCare scheduled maintenance: {} candidates, {} observed, {} quarantined, {} errors",
                report.candidates,
                linuxcare::format::bytes(report.observed_bytes),
                linuxcare::format::bytes(report.quarantined_bytes),
                report.errors
            );
        }
        Err(err) => {
            eprintln!("LinuxCare scheduled maintenance failed: {err}");
            std::process::exit(1);
        }
    }
}
