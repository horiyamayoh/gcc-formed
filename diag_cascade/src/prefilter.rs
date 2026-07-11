use crate::logical_group::LogicalGroup;
use std::collections::{BTreeMap, BTreeSet};

/// Maximum ordinal distance used for same-translation-unit candidate windows.
pub const TRANSLATION_UNIT_ORDINAL_WINDOW: usize = 2;

/// Strong reason why two logical groups should survive the prefilter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateReason {
    /// Same file and nearby line buckets within one origin/phase.
    NearbyFileBucket,
    /// Same translation unit and nearby top-level ordinal within one origin/phase.
    TranslationUnitWindow,
    /// Same extracted linker/symbol identity.
    SharedSymbol,
    /// Same template-instantiation frontier.
    SharedTemplateFrontier,
    /// Same macro-expansion frontier.
    SharedMacroFrontier,
    /// Same include frontier.
    SharedIncludeFrontier,
    /// Adjacent linker summary line paired with a more specific linker root.
    LinkerSummaryWindow,
}

/// Deterministic candidate pair that survived the prefilter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatePair {
    /// Left group index in the original `LogicalGroup` slice.
    pub left_index: usize,
    /// Right group index in the original `LogicalGroup` slice.
    pub right_index: usize,
    /// Stable left group ref.
    pub left_group_ref: String,
    /// Stable right group ref.
    pub right_group_ref: String,
    /// Sorted reasons why the pair survived the prefilter.
    pub reasons: Vec<CandidateReason>,
}

/// Build deterministic candidate pairs without falling back to all-pairs comparison.
pub fn candidate_pairs(groups: &[LogicalGroup]) -> Vec<CandidatePair> {
    let mut pair_reasons = BTreeMap::<(usize, usize), BTreeSet<CandidateReason>>::new();

    add_exact_key_pairs(
        groups,
        groups
            .iter()
            .enumerate()
            .filter_map(|(index, group)| group.keys.symbol_key.clone().map(|key| (key, index))),
        CandidateReason::SharedSymbol,
        &mut pair_reasons,
    );
    add_exact_key_pairs(
        groups,
        groups.iter().enumerate().filter_map(|(index, group)| {
            group
                .keys
                .template_frontier_key
                .clone()
                .map(|key| (key, index))
        }),
        CandidateReason::SharedTemplateFrontier,
        &mut pair_reasons,
    );
    add_exact_key_pairs(
        groups,
        groups.iter().enumerate().filter_map(|(index, group)| {
            group
                .keys
                .macro_frontier_key
                .clone()
                .map(|key| (key, index))
        }),
        CandidateReason::SharedMacroFrontier,
        &mut pair_reasons,
    );
    add_exact_key_pairs(
        groups,
        groups.iter().enumerate().filter_map(|(index, group)| {
            group
                .keys
                .include_frontier_key
                .clone()
                .map(|key| (key, index))
        }),
        CandidateReason::SharedIncludeFrontier,
        &mut pair_reasons,
    );

    add_nearby_file_bucket_pairs(groups, &mut pair_reasons);
    add_translation_unit_window_pairs(groups, &mut pair_reasons);
    add_linker_summary_pairs(groups, &mut pair_reasons);

    pair_reasons
        .into_iter()
        .map(|((left_index, right_index), reasons)| CandidatePair {
            left_index,
            right_index,
            left_group_ref: groups[left_index].group_ref.clone(),
            right_group_ref: groups[right_index].group_ref.clone(),
            reasons: reasons.into_iter().collect(),
        })
        .collect()
}

fn add_exact_key_pairs<I>(
    groups: &[LogicalGroup],
    entries: I,
    reason: CandidateReason,
    pair_reasons: &mut BTreeMap<(usize, usize), BTreeSet<CandidateReason>>,
) where
    I: IntoIterator<Item = (String, usize)>,
{
    let mut buckets = BTreeMap::<String, Vec<usize>>::new();
    for (key, index) in entries {
        buckets.entry(key).or_default().push(index);
    }

    for indices in buckets.into_values() {
        if indices.len() < 2 {
            continue;
        }
        for left_offset in 0..indices.len() {
            for right_offset in (left_offset + 1)..indices.len() {
                add_pair(
                    groups,
                    indices[left_offset],
                    indices[right_offset],
                    reason,
                    pair_reasons,
                );
            }
        }
    }
}

fn add_nearby_file_bucket_pairs(
    groups: &[LogicalGroup],
    pair_reasons: &mut BTreeMap<(usize, usize), BTreeSet<CandidateReason>>,
) {
    let mut buckets = BTreeMap::<(String, u32), Vec<usize>>::new();
    for (index, group) in groups.iter().enumerate() {
        let Some(file_key) = group.keys.primary_file_key.clone() else {
            continue;
        };
        let Some(line_bucket) = group.keys.primary_line_bucket else {
            continue;
        };
        buckets
            .entry((file_key, line_bucket))
            .or_default()
            .push(index);
    }

    for (index, group) in groups.iter().enumerate() {
        let Some(file_key) = group.keys.primary_file_key.as_ref() else {
            continue;
        };
        let Some(line_bucket) = group.keys.primary_line_bucket else {
            continue;
        };
        let start_bucket = line_bucket.saturating_sub(1);
        let end_bucket = line_bucket.saturating_add(1);
        for candidate_bucket in start_bucket..=end_bucket {
            let Some(indices) = buckets.get(&(file_key.clone(), candidate_bucket)) else {
                continue;
            };
            for &other_index in indices {
                if group.keys.origin_phase_key != groups[other_index].keys.origin_phase_key {
                    continue;
                }
                add_pair(
                    groups,
                    index,
                    other_index,
                    CandidateReason::NearbyFileBucket,
                    pair_reasons,
                );
            }
        }
    }
}

fn add_translation_unit_window_pairs(
    groups: &[LogicalGroup],
    pair_reasons: &mut BTreeMap<(usize, usize), BTreeSet<CandidateReason>>,
) {
    let mut buckets = BTreeMap::<String, Vec<usize>>::new();
    for (index, group) in groups.iter().enumerate() {
        let Some(tu_key) = group.keys.translation_unit_key.clone() else {
            continue;
        };
        buckets.entry(tu_key).or_default().push(index);
    }

    for indices in buckets.into_values() {
        for left_offset in 0..indices.len() {
            let left_index = indices[left_offset];
            for &right_index in indices.iter().skip(left_offset + 1) {
                if groups[right_index].keys.ordinal_in_invocation
                    - groups[left_index].keys.ordinal_in_invocation
                    > TRANSLATION_UNIT_ORDINAL_WINDOW
                {
                    break;
                }
                if groups[left_index].keys.origin_phase_key
                    != groups[right_index].keys.origin_phase_key
                {
                    continue;
                }
                if let (Some(left_bucket), Some(right_bucket)) = (
                    groups[left_index].keys.primary_line_bucket,
                    groups[right_index].keys.primary_line_bucket,
                ) && left_bucket.abs_diff(right_bucket) > 1
                {
                    continue;
                }
                add_pair(
                    groups,
                    left_index,
                    right_index,
                    CandidateReason::TranslationUnitWindow,
                    pair_reasons,
                );
            }
        }
    }
}

fn add_linker_summary_pairs(
    groups: &[LogicalGroup],
    pair_reasons: &mut BTreeMap<(usize, usize), BTreeSet<CandidateReason>>,
) {
    for left_index in 0..groups.len() {
        for right_index in (left_index + 1)..groups.len() {
            if groups[right_index].keys.ordinal_in_invocation
                - groups[left_index].keys.ordinal_in_invocation
                > TRANSLATION_UNIT_ORDINAL_WINDOW
            {
                break;
            }
            if !groups[left_index].keys.origin_phase_key.ends_with(":link")
                || !groups[right_index].keys.origin_phase_key.ends_with(":link")
            {
                continue;
            }
            let left_summary = is_driver_summary(&groups[left_index].keys.normalized_message_key);
            let right_summary = is_driver_summary(&groups[right_index].keys.normalized_message_key);
            if left_summary == right_summary {
                continue;
            }
            add_pair(
                groups,
                left_index,
                right_index,
                CandidateReason::LinkerSummaryWindow,
                pair_reasons,
            );
        }
    }
}

fn is_driver_summary(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("collect2")
        || message.contains("ld returned")
        || message.contains("linker command failed")
}

fn add_pair(
    groups: &[LogicalGroup],
    left_index: usize,
    right_index: usize,
    reason: CandidateReason,
    pair_reasons: &mut BTreeMap<(usize, usize), BTreeSet<CandidateReason>>,
) {
    if left_index == right_index {
        return;
    }
    let (left_index, right_index) = if left_index < right_index {
        (left_index, right_index)
    } else {
        (right_index, left_index)
    };
    if groups[left_index].group_ref == groups[right_index].group_ref {
        return;
    }
    pair_reasons
        .entry((left_index, right_index))
        .or_default()
        .insert(reason);
}
