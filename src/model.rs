use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CleanupRisk {
    Safe,
    Review,
    Advanced,
    Dangerous,
}

impl CleanupRisk {
    pub fn label(self) -> &'static str {
        match self {
            Self::Safe => "SAFE",
            Self::Review => "REVIEW",
            Self::Advanced => "ADVANCED",
            Self::Dangerous => "DANGEROUS",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::Safe => "risk-safe",
            Self::Review => "risk-review",
            Self::Advanced => "risk-advanced",
            Self::Dangerous => "risk-dangerous",
        }
    }
}

impl fmt::Display for CleanupRisk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CleanupCategory {
    UserCache,
    ThumbnailCache,
    Trash,
    AptCache,
    SystemJournal,
    SnapRevision,
    FlatpakRuntime,
    CrashReports,
    DeveloperCache,
}

impl CleanupCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::UserCache => "Application cache",
            Self::ThumbnailCache => "Thumbnail cache",
            Self::Trash => "Trash",
            Self::AptCache => "APT cache",
            Self::SystemJournal => "System journal",
            Self::SnapRevision => "Old Snap revision",
            Self::FlatpakRuntime => "Flatpak runtime",
            Self::CrashReports => "System crash reports",
            Self::DeveloperCache => "Developer package cache",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegedAction {
    CleanAptCache,
    AutoremovePackages,
    RemoveDisabledSnapRevision { name: String, revision: String },
    VacuumJournal { keep_days: u32 },
    RemoveUnusedFlatpakRuntimes { user_scope: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionKind {
    /// Can be cleaned without privilege, but only through a strict allowlist.
    UserAllowedDirectory,
    /// Requires a Polkit-authorized helper method.
    PrivilegedProvider,
    /// Cleaned via unprivileged or CLI provider (e.g. Flatpak unused runtimes).
    FlatpakUnused,
    /// Informational / review-only in this release.
    AnalysisOnly,
}

#[derive(Debug, Clone)]
pub struct CleanupCandidate {
    pub id: String,
    pub category: CleanupCategory,
    pub title: String,
    pub description: String,
    pub path: Option<PathBuf>,
    pub size_bytes: u64,
    pub risk: CleanupRisk,
    pub reason: String,
    pub consequence: String,
    pub requires_privilege: bool,
    pub execution: ExecutionKind,
    pub privileged_action: Option<PrivilegedAction>,
}

impl CleanupCandidate {
    pub fn executable_now(&self) -> bool {
        match self.execution {
            ExecutionKind::UserAllowedDirectory => true,
            ExecutionKind::PrivilegedProvider => self.privileged_action.is_some(),
            ExecutionKind::FlatpakUnused => true,
            ExecutionKind::AnalysisOnly => false,
        }
    }

    pub fn selected_by_default(&self) -> bool {
        self.risk == CleanupRisk::Safe && self.executable_now()
    }

    pub fn reclaimable_now(&self) -> bool {
        self.selected_by_default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScannerResult {
    pub module: String,
    pub candidates: Vec<CleanupCandidate>,
    pub files_scanned: u64,
    pub directories_scanned: u64,
    pub duration_ms: u128,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupStatus {
    Success,
    Failed,
    Skipped,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupOperationResult {
    pub candidate_id: String,
    pub title: String,
    pub category: CleanupCategory,
    pub status: CleanupStatus,
    pub estimated_bytes: u64,
    /// Bytes actually released from the filesystem during this operation.
    pub recovered_bytes: u64,
    /// Bytes moved into Safety Quarantine. These remain allocated until purge.
    #[serde(default)]
    pub quarantined_bytes: u64,
    /// Quarantine transaction that can be restored while its data remains available.
    #[serde(default)]
    pub undo_id: Option<String>,
    pub requires_privilege: bool,
    pub message: String,
}
