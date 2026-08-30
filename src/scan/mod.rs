pub mod fs_stats;
pub mod scanners;

use crate::model::ScannerResult;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc,
    },
    thread,
};

pub trait Scanner: Send + Sync {
    fn name(&self) -> &'static str;
    fn scan(&self, ctx: &ScanContext) -> ScannerResult;
}

#[derive(Clone)]
pub struct ScanContext {
    pub home: PathBuf,
    pub exclusions: Vec<String>,
    pub cancel: Arc<AtomicBool>,
}

impl ScanContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn is_excluded(&self, path: &std::path::Path) -> bool {
        self.exclusions.iter().any(|ex| {
            let ex_path = std::path::Path::new(ex);
            path.starts_with(ex_path) || path == ex_path
        })
    }
}

#[derive(Debug)]
pub enum ScanEvent {
    Started { total_modules: usize },
    ModuleStarted { index: usize, name: String },
    ModuleFinished { index: usize, result: ScannerResult },
    Cancelled,
    Finished,
}

pub struct ScanHandle {
    cancel: Arc<AtomicBool>,
}

impl ScanHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub fn start_scan(home: PathBuf, exclusions: Vec<String>, sender: Sender<ScanEvent>) -> ScanHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let thread_cancel = cancel.clone();

    thread::spawn(move || {
        let scanners = scanners::default_scanners();
        let total = scanners.len();
        let _ = sender.send(ScanEvent::Started {
            total_modules: total,
        });
        let ctx = ScanContext {
            home,
            exclusions: exclusions.clone(),
            cancel: thread_cancel.clone(),
        };

        for (index, scanner) in scanners.into_iter().enumerate() {
            if ctx.is_cancelled() {
                let _ = sender.send(ScanEvent::Cancelled);
                return;
            }

            let _ = sender.send(ScanEvent::ModuleStarted {
                index,
                name: scanner.name().to_string(),
            });
            let mut result = scanner.scan(&ctx);
            if !ctx.exclusions.is_empty() {
                result
                    .candidates
                    .retain(|c| c.path.as_ref().is_none_or(|p| !ctx.is_excluded(p)));
            }
            let _ = sender.send(ScanEvent::ModuleFinished { index, result });
        }

        if ctx.is_cancelled() {
            let _ = sender.send(ScanEvent::Cancelled);
        } else {
            let _ = sender.send(ScanEvent::Finished);
        }
    });

    ScanHandle { cancel }
}
