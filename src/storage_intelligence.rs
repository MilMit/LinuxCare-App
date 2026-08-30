use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const THIRTY_DAYS: u64 = 30 * 24 * 60 * 60;
const MIN_BASELINE_AGE: u64 = 60 * 60;
const MATERIAL_CHANGE_BYTES: i128 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
struct ScanSnapshot {
    timestamp_unix: u64,
    total_reclaimable_bytes: u64,
    categories: BTreeMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct CategoryTrend {
    pub category: String,
    pub current_bytes: u64,
    pub delta_bytes: i128,
}

#[derive(Debug, Clone)]
pub struct StorageIntelligenceReport {
    pub sample_count: usize,
    pub current_reclaimable_bytes: u64,
    pub reclaimable_delta_bytes: i128,
    pub baseline_age_secs: u64,
    pub daily_growth_bytes: Option<i128>,
    pub category_churn_bytes: u64,
    pub confidence_label: &'static str,
    pub top_categories: Vec<CategoryTrend>,
    pub growth_categories: Vec<CategoryTrend>,
    pub summary: String,
}

pub fn collect_report(home: &Path) -> StorageIntelligenceReport {
    let mut history = load_history(home);
    history.sort_by_key(|sample| sample.timestamp_unix);
    let Some(current) = history.last().cloned() else {
        return empty_report();
    };

    let baseline = choose_baseline(&history, &current);
    let mut categories = BTreeSet::new();
    categories.extend(current.categories.keys().cloned());
    if let Some(previous) = &baseline {
        categories.extend(previous.categories.keys().cloned());
    }

    let mut trends: Vec<CategoryTrend> = categories
        .into_iter()
        .map(|category| {
            let now = current.categories.get(&category).copied().unwrap_or(0);
            let before = baseline
                .as_ref()
                .and_then(|sample| sample.categories.get(&category).copied())
                .unwrap_or(now);
            CategoryTrend {
                category,
                current_bytes: now,
                delta_bytes: i128::from(now) - i128::from(before),
            }
        })
        .collect();

    let mut top_categories = trends.clone();
    top_categories.sort_by_key(|item| std::cmp::Reverse(item.current_bytes));
    top_categories.truncate(6);

    let category_churn_bytes = trends
        .iter()
        .map(|item| item.delta_bytes.unsigned_abs())
        .sum::<u128>()
        .min(u128::from(u64::MAX)) as u64;

    trends.retain(|item| item.delta_bytes.unsigned_abs() >= MATERIAL_CHANGE_BYTES as u128);
    trends.sort_by_key(|item| std::cmp::Reverse(item.delta_bytes));
    trends.truncate(6);

    let (delta, baseline_age_secs) = baseline
        .as_ref()
        .map(|previous| {
            (
                i128::from(current.total_reclaimable_bytes)
                    - i128::from(previous.total_reclaimable_bytes),
                current
                    .timestamp_unix
                    .saturating_sub(previous.timestamp_unix),
            )
        })
        .unwrap_or((0, 0));

    let daily_growth_bytes = daily_rate(delta, baseline_age_secs);
    let confidence_label = confidence(history.len(), baseline_age_secs);
    let summary = build_summary(
        baseline.as_ref(),
        delta,
        daily_growth_bytes,
        &trends,
        confidence_label,
    );

    StorageIntelligenceReport {
        sample_count: history.len(),
        current_reclaimable_bytes: current.total_reclaimable_bytes,
        reclaimable_delta_bytes: delta,
        baseline_age_secs,
        daily_growth_bytes,
        category_churn_bytes,
        confidence_label,
        top_categories,
        growth_categories: trends,
        summary,
    }
}

fn choose_baseline(history: &[ScanSnapshot], current: &ScanSnapshot) -> Option<ScanSnapshot> {
    history
        .iter()
        .filter(|sample| sample.timestamp_unix < current.timestamp_unix)
        .find(|sample| {
            let age = current.timestamp_unix.saturating_sub(sample.timestamp_unix);
            (MIN_BASELINE_AGE..=THIRTY_DAYS).contains(&age)
        })
        .cloned()
        .or_else(|| {
            history
                .iter()
                .rev()
                .find(|sample| sample.timestamp_unix < current.timestamp_unix)
                .cloned()
        })
}

fn daily_rate(delta: i128, age_secs: u64) -> Option<i128> {
    if age_secs < MIN_BASELINE_AGE {
        return None;
    }
    let seconds_per_day = 86_400_i128;
    Some(delta.saturating_mul(seconds_per_day) / i128::from(age_secs))
}

fn confidence(sample_count: usize, baseline_age_secs: u64) -> &'static str {
    if sample_count >= 8 && baseline_age_secs >= 7 * 24 * 60 * 60 {
        "HIGH"
    } else if sample_count >= 4 && baseline_age_secs >= 24 * 60 * 60 {
        "MEDIUM"
    } else if baseline_age_secs >= MIN_BASELINE_AGE {
        "LOW"
    } else {
        "LEARNING"
    }
}

fn build_summary(
    baseline: Option<&ScanSnapshot>,
    delta: i128,
    daily_growth: Option<i128>,
    trends: &[CategoryTrend],
    confidence: &str,
) -> String {
    if baseline.is_none() {
        return "One Smart Scan baseline exists. Run another scan later to identify category growth."
            .to_string();
    }
    if delta > 64 * 1024 * 1024 {
        let driver = trends
            .iter()
            .filter(|item| item.delta_bytes > 0)
            .max_by_key(|item| item.delta_bytes)
            .map(|item| item.category.as_str())
            .unwrap_or("multiple categories");
        let rate = daily_growth
            .map(|value| format!(" at about {} per day", format_signed_bytes(value)))
            .unwrap_or_default();
        format!(
            "Safe reclaimable data increased by {}{rate}. Largest visible growth: {driver}. Trend confidence: {confidence}.",
            crate::format::bytes(delta as u64)
        )
    } else if delta < -(64 * 1024 * 1024) {
        format!(
            "Safe reclaimable data decreased by {}. Trend confidence: {confidence}.",
            crate::format::bytes(delta.unsigned_abs() as u64)
        )
    } else {
        format!(
            "Cleanup-category growth is broadly stable against the comparison scan. Trend confidence: {confidence}."
        )
    }
}

fn format_signed_bytes(value: i128) -> String {
    if value > 0 {
        format!("+{}", crate::format::bytes(value as u64))
    } else if value < 0 {
        format!("−{}", crate::format::bytes(value.unsigned_abs() as u64))
    } else {
        "stable".to_string()
    }
}

fn empty_report() -> StorageIntelligenceReport {
    StorageIntelligenceReport {
        sample_count: 0,
        current_reclaimable_bytes: 0,
        reclaimable_delta_bytes: 0,
        baseline_age_secs: 0,
        daily_growth_bytes: None,
        category_churn_bytes: 0,
        confidence_label: "LEARNING",
        top_categories: Vec::new(),
        growth_categories: Vec::new(),
        summary: "Run Smart Scan at least twice to learn which cleanup categories are growing."
            .to_string(),
    }
}

fn load_history(home: &Path) -> Vec<ScanSnapshot> {
    let path = state_root(home).join("scan-snapshots.json");
    fs::read(path)
        .ok()
        .and_then(|payload| serde_json::from_slice(&payload).ok())
        .unwrap_or_default()
}

fn state_root(home: &Path) -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"))
        .join("linuxcare")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_rate_requires_real_time_separation() {
        assert_eq!(daily_rate(1024, 120), None);
        assert_eq!(daily_rate(86_400, 86_400), Some(86_400));
    }

    #[test]
    fn confidence_starts_in_learning_mode() {
        assert_eq!(confidence(1, 0), "LEARNING");
        assert_eq!(confidence(4, 86_400), "MEDIUM");
        assert_eq!(confidence(8, 7 * 86_400), "HIGH");
    }
}
