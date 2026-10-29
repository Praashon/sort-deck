#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap, HashSet};

use uuid::Uuid;

use crate::engine::naming::NameReservationTable;
use crate::engine::paths::{
    is_project_root, validate_rule_destination, NormalizedPath, ProtectedRoots,
};
use crate::engine::rules::{
    expand_destination_template, match_rule_with_context, RuleMatchOutcome,
};
use crate::engine::scanner::{
    classify_file_safety, resolve_destination_dir, scan_watched_folder, ScanConfig,
};
use crate::errors::AppError;
use crate::fsops::{CancelToken, FileFingerprint, FsBackend, VolumeInfo};
use crate::models::{
    DestinationGroupSummary, OperationRevalidationFailure, OperationStatus, PlanPageRequest,
    PlanPageResponse, PlanRevalidationReport, PlanSummary, PlannedOperation, Rule, SkipReason,
    SkipReasonCount, SourceFingerprintDto, WatchedFolder,
};

/// Default expiry window for dry-run plans (`AGENTS.md` §7.4).
pub const DEFAULT_PLAN_TTL_MS: i64 = 10 * 60 * 1000;

/// Maximum file size representable on FAT32 (`4 GiB - 1`).
pub const FAT32_MAX_FILE_SIZE: u64 = 4_294_967_295;

const MIN_CROSS_VOLUME_MARGIN_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PAGE_LIMIT: u32 = 500;

/// Backend-owned dry-run plan held in memory until executed, discarded, or expired.
#[derive(Debug, Clone)]
pub struct StoredPlan {
    pub summary: PlanSummary,
    pub folder_path_key: String,
    pub operations: Vec<PlannedOperation>,
}

/// Generates a strictly read-only dry-run plan for `folder` (`AGENTS.md` §7).
#[allow(clippy::too_many_arguments)]
pub fn build_dry_run_plan(
    folder: &WatchedFolder,
    rules: &[Rule],
    config_revision: u64,
    roots: &ProtectedRoots,
    fs: &dyn FsBackend,
    cancel: &CancelToken,
    scan_config: ScanConfig,
    now_ms: i64,
    ttl_ms: i64,
) -> Result<StoredPlan, AppError> {
    let scan = scan_watched_folder(folder, rules, roots, fs, cancel, scan_config)?;
    let plan_id = Uuid::new_v4().to_string();

    let source_volume = fs.volume_info(&scan.folder_norm.path).ok();
    let mut dest_volume_cache: HashMap<String, Option<VolumeInfo>> = HashMap::new();
    let mut reserved_dest_bytes: HashMap<String, u64> = HashMap::new();
    let mut validated_dest_cache: HashMap<String, Result<NormalizedPath, SkipReason>> =
        HashMap::new();
    let mut name_table = NameReservationTable::new();

    let mut operations = Vec::with_capacity(scan.files.len());
    let mut skip_counts: BTreeMap<SkipReason, u64> = BTreeMap::new();
    let mut dest_groups: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut eligible_count: u64 = 0;
    let mut conflict_renamed_count: u64 = 0;
    let mut skipped_count: u64 = 0;
    let mut total_bytes_to_move: u64 = 0;
    let mut cross_volume_count: u64 = 0;
    let mut plan_warnings = scan.warnings;

    let folder_slice = std::slice::from_ref(folder);

    for (idx, scanned) in scan.files.into_iter().enumerate() {
        if (idx + 1) % 256 == 0 && cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }

        let op_id = format!("op-{:06}", idx + 1);
        let fp_dto = fingerprint_to_dto(&scanned.fingerprint);

        if let Some(reason) = scanned.pre_skip_reason {
            *skip_counts.entry(reason).or_insert(0) += 1;
            skipped_count += 1;
            operations.push(PlannedOperation {
                operation_id: op_id,
                source_path: scanned.path_display,
                destination_path: None,
                original_filename: scanned.file_name,
                proposed_filename: None,
                file_size: scanned.size,
                source_fingerprint: fp_dto,
                matched_rule_id: None,
                matched_rule_name: None,
                operation_status: OperationStatus::Skipped,
                skip_reason: Some(reason),
                cross_volume_estimate: false,
                warnings: Vec::new(),
            });
            continue;
        }

        let matched_rule = match match_rule_with_context(
            &scanned.file_name,
            scanned.size,
            scanned.fingerprint.modified_epoch_ms,
            now_ms,
            rules,
        ) {
            RuleMatchOutcome::Matched(rule) => rule,
            RuleMatchOutcome::MatchedDisabledOnly(rule) => {
                *skip_counts.entry(SkipReason::RuleDisabled).or_insert(0) += 1;
                skipped_count += 1;
                operations.push(PlannedOperation {
                    operation_id: op_id,
                    source_path: scanned.path_display,
                    destination_path: None,
                    original_filename: scanned.file_name,
                    proposed_filename: None,
                    file_size: scanned.size,
                    source_fingerprint: fp_dto,
                    matched_rule_id: Some(rule.id),
                    matched_rule_name: Some(rule.name.clone()),
                    operation_status: OperationStatus::Skipped,
                    skip_reason: Some(SkipReason::RuleDisabled),
                    cross_volume_estimate: false,
                    warnings: Vec::new(),
                });
                continue;
            }
            RuleMatchOutcome::Unmatched => {
                *skip_counts.entry(SkipReason::NoMatchingRule).or_insert(0) += 1;
                skipped_count += 1;
                operations.push(PlannedOperation {
                    operation_id: op_id,
                    source_path: scanned.path_display,
                    destination_path: None,
                    original_filename: scanned.file_name,
                    proposed_filename: None,
                    file_size: scanned.size,
                    source_fingerprint: fp_dto,
                    matched_rule_id: None,
                    matched_rule_name: None,
                    operation_status: OperationStatus::Skipped,
                    skip_reason: Some(SkipReason::NoMatchingRule),
                    cross_volume_estimate: false,
                    warnings: Vec::new(),
                });
                continue;
            }
        };

        let expanded_dest = expand_destination_template(
            &matched_rule.destination,
            scanned.fingerprint.modified_epoch_ms,
            now_ms,
        );
        let dest_dir_norm = match validate_or_get_cached_dest(
            &scan.folder_norm,
            &expanded_dest,
            roots,
            folder_slice,
            fs,
            &mut validated_dest_cache,
        ) {
            Ok(norm) => norm,
            Err(reason) => {
                *skip_counts.entry(reason).or_insert(0) += 1;
                skipped_count += 1;
                operations.push(PlannedOperation {
                    operation_id: op_id,
                    source_path: scanned.path_display,
                    destination_path: None,
                    original_filename: scanned.file_name,
                    proposed_filename: None,
                    file_size: scanned.size,
                    source_fingerprint: fp_dto,
                    matched_rule_id: Some(matched_rule.id),
                    matched_rule_name: Some(matched_rule.name.clone()),
                    operation_status: OperationStatus::Skipped,
                    skip_reason: Some(reason),
                    cross_volume_estimate: false,
                    warnings: Vec::new(),
                });
                continue;
            }
        };

        // Already inside the matched destination folder -> idempotent skip (INV-12).
        if scanned.parent_key == dest_dir_norm.key {
            *skip_counts
                .entry(SkipReason::AlreadyInDestination)
                .or_insert(0) += 1;
            skipped_count += 1;
            operations.push(PlannedOperation {
                operation_id: op_id,
                source_path: scanned.path_display,
                destination_path: None,
                original_filename: scanned.file_name,
                proposed_filename: None,
                file_size: scanned.size,
                source_fingerprint: fp_dto,
                matched_rule_id: Some(matched_rule.id),
                matched_rule_name: Some(matched_rule.name.clone()),
                operation_status: OperationStatus::Skipped,
                skip_reason: Some(SkipReason::AlreadyInDestination),
                cross_volume_estimate: false,
                warnings: Vec::new(),
            });
            continue;
        }

        let dest_vol_opt = dest_volume_cache
            .entry(dest_dir_norm.key.clone())
            .or_insert_with(|| lookup_destination_volume_info(&dest_dir_norm.path, fs))
            .clone();

        let cross_volume_estimate = estimate_cross_volume(
            &scan.folder_norm,
            source_volume.as_ref(),
            &dest_dir_norm,
            dest_vol_opt.as_ref(),
        );

        let mut op_warnings = Vec::new();
        if cross_volume_estimate {
            if let Some(reason) = check_cross_volume_eligibility(
                scanned.attributes,
                scanned.size,
                dest_vol_opt.as_ref(),
                &dest_dir_norm,
                &mut reserved_dest_bytes,
                &mut op_warnings,
            ) {
                *skip_counts.entry(reason).or_insert(0) += 1;
                skipped_count += 1;
                operations.push(PlannedOperation {
                    operation_id: op_id,
                    source_path: scanned.path_display,
                    destination_path: None,
                    original_filename: scanned.file_name,
                    proposed_filename: None,
                    file_size: scanned.size,
                    source_fingerprint: fp_dto,
                    matched_rule_id: Some(matched_rule.id),
                    matched_rule_name: Some(matched_rule.name.clone()),
                    operation_status: OperationStatus::Skipped,
                    skip_reason: Some(reason),
                    cross_volume_estimate: true,
                    warnings: op_warnings,
                });
                continue;
            }
        }

        let reserved = match name_table.reserve_name(&dest_dir_norm, &scanned.file_name, fs) {
            Ok(res) => res,
            Err(reason) => {
                *skip_counts.entry(reason).or_insert(0) += 1;
                skipped_count += 1;
                operations.push(PlannedOperation {
                    operation_id: op_id,
                    source_path: scanned.path_display,
                    destination_path: None,
                    original_filename: scanned.file_name,
                    proposed_filename: None,
                    file_size: scanned.size,
                    source_fingerprint: fp_dto,
                    matched_rule_id: Some(matched_rule.id),
                    matched_rule_name: Some(matched_rule.name.clone()),
                    operation_status: OperationStatus::Skipped,
                    skip_reason: Some(reason),
                    cross_volume_estimate,
                    warnings: op_warnings,
                });
                continue;
            }
        };

        let status = if reserved.was_renamed {
            conflict_renamed_count += 1;
            OperationStatus::ConflictRenamed
        } else {
            eligible_count += 1;
            OperationStatus::Eligible
        };

        total_bytes_to_move = total_bytes_to_move.saturating_add(scanned.size);
        if cross_volume_estimate {
            cross_volume_count += 1;
        }

        let group_entry = dest_groups
            .entry(dest_dir_norm.display.clone())
            .or_insert((0, 0));
        group_entry.0 += 1;
        group_entry.1 = group_entry.1.saturating_add(scanned.size);

        let full_dest = dest_dir_norm.path.join(&reserved.filename);
        let full_dest_str = full_dest.to_string_lossy().into_owned();

        operations.push(PlannedOperation {
            operation_id: op_id,
            source_path: scanned.path_display,
            destination_path: Some(full_dest_str),
            original_filename: scanned.file_name,
            proposed_filename: Some(reserved.filename),
            file_size: scanned.size,
            source_fingerprint: fp_dto,
            matched_rule_id: Some(matched_rule.id),
            matched_rule_name: Some(matched_rule.name.clone()),
            operation_status: status,
            skip_reason: None,
            cross_volume_estimate,
            warnings: op_warnings,
        });
    }

    if eligible_count == 0 && conflict_renamed_count == 0 && !scan.sortignore_detected {
        plan_warnings.push("No eligible files were found for sorting.".to_owned());
    }

    let skip_reason_counts = skip_counts
        .into_iter()
        .map(|(reason, count)| SkipReasonCount { reason, count })
        .collect();

    let destination_groups = dest_groups
        .into_iter()
        .map(
            |(destination_dir, (file_count, total_bytes))| DestinationGroupSummary {
                destination_dir,
                file_count,
                total_bytes,
            },
        )
        .collect();

    let summary = PlanSummary {
        plan_id,
        folder_id: folder.id,
        folder_path: scan.folder_norm.display,
        created_at: now_ms,
        expires_at: now_ms.saturating_add(ttl_ms),
        config_revision,
        total_discovered: scan.total_discovered,
        eligible_count,
        conflict_renamed_count,
        skipped_count,
        total_bytes_to_move,
        cross_volume_count,
        skip_reason_counts,
        destination_groups,
        warnings: plan_warnings,
        sortignore_detected: scan.sortignore_detected,
    };

    Ok(StoredPlan {
        summary,
        folder_path_key: scan.folder_norm.key,
        operations,
    })
}

/// Pages and filters operations from a backend-stored plan (`AGENTS.md` §7.3).
#[must_use]
pub fn page_plan(plan: &StoredPlan, req: &PlanPageRequest) -> PlanPageResponse {
    let limit = req.limit.clamp(1, MAX_PAGE_LIMIT);
    let offset = usize::try_from(req.offset).unwrap_or(usize::MAX);
    let search_lower = req
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase);

    let filtered: Vec<&PlannedOperation> = plan
        .operations
        .iter()
        .filter(|op| {
            if let Some(status) = req.status_filter {
                if op.operation_status != status {
                    return false;
                }
            }
            if let Some(reason) = req.skip_reason_filter {
                if op.skip_reason != Some(reason) {
                    return false;
                }
            }
            if let Some(ref query) = search_lower {
                let in_name = op.original_filename.to_lowercase().contains(query);
                let in_src = op.source_path.to_lowercase().contains(query);
                let in_dst = op
                    .destination_path
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(query));
                if !in_name && !in_src && !in_dst {
                    return false;
                }
            }
            true
        })
        .collect();

    let total_matching = u64::try_from(filtered.len()).unwrap_or(u64::MAX);
    let items = filtered
        .into_iter()
        .skip(offset)
        .take(limit as usize)
        .cloned()
        .collect();

    PlanPageResponse {
        plan_id: plan.summary.plan_id.clone(),
        total_matching,
        offset: req.offset,
        limit,
        items,
    }
}

/// Revalidates every selected executable operation in `plan` before execution (INV-11, §7.4).
#[allow(clippy::too_many_arguments)]
pub fn revalidate_plan(
    plan: &StoredPlan,
    deselected_operation_ids: &[String],
    current_config_revision: u64,
    folder: &WatchedFolder,
    rules: &[Rule],
    roots: &ProtectedRoots,
    fs: &dyn FsBackend,
    now_ms: i64,
) -> Result<PlanRevalidationReport, AppError> {
    if now_ms > plan.summary.expires_at {
        return Err(AppError::PlanExpired {
            plan_id: plan.summary.plan_id.clone(),
        });
    }

    if plan.summary.config_revision != current_config_revision {
        return Err(AppError::PlanInvalidated {
            plan_id: plan.summary.plan_id.clone(),
            reason: "configuration revision changed after preview".to_owned(),
        });
    }

    if !folder.enabled {
        return Err(AppError::PlanInvalidated {
            plan_id: plan.summary.plan_id.clone(),
            reason: "watched folder is disabled".to_owned(),
        });
    }

    let deselected: HashSet<&str> = deselected_operation_ids
        .iter()
        .map(String::as_str)
        .collect();
    let folder_norm = crate::engine::paths::normalize_absolute_path(&folder.path)?;
    let folder_slice = std::slice::from_ref(folder);
    let mut validated_dest_cache: HashMap<String, Result<NormalizedPath, SkipReason>> =
        HashMap::new();
    let empty_locks = HashSet::new();

    let mut valid_operation_ids = Vec::new();
    let mut rejected = Vec::new();

    for op in &plan.operations {
        if !matches!(
            op.operation_status,
            OperationStatus::Eligible | OperationStatus::ConflictRenamed
        ) {
            continue;
        }
        if deselected.contains(op.operation_id.as_str()) {
            continue;
        }

        let src_path = std::path::Path::new(&op.source_path);
        if !fs.exists(src_path).unwrap_or(false) {
            rejected.push(OperationRevalidationFailure {
                operation_id: op.operation_id.clone(),
                reason: SkipReason::SourceVanished,
                detail: "source file no longer exists".to_owned(),
            });
            continue;
        }

        let current_fp = match fs.fingerprint(src_path) {
            Ok(fp) => fp,
            Err(_) => {
                rejected.push(OperationRevalidationFailure {
                    operation_id: op.operation_id.clone(),
                    reason: SkipReason::PermissionDenied,
                    detail: "unable to inspect current source fingerprint".to_owned(),
                });
                continue;
            }
        };

        if !fingerprints_match(&op.source_fingerprint, &current_fp) {
            rejected.push(OperationRevalidationFailure {
                operation_id: op.operation_id.clone(),
                reason: SkipReason::SourceChanged,
                detail: "source file changed since plan creation".to_owned(),
            });
            continue;
        }

        let attrs = match fs.attributes(src_path) {
            Ok(a) => a,
            Err(_) => {
                rejected.push(OperationRevalidationFailure {
                    operation_id: op.operation_id.clone(),
                    reason: SkipReason::PermissionDenied,
                    detail: "unable to inspect current source attributes".to_owned(),
                });
                continue;
            }
        };

        let src_norm = match crate::engine::paths::normalize_absolute_path(&op.source_path) {
            Ok(n) => n,
            Err(_) => {
                rejected.push(OperationRevalidationFailure {
                    operation_id: op.operation_id.clone(),
                    reason: SkipReason::InvalidName,
                    detail: "source path failed validation".to_owned(),
                });
                continue;
            }
        };

        if let Some(reason) =
            classify_file_safety(&src_norm, &op.original_filename, attrs, &empty_locks, roots)
        {
            rejected.push(OperationRevalidationFailure {
                operation_id: op.operation_id.clone(),
                reason,
                detail: "source file failed safety revalidation".to_owned(),
            });
            continue;
        }

        let expected_rule_id = op.matched_rule_id;
        let current_match = match match_rule_with_context(
            &op.original_filename,
            current_fp.size,
            current_fp.modified_epoch_ms,
            now_ms,
            rules,
        ) {
            RuleMatchOutcome::Matched(rule) if Some(rule.id) == expected_rule_id => rule,
            RuleMatchOutcome::MatchedDisabledOnly(_) => {
                rejected.push(OperationRevalidationFailure {
                    operation_id: op.operation_id.clone(),
                    reason: SkipReason::RuleDisabled,
                    detail: "matched rule is now disabled".to_owned(),
                });
                continue;
            }
            _ => {
                rejected.push(OperationRevalidationFailure {
                    operation_id: op.operation_id.clone(),
                    reason: SkipReason::NoMatchingRule,
                    detail: "rule match changed since plan creation".to_owned(),
                });
                continue;
            }
        };

        let expanded_dest = expand_destination_template(
            &current_match.destination,
            current_fp.modified_epoch_ms,
            now_ms,
        );
        if let Err(reason) = validate_or_get_cached_dest(
            &folder_norm,
            &expanded_dest,
            roots,
            folder_slice,
            fs,
            &mut validated_dest_cache,
        ) {
            rejected.push(OperationRevalidationFailure {
                operation_id: op.operation_id.clone(),
                reason,
                detail: "destination directory failed safety revalidation".to_owned(),
            });
            continue;
        }

        valid_operation_ids.push(op.operation_id.clone());
    }

    Ok(PlanRevalidationReport {
        plan_id: plan.summary.plan_id.clone(),
        valid_operation_ids,
        rejected,
    })
}

fn validate_or_get_cached_dest(
    folder_norm: &NormalizedPath,
    raw_dest: &str,
    roots: &ProtectedRoots,
    folders: &[WatchedFolder],
    fs: &dyn FsBackend,
    cache: &mut HashMap<String, Result<NormalizedPath, SkipReason>>,
) -> Result<NormalizedPath, SkipReason> {
    if let Some(cached) = cache.get(raw_dest) {
        return cached.clone();
    }

    let result = (|| {
        validate_rule_destination(raw_dest, roots, folders, fs)
            .map_err(|_| SkipReason::DestinationInvalid)?;
        let dest_norm =
            resolve_destination_dir(folder_norm, raw_dest).ok_or(SkipReason::DestinationInvalid)?;

        if fs.exists(&dest_norm.path).unwrap_or(false) {
            let attrs = fs
                .attributes(&dest_norm.path)
                .map_err(|_| SkipReason::DestinationUnavailable)?;
            if attrs.is_reparse_point || !attrs.is_dir {
                return Err(SkipReason::DestinationInvalid);
            }
            if is_project_root(&dest_norm.path, fs).unwrap_or(false) {
                return Err(SkipReason::DestinationInvalid);
            }
        }

        if fs
            .exists(&dest_norm.path.join(".sortignore"))
            .unwrap_or(false)
        {
            return Err(SkipReason::DestinationInvalid);
        }

        Ok(dest_norm)
    })();

    cache.insert(raw_dest.to_owned(), result.clone());
    result
}

fn lookup_destination_volume_info(dest_dir: &Path, fs: &dyn FsBackend) -> Option<VolumeInfo> {
    let mut current = Some(dest_dir);
    while let Some(candidate) = current {
        if fs.exists(candidate).unwrap_or(false) {
            return fs.volume_info(candidate).ok();
        }
        current = candidate.parent();
    }
    fs.volume_info(dest_dir).ok()
}

fn estimate_cross_volume(
    src_norm: &NormalizedPath,
    src_vol: Option<&VolumeInfo>,
    dst_norm: &NormalizedPath,
    dst_vol: Option<&VolumeInfo>,
) -> bool {
    if let (Some(s), Some(d)) = (src_vol, dst_vol) {
        if let (Some(s_ser), Some(d_ser)) = (s.volume_serial, d.volume_serial) {
            return s_ser != d_ser;
        }
    }

    volume_prefix_key(&src_norm.key) != volume_prefix_key(&dst_norm.key)
}

fn volume_prefix_key(key: &str) -> &str {
    if let Some(rest) = key.strip_prefix("//") {
        // UNC: //server/share
        let mut slashes = 0usize;
        for (idx, ch) in rest.char_indices() {
            if ch == '/' {
                slashes += 1;
                if slashes == 2 {
                    return &key[..idx + 2];
                }
            }
        }
        key
    } else {
        key.get(..3).unwrap_or(key)
    }
}

fn check_cross_volume_eligibility(
    attrs: crate::fsops::FileAttributes,
    file_size: u64,
    dest_vol: Option<&VolumeInfo>,
    dest_dir_norm: &NormalizedPath,
    reserved_bytes: &mut HashMap<String, u64>,
    op_warnings: &mut Vec<String>,
) -> Option<SkipReason> {
    if attrs.is_readonly {
        return Some(SkipReason::ReadOnlyCrossVolume);
    }
    if attrs.is_cloud_managed {
        return Some(SkipReason::CloudManagedCrossVolume);
    }
    if attrs.is_encrypted {
        op_warnings.push("Encrypted (EFS) files are skipped for cross-volume moves.".to_owned());
        return Some(SkipReason::DestinationInvalid);
    }

    if let Some(vol) = dest_vol {
        let fs_upper = vol.filesystem_name.to_ascii_uppercase();
        if fs_upper == "FAT32" && file_size > FAT32_MAX_FILE_SIZE {
            return Some(SkipReason::FilesystemLimit);
        }
        if matches!(fs_upper.as_str(), "FAT32" | "EXFAT" | "FAT") && !vol.supports_ads {
            op_warnings.push(
                "Destination filesystem does not support Alternate Data Streams (Zone.Identifier)."
                    .to_owned(),
            );
        }
        if let Some(avail) = vol.available_bytes {
            let vol_key = volume_prefix_key(&dest_dir_norm.key).to_owned();
            let already_reserved = reserved_bytes.get(&vol_key).copied().unwrap_or(0);
            let margin = MIN_CROSS_VOLUME_MARGIN_BYTES.max(file_size / 100);
            let needed = already_reserved
                .saturating_add(file_size)
                .saturating_add(margin);
            if avail < needed {
                return Some(SkipReason::InsufficientSpace);
            }
            reserved_bytes.insert(vol_key, already_reserved.saturating_add(file_size));
        }
    }

    None
}

fn fingerprint_to_dto(fp: &FileFingerprint) -> SourceFingerprintDto {
    SourceFingerprintDto {
        volume_serial: fp.volume_serial,
        file_id: fp.file_id.clone(),
        size: fp.size,
        modified_epoch_ms: fp.modified_epoch_ms,
    }
}

fn fingerprints_match(planned: &SourceFingerprintDto, current: &FileFingerprint) -> bool {
    planned.size == current.size
        && planned.modified_epoch_ms == current.modified_epoch_ms
        && planned.volume_serial == current.volume_serial
        && planned.file_id == current.file_id
}

use std::path::Path;
