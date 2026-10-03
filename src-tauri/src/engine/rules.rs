#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashSet};

use crate::errors::AppError;
use crate::models::{ExtensionOverlapWarning, Rule};

const INVALID_EXTENSION_CHARS: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
const MAX_MIN_AGE_DAYS: u32 = 36_500;
const MS_PER_DAY: i64 = 86_400_000;

/// Validates optional filename substring filter (`name_contains`).
pub fn validate_name_contains(raw: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(text) = raw else {
        return Ok(None);
    };
    if text.chars().any(char::is_control) {
        return Err(AppError::InvalidInput {
            message: "filename filter must not contain control characters".to_owned(),
        });
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > 80 {
        return Err(AppError::InvalidInput {
            message: "filename filter must not exceed 80 characters".to_owned(),
        });
    }
    Ok(Some(trimmed.to_owned()))
}

/// Validates optional minimum/maximum byte thresholds.
pub fn validate_size_bounds(min_size: Option<u64>, max_size: Option<u64>) -> Result<(), AppError> {
    if let (Some(min), Some(max)) = (min_size, max_size) {
        if min > max {
            return Err(AppError::InvalidInput {
                message: "minimum file size must not exceed maximum file size".to_owned(),
            });
        }
    }
    Ok(())
}

/// Validates optional minimum file age in days (`<= 36,500`).
pub fn validate_min_age_days(days: Option<u32>) -> Result<(), AppError> {
    if let Some(d) = days {
        if d > MAX_MIN_AGE_DAYS {
            return Err(AppError::InvalidInput {
                message: format!("minimum file age in days must not exceed {MAX_MIN_AGE_DAYS}"),
            });
        }
    }
    Ok(())
}

/// Expands `{year}`, `{month}`, and `{day}` tokens in a destination template using UTC
/// calendar components derived from the file modification timestamp (or `fallback_now_ms`).
#[must_use]
pub fn expand_destination_template(
    template: &str,
    modified_epoch_ms: Option<i64>,
    fallback_now_ms: i64,
) -> String {
    if !template.contains('{') {
        return template.to_owned();
    }
    let ts_ms = modified_epoch_ms
        .filter(|&ms| ms > 0)
        .unwrap_or(fallback_now_ms);
    let days = ts_ms.div_euclid(MS_PER_DAY);
    let (year, month, day) = civil_from_days(days);

    template
        .replace("{year}", &format!("{year:04}"))
        .replace("{month}", &format!("{month:02}"))
        .replace("{day}", &format!("{day:02}"))
}

// Howard Hinnant's civil_from_days algorithm (UTC days since 1970-01-01 -> YYYY-MM-DD).
const fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

/// Validates and trims a rule name.
pub fn validate_rule_name(raw_name: &str) -> Result<String, AppError> {
    if raw_name.chars().any(char::is_control) {
        return Err(AppError::InvalidInput {
            message: "rule name must not contain control characters".to_owned(),
        });
    }

    let trimmed = raw_name.trim();
    let len = trimmed.chars().count();
    if !(1..=80).contains(&len) {
        return Err(AppError::InvalidInput {
            message: "rule name must be between 1 and 80 characters".to_owned(),
        });
    }

    Ok(trimmed.to_owned())
}

/// Normalizes and validates a single extension string.
pub fn normalize_extension(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    let without_dot = trimmed.strip_prefix('.').unwrap_or(trimmed);

    if without_dot.is_empty() {
        return Err(AppError::InvalidInput {
            message: "extension must not be empty".to_owned(),
        });
    }

    if without_dot.starts_with('.') || without_dot.ends_with('.') {
        return Err(AppError::InvalidInput {
            message: format!("invalid extension '{raw}'"),
        });
    }

    if without_dot.chars().any(char::is_whitespace)
        || without_dot.chars().any(char::is_control)
        || without_dot
            .chars()
            .any(|ch| INVALID_EXTENSION_CHARS.contains(&ch))
    {
        return Err(AppError::InvalidInput {
            message: format!("extension '{raw}' contains disallowed characters"),
        });
    }

    let normalized = without_dot.to_lowercase();
    if normalized.len() > 32 {
        return Err(AppError::InvalidInput {
            message: format!("extension '{normalized}' exceeds 32 bytes"),
        });
    }

    Ok(normalized)
}

/// Normalizes a rule's extension list and rejects empty lists or duplicates within the rule.
pub fn normalize_and_validate_extensions(raw_exts: &[String]) -> Result<Vec<String>, AppError> {
    if raw_exts.is_empty() {
        return Err(AppError::InvalidInput {
            message: "a rule must contain at least one extension".to_owned(),
        });
    }

    let mut seen = HashSet::with_capacity(raw_exts.len());
    let mut normalized = Vec::with_capacity(raw_exts.len());

    for raw in raw_exts {
        let ext = normalize_extension(raw)?;
        if !seen.insert(ext.clone()) {
            return Err(AppError::InvalidInput {
                message: format!("duplicate extension '{ext}' within the same rule"),
            });
        }
        normalized.push(ext);
    }

    Ok(normalized)
}

fn has_extra_filters(rule: &Rule) -> bool {
    rule.name_contains.is_some()
        || rule.min_size_bytes.is_some()
        || rule.max_size_bytes.is_some()
        || rule.min_age_days.is_some()
}

/// Computes non-blocking overlap warnings for extensions claimed by multiple enabled
/// rules, naming the deterministic winner (`priority ASC, id ASC`).
///
/// Intentional narrow rules with extra keyword/size/age filters sitting above a broad
/// extension rule do not emit false overlap warnings unless two rules share the same filter.
#[must_use]
pub fn detect_extension_overlaps(rules: &[Rule]) -> Vec<ExtensionOverlapWarning> {
    let mut sorted_enabled: Vec<&Rule> = rules.iter().filter(|r| r.enabled).collect();
    sorted_enabled.sort_by_key(|r| (r.priority, r.id));

    let mut claims: BTreeMap<String, Vec<&Rule>> = BTreeMap::new();
    for rule in sorted_enabled {
        for ext in &rule.extensions {
            claims.entry(ext.clone()).or_default().push(rule);
        }
    }

    let mut warnings = Vec::new();
    for (extension, claimants) in claims {
        if claimants.len() > 1 {
            let winner = claimants[0];
            let shadowed: Vec<i64> = claimants[1..]
                .iter()
                .filter(|other| {
                    !has_extra_filters(winner)
                        || (winner.name_contains.as_deref().map(str::to_lowercase)
                            == other.name_contains.as_deref().map(str::to_lowercase)
                            && winner.min_size_bytes == other.min_size_bytes
                            && winner.max_size_bytes == other.max_size_bytes
                            && winner.min_age_days == other.min_age_days)
                })
                .map(|r| r.id)
                .collect();

            if !shadowed.is_empty() {
                warnings.push(ExtensionOverlapWarning {
                    extension,
                    winner_rule_id: winner.id,
                    winner_rule_name: winner.name.clone(),
                    shadowed_rule_ids: shadowed,
                });
            }
        }
    }
    warnings
}

/// Result of matching a filename against configured sorting rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleMatchOutcome<'a> {
    Matched(&'a Rule),
    MatchedDisabledOnly(&'a Rule),
    Unmatched,
}

/// Extracts candidate extension suffixes longest-first in Unicode lowercase.
///
/// Examples:
/// - `archive.tar.gz` -> `["tar.gz", "gz"]`
/// - `report.final.v2.pdf` -> `["final.v2.pdf", "v2.pdf", "pdf"]`
/// - `.gitignore` -> `[]` (leading dot is part of filename, no extension)
/// - `.env.local` -> `["local"]`
/// - `file.` or `noext` -> `[]`
#[must_use]
pub fn extract_candidate_extensions(filename: &str) -> Vec<String> {
    if filename.is_empty() || filename.ends_with('.') {
        return Vec::new();
    }

    let dot_positions: Vec<usize> = filename
        .match_indices('.')
        .map(|(idx, _)| idx)
        .filter(|&idx| idx > 0)
        .collect();

    if dot_positions.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::with_capacity(dot_positions.len());
    for pos in dot_positions {
        let suffix = &filename[pos + 1..];
        if suffix.is_empty() || suffix.starts_with('.') || suffix.contains("..") {
            continue;
        }
        candidates.push(suffix.to_lowercase());
    }
    candidates
}

/// Evaluates `filename` against `rules` in deterministic `(priority ASC, id ASC)`
/// order, checking longest candidate extensions first.
#[must_use]
pub fn match_rule<'a>(filename: &str, rules: &'a [Rule]) -> RuleMatchOutcome<'a> {
    match_rule_with_context(filename, 0, None, 0, rules)
}

/// Evaluates `filename`, `file_size`, and modification age against `rules` in
/// deterministic `(priority ASC, id ASC)` order.
#[must_use]
pub fn match_rule_with_context<'a>(
    filename: &str,
    file_size: u64,
    modified_epoch_ms: Option<i64>,
    now_epoch_ms: i64,
    rules: &'a [Rule],
) -> RuleMatchOutcome<'a> {
    let candidates = extract_candidate_extensions(filename);
    if candidates.is_empty() {
        return RuleMatchOutcome::Unmatched;
    }

    let lower_filename = filename.to_lowercase();

    let mut enabled: Vec<&Rule> = rules.iter().filter(|r| r.enabled).collect();
    enabled.sort_by_key(|r| (r.priority, r.id));

    for cand in &candidates {
        if let Some(&rule) = enabled.iter().find(|r| {
            r.extensions.iter().any(|ext| ext == cand)
                && rule_context_matches(
                    r,
                    &lower_filename,
                    file_size,
                    modified_epoch_ms,
                    now_epoch_ms,
                )
        }) {
            return RuleMatchOutcome::Matched(rule);
        }
    }

    let mut disabled: Vec<&Rule> = rules.iter().filter(|r| !r.enabled).collect();
    disabled.sort_by_key(|r| (r.priority, r.id));

    for cand in &candidates {
        if let Some(&rule) = disabled.iter().find(|r| {
            r.extensions.iter().any(|ext| ext == cand)
                && rule_context_matches(
                    r,
                    &lower_filename,
                    file_size,
                    modified_epoch_ms,
                    now_epoch_ms,
                )
        }) {
            return RuleMatchOutcome::MatchedDisabledOnly(rule);
        }
    }

    RuleMatchOutcome::Unmatched
}

fn rule_context_matches(
    rule: &Rule,
    lower_filename: &str,
    file_size: u64,
    modified_epoch_ms: Option<i64>,
    now_epoch_ms: i64,
) -> bool {
    if let Some(ref keyword) = rule.name_contains {
        let lower_kw = keyword.trim().to_lowercase();
        if !lower_kw.is_empty() && !lower_filename.contains(&lower_kw) {
            return false;
        }
    }

    if let Some(min_bytes) = rule.min_size_bytes {
        if file_size < min_bytes {
            return false;
        }
    }

    if let Some(max_bytes) = rule.max_size_bytes {
        if file_size > max_bytes {
            return false;
        }
    }

    if let Some(min_days) = rule.min_age_days {
        if min_days > 0 {
            let Some(mod_ms) = modified_epoch_ms else {
                return false;
            };
            let required_age_ms = i64::from(min_days).saturating_mul(MS_PER_DAY);
            if now_epoch_ms.saturating_sub(mod_ms) < required_age_ms {
                return false;
            }
        }
    }

    true
}
