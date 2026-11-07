#![forbid(unsafe_code)]

use crate::database::queries;
use crate::engine::{paths, rules as rule_engine};
use crate::errors::AppError;
use crate::models::{
    CreateRuleRequest, ReorderRulesRequest, Rule, RulesListResponse, UpdateRuleRequest,
};
use crate::services::state::AppState;

/// Lists all sorting rules alongside non-blocking extension overlap warnings.
pub fn list_rules(state: &AppState) -> Result<RulesListResponse, AppError> {
    let rules = state.db.with_writer(|conn| queries::list_rules(conn))?;
    let overlaps = rule_engine::detect_extension_overlaps(&rules);
    Ok(RulesListResponse { rules, overlaps })
}

/// Validates and creates a custom sorting rule.
pub fn create_rule(state: &AppState, req: CreateRuleRequest) -> Result<Rule, AppError> {
    let name = rule_engine::validate_rule_name(&req.name)?;
    let extensions = rule_engine::normalize_and_validate_extensions(&req.extensions)?;
    let name_contains = rule_engine::validate_name_contains(req.name_contains.as_deref())?;
    rule_engine::validate_size_bounds(req.min_size_bytes, req.max_size_bytes)?;
    rule_engine::validate_min_age_days(req.min_age_days)?;

    let watched_folders = state
        .db
        .with_writer(|conn| queries::list_watched_folders(conn))?;

    let destination = paths::validate_rule_destination(
        &req.destination,
        &state.protected_roots,
        &watched_folders,
        state.fs.as_ref(),
    )?;

    state.db.with_writer(|conn| {
        queries::insert_rule(
            conn,
            &name,
            &destination,
            &extensions,
            req.enabled,
            name_contains.as_deref(),
            req.min_size_bytes,
            req.max_size_bytes,
            req.min_age_days,
        )
    })
}

/// Validates and updates an existing sorting rule.
pub fn update_rule(state: &AppState, req: UpdateRuleRequest) -> Result<Rule, AppError> {
    if req.id <= 0 {
        return Err(AppError::InvalidInput {
            message: "rule id must be positive".to_owned(),
        });
    }
    let name = rule_engine::validate_rule_name(&req.name)?;
    let extensions = rule_engine::normalize_and_validate_extensions(&req.extensions)?;
    let name_contains = rule_engine::validate_name_contains(req.name_contains.as_deref())?;
    rule_engine::validate_size_bounds(req.min_size_bytes, req.max_size_bytes)?;
    rule_engine::validate_min_age_days(req.min_age_days)?;

    let watched_folders = state
        .db
        .with_writer(|conn| queries::list_watched_folders(conn))?;

    let destination = paths::validate_rule_destination(
        &req.destination,
        &state.protected_roots,
        &watched_folders,
        state.fs.as_ref(),
    )?;

    state.db.with_writer(|conn| {
        queries::update_rule(
            conn,
            req.id,
            &name,
            &destination,
            &extensions,
            req.enabled,
            name_contains.as_deref(),
            req.min_size_bytes,
            req.max_size_bytes,
            req.min_age_days,
        )
    })
}

/// Deletes a rule and densifies remaining priorities.
pub fn delete_rule(state: &AppState, id: i64) -> Result<(), AppError> {
    if id <= 0 {
        return Err(AppError::InvalidInput {
            message: "rule id must be positive".to_owned(),
        });
    }
    state.db.with_writer(|conn| queries::delete_rule(conn, id))
}

/// Reorders all rules densely according to the provided ordered ID slice.
pub fn reorder_rules(
    state: &AppState,
    req: ReorderRulesRequest,
) -> Result<RulesListResponse, AppError> {
    let rules = state
        .db
        .with_writer(|conn| queries::reorder_rules(conn, &req.ordered_rule_ids))?;
    let overlaps = rule_engine::detect_extension_overlaps(&rules);
    Ok(RulesListResponse { rules, overlaps })
}

/// Restores missing or edited default rules without touching custom rules.
pub fn restore_default_rules(state: &AppState) -> Result<RulesListResponse, AppError> {
    let rules = state.db.with_writer(queries::restore_default_rules)?;
    let overlaps = rule_engine::detect_extension_overlaps(&rules);
    Ok(RulesListResponse { rules, overlaps })
}
