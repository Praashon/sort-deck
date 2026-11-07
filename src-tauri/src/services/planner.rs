#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::database::queries;
use crate::engine::planner::{self, StoredPlan, DEFAULT_PLAN_TTL_MS};
use crate::engine::rules::{match_rule_with_context, RuleMatchOutcome};
use crate::engine::scanner::{self, ScanConfig, ScannedFile};
use crate::errors::AppError;
use crate::fsops::CancelToken;
use crate::models::{
    DuplicateGroup, FolderStorageReport, LargeFileEntry, PlanPageRequest, PlanPageResponse,
    PlanRevalidationReport, PlanSummary, StorageCategoryBreakdown,
};
use crate::services::state::AppState;

const MAX_STORED_PLANS: usize = 16;

/// Bounded in-memory registry of backend-owned dry-run plans (`AGENTS.md` §7.3, §7.4).
#[derive(Debug, Default)]
pub struct PlanStore {
    plans: HashMap<String, StoredPlan>,
    insertion_order: VecDeque<String>,
}

impl PlanStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, plan: StoredPlan, now_ms: i64) {
        self.prune_expired(now_ms);
        while self.plans.len() >= MAX_STORED_PLANS {
            if let Some(oldest_id) = self.insertion_order.pop_front() {
                self.plans.remove(&oldest_id);
            } else {
                break;
            }
        }
        let id = plan.summary.plan_id.clone();
        self.insertion_order.push_back(id.clone());
        self.plans.insert(id, plan);
    }

    #[must_use]
    pub fn get(&self, plan_id: &str) -> Option<&StoredPlan> {
        self.plans.get(plan_id)
    }

    pub fn remove(&mut self, plan_id: &str) -> Option<StoredPlan> {
        self.insertion_order.retain(|id| id != plan_id);
        self.plans.remove(plan_id)
    }

    fn prune_expired(&mut self, now_ms: i64) {
        self.plans
            .retain(|_, plan| now_ms <= plan.summary.expires_at);
        let plans = &self.plans;
        self.insertion_order.retain(|id| plans.contains_key(id));
    }
}

/// Scans `folder_id` in read-only mode, produces a dry-run plan, stores it in
/// backend memory, and returns the summary (`AGENTS.md` §7.3).
pub fn create_dry_run_plan(state: &AppState, folder_id: i64) -> Result<PlanSummary, AppError> {
    if folder_id <= 0 {
        return Err(AppError::InvalidInput {
            message: "watched folder id must be positive".to_owned(),
        });
    }

    // Read configuration and release the SQLite writer lock before scanning (§5.1).
    let (folder, rules, config_revision) = state.db.with_writer(|conn| {
        let folder = queries::get_watched_folder(conn, folder_id)?;
        let rules = queries::list_rules(conn)?;
        let rev = queries::get_config_revision(conn)?;
        Ok((folder, rules, rev))
    })?;

    if !folder.enabled {
        return Err(AppError::InvalidInput {
            message: format!("watched folder '{}' is disabled", folder.path),
        });
    }

    let now_ms = queries::now_epoch_ms();
    let cancel = CancelToken::new();
    let stored = planner::build_dry_run_plan(
        &folder,
        &rules,
        config_revision,
        &state.protected_roots,
        state.fs.as_ref(),
        &cancel,
        ScanConfig::default(),
        now_ms,
        DEFAULT_PLAN_TTL_MS,
    )?;

    let summary = stored.summary.clone();
    let mut store = state.plans.lock().map_err(|_| AppError::Internal {
        message: "plan store mutex poisoned".to_owned(),
    })?;
    store.insert(stored, now_ms);

    Ok(summary)
}

/// Returns a filtered, paginated slice of operations for `req.plan_id` (§7.3).
pub fn get_plan_page(state: &AppState, req: PlanPageRequest) -> Result<PlanPageResponse, AppError> {
    if req.plan_id.trim().is_empty() {
        return Err(AppError::InvalidInput {
            message: "planId must not be empty".to_owned(),
        });
    }

    let current_rev = state
        .db
        .with_writer(|conn| queries::get_config_revision(conn))?;
    let now_ms = queries::now_epoch_ms();

    let store = state.plans.lock().map_err(|_| AppError::Internal {
        message: "plan store mutex poisoned".to_owned(),
    })?;

    let plan = store.get(&req.plan_id).ok_or_else(|| AppError::NotFound {
        resource: format!("plan {}", req.plan_id),
        os_error_code: None,
    })?;

    if now_ms > plan.summary.expires_at {
        return Err(AppError::PlanExpired {
            plan_id: plan.summary.plan_id.clone(),
        });
    }

    if plan.summary.config_revision != current_rev {
        return Err(AppError::PlanInvalidated {
            plan_id: plan.summary.plan_id.clone(),
            reason: "configuration changed since plan was generated".to_owned(),
        });
    }

    Ok(planner::page_plan(plan, &req))
}

/// Discards a stored dry-run plan from backend memory.
pub fn discard_plan(state: &AppState, plan_id: &str) -> Result<(), AppError> {
    if plan_id.trim().is_empty() {
        return Err(AppError::InvalidInput {
            message: "planId must not be empty".to_owned(),
        });
    }

    let mut store = state.plans.lock().map_err(|_| AppError::Internal {
        message: "plan store mutex poisoned".to_owned(),
    })?;
    store.remove(plan_id);
    Ok(())
}

/// Revalidates a stored plan prior to execution (`AGENTS.md` §7.4, INV-11).
pub fn revalidate_plan(
    state: &AppState,
    plan_id: &str,
    deselected_operation_ids: &[String],
) -> Result<PlanRevalidationReport, AppError> {
    let plan = {
        let store = state.plans.lock().map_err(|_| AppError::Internal {
            message: "plan store mutex poisoned".to_owned(),
        })?;
        store
            .get(plan_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound {
                resource: format!("plan {plan_id}"),
                os_error_code: None,
            })?
    };

    let (folder, rules, current_rev) = state.db.with_writer(|conn| {
        let folder = queries::get_watched_folder(conn, plan.summary.folder_id)?;
        let rules = queries::list_rules(conn)?;
        let rev = queries::get_config_revision(conn)?;
        Ok((folder, rules, rev))
    })?;

    let now_ms = queries::now_epoch_ms();
    planner::revalidate_plan(
        &plan,
        deselected_operation_ids,
        current_rev,
        &folder,
        &rules,
        &state.protected_roots,
        state.fs.as_ref(),
        now_ms,
    )
}

const NINETY_DAYS_MS: i64 = 90 * 86_400_000;
const MAX_DUPLICATE_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_DUPLICATE_CANDIDATES_HASHED: usize = 200;

/// Produces a strictly read-only breakdown of space usage, old untouched files,
/// largest files, and SHA-256 verified duplicate groups inside `folder_id`.
pub fn analyze_folder_storage(
    state: &AppState,
    folder_id: i64,
) -> Result<FolderStorageReport, AppError> {
    if folder_id <= 0 {
        return Err(AppError::InvalidInput {
            message: "watched folder id must be positive".to_owned(),
        });
    }

    let (folder, rules) = state.db.with_writer(|conn| {
        let folder = queries::get_watched_folder(conn, folder_id)?;
        let rules = queries::list_rules(conn)?;
        Ok((folder, rules))
    })?;

    let cancel = CancelToken::new();
    let now_ms = queries::now_epoch_ms();
    let scan = scanner::scan_watched_folder(
        &folder,
        &rules,
        &state.protected_roots,
        state.fs.as_ref(),
        &cancel,
        ScanConfig::default(),
    )?;

    let mut total_files: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut old_files_count: u64 = 0;
    let mut old_files_bytes: u64 = 0;
    let mut cat_map: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut largest_entries: Vec<LargeFileEntry> = Vec::new();
    let mut size_buckets: BTreeMap<u64, Vec<&ScannedFile>> = BTreeMap::new();

    for file in &scan.files {
        if file.pre_skip_reason.is_some() {
            continue;
        }
        total_files = total_files.saturating_add(1);
        total_bytes = total_bytes.saturating_add(file.size);

        if let Some(mod_ms) = file.fingerprint.modified_epoch_ms {
            if now_ms.saturating_sub(mod_ms) >= NINETY_DAYS_MS {
                old_files_count = old_files_count.saturating_add(1);
                old_files_bytes = old_files_bytes.saturating_add(file.size);
            }
        }

        let cat_name = match match_rule_with_context(
            &file.file_name,
            file.size,
            file.fingerprint.modified_epoch_ms,
            now_ms,
            &rules,
        ) {
            RuleMatchOutcome::Matched(rule) | RuleMatchOutcome::MatchedDisabledOnly(rule) => {
                rule.name.clone()
            }
            RuleMatchOutcome::Unmatched => "Other / Unmatched".to_owned(),
        };

        let entry = cat_map.entry(cat_name.clone()).or_insert((0, 0));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = entry.1.saturating_add(file.size);

        largest_entries.push(LargeFileEntry {
            path: file.path_display.clone(),
            file_name: file.file_name.clone(),
            size_bytes: file.size,
            modified_epoch_ms: file.fingerprint.modified_epoch_ms,
            matched_rule_name: if cat_name == "Other / Unmatched" {
                None
            } else {
                Some(cat_name)
            },
        });

        if file.size > 0 && file.size <= MAX_DUPLICATE_FILE_BYTES {
            size_buckets.entry(file.size).or_default().push(file);
        }
    }

    let mut categories: Vec<StorageCategoryBreakdown> = cat_map
        .into_iter()
        .map(|(name, (file_count, cat_bytes))| StorageCategoryBreakdown {
            name,
            file_count,
            total_bytes: cat_bytes,
        })
        .collect();
    categories.sort_by_key(|a| std::cmp::Reverse(a.total_bytes));

    largest_entries.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
    largest_entries.truncate(10);

    let mut duplicate_groups: Vec<DuplicateGroup> = Vec::new();
    let mut hashed_count: usize = 0;

    for (size_bytes, bucket) in size_buckets {
        if bucket.len() < 2 {
            continue;
        }
        let mut by_hash: HashMap<String, Vec<String>> = HashMap::new();
        for file in bucket {
            if hashed_count >= MAX_DUPLICATE_CANDIDATES_HASHED {
                break;
            }
            hashed_count += 1;
            if let Ok(digest) = state.fs.hash_sha256(&file.path, &cancel) {
                by_hash
                    .entry(digest)
                    .or_default()
                    .push(file.path_display.clone());
            }
        }
        for (sha256, paths) in by_hash {
            if paths.len() >= 2 {
                let redundant = u64::try_from(paths.len().saturating_sub(1)).unwrap_or(0);
                duplicate_groups.push(DuplicateGroup {
                    sha256,
                    size_bytes,
                    wasted_bytes: size_bytes.saturating_mul(redundant),
                    paths,
                });
            }
        }
        if hashed_count >= MAX_DUPLICATE_CANDIDATES_HASHED {
            break;
        }
    }
    duplicate_groups.sort_by_key(|a| std::cmp::Reverse(a.wasted_bytes));
    duplicate_groups.truncate(20);

    Ok(FolderStorageReport {
        folder_id: folder.id,
        folder_path: folder.path,
        total_files,
        total_bytes,
        old_files_count,
        old_files_bytes,
        categories,
        largest_files: largest_entries,
        duplicate_groups,
        scanned_at: now_ms,
    })
}
