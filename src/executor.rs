use crate::{
    cleanup,
    model::{CleanupCandidate, CleanupOperationResult, ExecutionKind, PrivilegedAction},
    providers::{JournalProvider, PackageProvider, SnapProvider, UbuntuSystemProvider},
    quarantine,
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::Sender,
    thread,
};
use thiserror::Error;

#[derive(Debug)]
struct ExecutionOutcome {
    recovered_bytes: u64,
    quarantined_bytes: u64,
    undo_id: Option<String>,
    message: String,
}

impl ExecutionOutcome {
    fn freed(bytes: u64, message: impl Into<String>) -> Self {
        Self {
            recovered_bytes: bytes,
            quarantined_bytes: 0,
            undo_id: None,
            message: message.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error(transparent)]
    User(#[from] cleanup::CleanupError),
    #[error("candidate does not define a privileged action")]
    MissingPrivilegedAction,
    #[error("candidate is analysis-only")]
    AnalysisOnly,
    #[error("{0}")]
    Provider(String),
}

#[derive(Debug, Clone)]
pub struct CleanupHandle {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl CleanupHandle {
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[derive(Debug)]
pub enum CleanupEvent {
    Started {
        operations: usize,
    },
    ActionStarted {
        index: usize,
        title: String,
    },
    ActionFinished {
        index: usize,
        title: String,
        recovered_bytes: u64,
        error: Option<String>,
        operation_result: CleanupOperationResult,
    },
    Cancelled {
        recovered_bytes: u64,
        errors: Vec<String>,
        results: Vec<CleanupOperationResult>,
    },
    Finished {
        recovered_bytes: u64,
        errors: Vec<String>,
        results: Vec<CleanupOperationResult>,
    },
}

pub fn start_cleanup(
    candidates: Vec<CleanupCandidate>,
    home: PathBuf,
    sender: Sender<CleanupEvent>,
) -> CleanupHandle {
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancelled_thread = cancelled.clone();

    thread::spawn(move || {
        let _ = sender.send(CleanupEvent::Started {
            operations: candidates.len(),
        });
        let mut recovered = 0u64;
        let mut errors = Vec::new();
        let mut results = Vec::new();
        let mut provider: Option<Result<UbuntuSystemProvider, String>> = None;

        for (index, candidate) in candidates.into_iter().enumerate() {
            if cancelled_thread.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = sender.send(CleanupEvent::Cancelled {
                    recovered_bytes: recovered,
                    errors,
                    results,
                });
                return;
            }

            let _ = sender.send(CleanupEvent::ActionStarted {
                index,
                title: candidate.title.clone(),
            });

            let result: Result<ExecutionOutcome, ExecutionError> = match candidate.execution {
                ExecutionKind::UserAllowedDirectory => {
                    execute_user_allowed_directory(&candidate, &home)
                }
                ExecutionKind::PrivilegedProvider => {
                    let p = provider.get_or_insert_with(|| {
                        UbuntuSystemProvider::connect().map_err(|e| e.to_string())
                    });
                    match p {
                        Ok(p) => execute_privileged(p, &candidate).map(|bytes| {
                            ExecutionOutcome::freed(
                                bytes,
                                if bytes == 0 {
                                    "Completed successfully; reclaimed size was not reported"
                                } else {
                                    "Cleaned successfully"
                                },
                            )
                        }),
                        Err(err) => Err(ExecutionError::Provider(err.clone())),
                    }
                }
                ExecutionKind::FlatpakUnused => {
                    use crate::providers::flatpak::{FlatpakCliProvider, FlatpakProvider};
                    let user_scope = candidate
                        .privileged_action
                        .as_ref()
                        .map(|a| match a {
                            PrivilegedAction::RemoveUnusedFlatpakRuntimes { user_scope } => {
                                *user_scope
                            }
                            _ => true,
                        })
                        .unwrap_or(true);
                    FlatpakCliProvider
                        .remove_unused_runtimes(user_scope)
                        .map(|bytes| {
                            ExecutionOutcome::freed(
                                bytes,
                                if bytes == 0 {
                                    "Completed successfully; reclaimed size was not reported"
                                } else {
                                    "Cleaned successfully"
                                },
                            )
                        })
                        .map_err(|e| ExecutionError::Provider(e.to_string()))
                }
                ExecutionKind::AnalysisOnly => Err(ExecutionError::AnalysisOnly),
            };

            let (status, outcome, msg) = match result {
                Ok(outcome) => {
                    let msg = outcome.message.clone();
                    (crate::model::CleanupStatus::Success, Some(outcome), msg)
                }
                Err(err) => (crate::model::CleanupStatus::Failed, None, format!("{err}")),
            };
            let rec_bytes = outcome
                .as_ref()
                .map(|value| value.recovered_bytes)
                .unwrap_or(0);
            let quarantined_bytes = outcome
                .as_ref()
                .map(|value| value.quarantined_bytes)
                .unwrap_or(0);
            let undo_id = outcome.as_ref().and_then(|value| value.undo_id.clone());

            if status == crate::model::CleanupStatus::Success {
                recovered = recovered.saturating_add(rec_bytes);
            } else {
                errors.push(format!("{}: {msg}", candidate.title));
            }

            let op_res = CleanupOperationResult {
                candidate_id: candidate.id.clone(),
                title: candidate.title.clone(),
                category: candidate.category,
                status,
                estimated_bytes: candidate.size_bytes,
                recovered_bytes: rec_bytes,
                quarantined_bytes,
                undo_id,
                requires_privilege: candidate.requires_privilege,
                message: msg.clone(),
            };
            results.push(op_res.clone());

            let _ = sender.send(CleanupEvent::ActionFinished {
                index,
                title: candidate.title,
                recovered_bytes: rec_bytes,
                error: if status == crate::model::CleanupStatus::Failed {
                    Some(msg)
                } else {
                    None
                },
                operation_result: op_res,
            });
        }

        let _ = sender.send(CleanupEvent::Finished {
            recovered_bytes: recovered,
            errors,
            results,
        });
    });

    CleanupHandle { cancelled }
}

fn execute_user_allowed_directory(
    candidate: &CleanupCandidate,
    home: &Path,
) -> Result<ExecutionOutcome, ExecutionError> {
    let outcome = quarantine::quarantine_candidate(candidate, home)
        .map_err(|err| ExecutionError::Provider(err.to_string()))?;

    Ok(ExecutionOutcome {
        recovered_bytes: 0,
        quarantined_bytes: outcome.protected_bytes,
        undo_id: Some(outcome.id),
        message: "Moved to Safety Quarantine; disk space is reclaimed only after purge".to_string(),
    })
}

fn execute_privileged(
    provider: &UbuntuSystemProvider,
    candidate: &CleanupCandidate,
) -> Result<u64, ExecutionError> {
    let action = candidate
        .privileged_action
        .as_ref()
        .ok_or(ExecutionError::MissingPrivilegedAction)?;

    match action {
        PrivilegedAction::CleanAptCache => provider
            .clean_package_cache()
            .map_err(|e| ExecutionError::Provider(e.to_string())),
        PrivilegedAction::AutoremovePackages => provider
            .autoremove_packages()
            .map_err(|e| ExecutionError::Provider(e.to_string())),
        PrivilegedAction::RemoveDisabledSnapRevision { name, revision } => provider
            .remove_disabled_revision(name, revision)
            .map_err(|e| ExecutionError::Provider(e.to_string())),
        PrivilegedAction::VacuumJournal { keep_days } => provider
            .vacuum_journal(*keep_days)
            .map_err(|e| ExecutionError::Provider(e.to_string())),
        PrivilegedAction::RemoveUnusedFlatpakRuntimes { .. } => Err(ExecutionError::Provider(
            "flatpak handled separately".into(),
        )),
    }
}
