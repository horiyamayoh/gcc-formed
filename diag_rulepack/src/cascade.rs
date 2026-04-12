//! Code-backed cascade policy authority for WP-005.
//!
//! The checked-in manifest currently externalizes enrich/residual/render
//! sections only. Cascade tuning still needs one authoritative home, so this
//! module provides the phase1 checked-in cascade policy that `diag_cascade`
//! consumes directly.

use diag_backend_probe::{ProcessingPath, VersionBand};
use diag_core::{FallbackGrade, SourceAuthority};

/// Thresholds and reason weights used by document-wide cascade scoring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CascadeWeights {
    /// Minimum root score that still counts as independent when no parent wins.
    pub independent_root_score: f32,
    /// Minimum accepted dependency score before a parent is chosen.
    pub dependency_threshold: f32,
    /// Minimum accepted duplicate score before a duplicate relation is chosen.
    pub duplicate_threshold: f32,
    /// Minimum root-score advantage a parent needs before receiving the bonus.
    pub parent_root_advantage_min: f32,
    /// Bonus applied when a linker summary line survives the prefilter.
    pub linker_summary_window_bonus: f32,
}

/// Path-aware redundancy policy controlling how aggressive hidden suppression may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CascadeRedundancyPolicy {
    /// Extra dependency score required for this band/path/source combination.
    pub dependency_threshold_delta: i16,
    /// Extra parent-margin score required before accepting a hidden relation.
    pub margin_delta: i16,
    /// Additional evidence points required before hidden suppression is allowed.
    pub extra_evidence_points: usize,
    /// Disables hidden suppression entirely.
    pub hidden_disabled: bool,
    /// Restricts hidden suppression to duplicates only.
    pub duplicate_only: bool,
    /// Lowers suppress-likelihood for weaker paths.
    pub suppress_penalty: i16,
}

impl CascadeRedundancyPolicy {
    /// Returns the dependency-threshold delta as an f32 score.
    pub fn dependency_threshold_delta_score(self) -> f32 {
        score_from_basis_points(self.dependency_threshold_delta)
    }

    /// Returns the margin delta as an f32 score.
    pub fn margin_delta_score(self) -> f32 {
        score_from_basis_points(self.margin_delta)
    }

    /// Returns the suppress-penalty delta as an f32 score.
    pub fn suppress_penalty_score(self) -> f32 {
        score_from_basis_points(self.suppress_penalty)
    }
}

/// Family policy used by the generic cascade engine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CascadeFamilyPolicy {
    family: &'static str,
    exact_families: &'static [&'static str],
    prefix_families: &'static [&'static str],
    /// Whether this family should receive the strong-root bonus by default.
    pub strong_root: bool,
    root_terms: &'static [&'static str],
    follow_on_terms: &'static [&'static str],
    candidate_repeat_terms: &'static [&'static str],
    generic_wrapper_terms: &'static [&'static str],
    /// Extra cascade score when the child clearly looks like a family follow-on.
    pub follow_on_cascade_bonus: f32,
    /// Extra duplicate score when the child clearly looks like a repeated candidate.
    pub candidate_duplicate_bonus: f32,
    /// Extra cascade score when the child is only a generic wrapper/summary.
    pub generic_wrapper_cascade_bonus: f32,
}

impl CascadeFamilyPolicy {
    fn matches_family(self, family: &str) -> bool {
        self.exact_families.contains(&family)
            || self
                .prefix_families
                .iter()
                .any(|prefix| family.starts_with(prefix))
    }
}

/// Checked-in cascade rulepack consumed by `diag_cascade`.
#[derive(Debug, Clone, PartialEq)]
pub struct CascadeRulepack {
    weights: CascadeWeights,
    family_policies: &'static [CascadeFamilyPolicy],
}

impl CascadeRulepack {
    /// Returns the checked-in generic thresholds and reason weights.
    pub fn weights(&self) -> CascadeWeights {
        self.weights
    }

    /// Returns the best-matching family policy, or the generic fallback policy.
    pub fn family_policy(&self, family: &str) -> &'static CascadeFamilyPolicy {
        self.family_policies
            .iter()
            .find(|policy| policy.matches_family(family))
            .unwrap_or(&UNKNOWN_POLICY)
    }

    /// Returns `true` when the family/message pair should receive a strong-root bonus.
    pub fn is_strong_root(&self, family: &str, message_lower: &str) -> bool {
        let policy = self.family_policy(family);
        policy.strong_root || contains_any(message_lower, policy.root_terms)
    }

    /// Returns `true` when the family/message pair looks like a generic follow-on line.
    pub fn is_generic_follow_on(&self, family: &str, message_lower: &str) -> bool {
        contains_any(message_lower, COMMON_FOLLOW_ON_TERMS)
            || contains_any(message_lower, self.family_policy(family).follow_on_terms)
    }

    /// Returns `true` when the family/message pair looks like a repeated candidate/detail note.
    pub fn is_candidate_repeat(&self, family: &str, message_lower: &str) -> bool {
        contains_any(message_lower, COMMON_CANDIDATE_REPEAT_TERMS)
            || contains_any(
                message_lower,
                self.family_policy(family).candidate_repeat_terms,
            )
    }

    /// Returns `true` when the family/message pair is only a generic wrapper around a real root.
    pub fn is_generic_wrapper(&self, family: &str, message_lower: &str) -> bool {
        contains_any(message_lower, COMMON_GENERIC_WRAPPER_TERMS)
            || contains_any(
                message_lower,
                self.family_policy(family).generic_wrapper_terms,
            )
    }

    /// Returns `true` when the two families should be paired as a linker-summary window.
    pub fn is_linker_summary_pair(&self, left_family: &str, right_family: &str) -> bool {
        (is_linker_summary_family(left_family) && is_specific_linker_root_family(right_family))
            || (is_linker_summary_family(right_family)
                && is_specific_linker_root_family(left_family))
    }

    /// Returns the path-aware redundancy tuning for the given context.
    pub fn redundancy_policy(
        &self,
        version_band: VersionBand,
        processing_path: ProcessingPath,
        source_authority: SourceAuthority,
        fallback_grade: FallbackGrade,
    ) -> CascadeRedundancyPolicy {
        let mut policy = CascadeRedundancyPolicy::default();

        match version_band {
            VersionBand::Gcc15Plus => {}
            VersionBand::Gcc13_14 => {
                policy.suppress_penalty -= 2;
            }
            VersionBand::Gcc9_12 => {
                policy.extra_evidence_points += 1;
                policy.suppress_penalty -= 5;
            }
            VersionBand::Unknown => {
                policy.extra_evidence_points += 1;
                policy.suppress_penalty -= 10;
            }
        }

        match processing_path {
            ProcessingPath::DualSinkStructured => {}
            ProcessingPath::SingleSinkStructured => {
                policy.suppress_penalty -= 2;
            }
            ProcessingPath::NativeTextCapture => {
                policy.extra_evidence_points += 1;
                policy.suppress_penalty -= 10;
            }
            ProcessingPath::Passthrough => {
                policy.hidden_disabled = true;
                policy.suppress_penalty -= 18;
            }
        }

        match source_authority {
            SourceAuthority::Structured => {}
            SourceAuthority::ResidualText => {
                policy.extra_evidence_points += 1;
                policy.suppress_penalty -= 8;
            }
            SourceAuthority::None => {
                policy.hidden_disabled = true;
                policy.suppress_penalty -= 12;
            }
        }

        match fallback_grade {
            FallbackGrade::None => {}
            FallbackGrade::Compatibility => {
                policy.extra_evidence_points += 1;
                policy.suppress_penalty -= 8;
            }
            FallbackGrade::FailOpen => {
                policy.duplicate_only = true;
                policy.extra_evidence_points += 1;
                policy.suppress_penalty -= 14;
            }
        }

        policy
    }
}

/// Returns the checked-in phase1 cascade rulepack.
pub fn checked_in_cascade_rulepack() -> &'static CascadeRulepack {
    &CHECKED_IN_CASCADE_RULEPACK
}

const CHECKED_IN_CASCADE_RULEPACK: CascadeRulepack = CascadeRulepack {
    weights: CascadeWeights {
        independent_root_score: 0.50,
        dependency_threshold: 0.70,
        duplicate_threshold: 0.78,
        parent_root_advantage_min: 0.05,
        linker_summary_window_bonus: 0.24,
    },
    family_policies: &[
        COLLECT2_SUMMARY_POLICY,
        LINKER_ROOT_POLICY,
        TEMPLATE_POLICY,
        TYPE_OVERLOAD_POLICY,
        SYNTAX_POLICY,
    ],
};

const UNKNOWN_POLICY: CascadeFamilyPolicy = CascadeFamilyPolicy {
    family: "unknown",
    exact_families: &["unknown"],
    prefix_families: &[],
    strong_root: false,
    root_terms: &[],
    follow_on_terms: &[],
    candidate_repeat_terms: &[],
    generic_wrapper_terms: &[],
    follow_on_cascade_bonus: 0.12,
    candidate_duplicate_bonus: 0.10,
    generic_wrapper_cascade_bonus: 0.18,
};

const SYNTAX_POLICY: CascadeFamilyPolicy = CascadeFamilyPolicy {
    family: "syntax",
    exact_families: &["syntax"],
    prefix_families: &[],
    strong_root: true,
    root_terms: &[
        "expected ",
        "before ",
        "after ",
        "at end of input",
        "does not name a type",
    ],
    follow_on_terms: &[
        "expected declaration or statement at end of input",
        "expected identifier or '('",
        "expected declaration specifiers",
        "expected unqualified-id",
        "expected primary-expression",
        "expected initializer before",
    ],
    candidate_repeat_terms: &[],
    generic_wrapper_terms: &[],
    follow_on_cascade_bonus: 0.26,
    candidate_duplicate_bonus: 0.0,
    generic_wrapper_cascade_bonus: 0.0,
};

const TYPE_OVERLOAD_POLICY: CascadeFamilyPolicy = CascadeFamilyPolicy {
    family: "type_overload",
    exact_families: &["type_overload"],
    prefix_families: &[],
    strong_root: true,
    root_terms: &[
        "no matching function",
        "cannot convert",
        "invalid conversion",
        "incompatible type",
        "passing argument",
        "deduced conflicting",
    ],
    follow_on_terms: &[
        "there is ",
        "there are ",
        "candidate ",
        "candidate expects",
        "conversion candidate",
        "no known conversion",
        "mismatched types",
        "could not convert",
    ],
    candidate_repeat_terms: &[
        "candidate ",
        "candidate expects",
        "conversion candidate",
        "deduced conflicting",
        "mismatched types",
    ],
    generic_wrapper_terms: &[],
    follow_on_cascade_bonus: 0.18,
    candidate_duplicate_bonus: 0.16,
    generic_wrapper_cascade_bonus: 0.0,
};

const TEMPLATE_POLICY: CascadeFamilyPolicy = CascadeFamilyPolicy {
    family: "template",
    exact_families: &["template"],
    prefix_families: &[],
    strong_root: true,
    root_terms: &[
        "template",
        "deduction/substitution",
        "class template argument deduction failed",
        "template argument deduction/substitution failed",
    ],
    follow_on_terms: &[
        "there is ",
        "there are ",
        "candidate ",
        "template argument deduction/substitution failed",
        "deduced conflicting",
        "required from here",
        "instantiated from here",
    ],
    candidate_repeat_terms: &[
        "candidate ",
        "template argument deduction/substitution failed",
        "deduced conflicting",
    ],
    generic_wrapper_terms: &[],
    follow_on_cascade_bonus: 0.30,
    candidate_duplicate_bonus: 0.18,
    generic_wrapper_cascade_bonus: 0.0,
};

const LINKER_ROOT_POLICY: CascadeFamilyPolicy = CascadeFamilyPolicy {
    family: "linker_root",
    exact_families: &[],
    prefix_families: &[
        "linker.undefined_reference",
        "linker.multiple_definition",
        "linker.cannot_find_library",
        "linker.file_format_or_relocation",
    ],
    strong_root: true,
    root_terms: &[
        "undefined reference",
        "multiple definition",
        "cannot find -l",
        "file format not recognized",
        "relocation truncated",
    ],
    follow_on_terms: &["first defined here"],
    candidate_repeat_terms: &[],
    generic_wrapper_terms: &[],
    follow_on_cascade_bonus: 0.12,
    candidate_duplicate_bonus: 0.0,
    generic_wrapper_cascade_bonus: 0.0,
};

const COLLECT2_SUMMARY_POLICY: CascadeFamilyPolicy = CascadeFamilyPolicy {
    family: "collect2_summary",
    exact_families: &["collect2_summary"],
    prefix_families: &[],
    strong_root: false,
    root_terms: &[],
    follow_on_terms: &["collect2: error:", "ld returned"],
    candidate_repeat_terms: &[],
    generic_wrapper_terms: &["collect2: error:", "ld returned"],
    follow_on_cascade_bonus: 0.18,
    candidate_duplicate_bonus: 0.0,
    generic_wrapper_cascade_bonus: 0.22,
};

const COMMON_FOLLOW_ON_TERMS: &[&str] = &[
    "required from here",
    "instantiated from here",
    "in expansion of macro",
    "previous declaration",
    "previous definition",
    "first defined here",
];

const COMMON_CANDIDATE_REPEAT_TERMS: &[&str] =
    &["candidate:", "candidate ", "conversion candidate"];

const COMMON_GENERIC_WRAPPER_TERMS: &[&str] = &["ld returned"];

fn contains_any(message_lower: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| message_lower.contains(term))
}

fn is_linker_summary_family(family: &str) -> bool {
    family == COLLECT2_SUMMARY_POLICY.family
}

fn is_specific_linker_root_family(family: &str) -> bool {
    LINKER_ROOT_POLICY.matches_family(family)
}

fn score_from_basis_points(value: i16) -> f32 {
    value as f32 / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_collect2_as_a_distinct_wrapper_family() {
        let rulepack = checked_in_cascade_rulepack();

        assert_eq!(
            rulepack.family_policy("collect2_summary").family,
            "collect2_summary"
        );
        assert!(rulepack.is_generic_wrapper(
            "collect2_summary",
            "collect2: error: ld returned 1 exit status"
        ));
        assert!(rulepack.is_linker_summary_pair("collect2_summary", "linker.undefined_reference"));
    }

    #[test]
    fn treats_numbered_candidate_lines_as_candidate_repeats() {
        let rulepack = checked_in_cascade_rulepack();

        assert!(rulepack.is_candidate_repeat(
            "template",
            "candidate 2: 'template<class t> pair(pair<t>) -> pair<t>'"
        ));
        assert!(
            rulepack
                .is_candidate_repeat("type_overload", "candidate expects 2 arguments, 1 provided")
        );
    }

    #[test]
    fn syntax_follow_on_terms_cover_parser_desync_tail_lines() {
        let rulepack = checked_in_cascade_rulepack();

        assert!(rulepack.is_strong_root("syntax", "expected ';' before '}' token"));
        assert!(rulepack.is_generic_follow_on(
            "syntax",
            "expected declaration or statement at end of input"
        ));
    }

    #[test]
    fn native_text_paths_require_more_hidden_evidence_than_dual_sink_structured() {
        let rulepack = checked_in_cascade_rulepack();
        let dual_sink = rulepack.redundancy_policy(
            VersionBand::Gcc15Plus,
            ProcessingPath::DualSinkStructured,
            SourceAuthority::Structured,
            FallbackGrade::None,
        );
        let native = rulepack.redundancy_policy(
            VersionBand::Gcc9_12,
            ProcessingPath::NativeTextCapture,
            SourceAuthority::ResidualText,
            FallbackGrade::FailOpen,
        );

        assert_eq!(dual_sink.extra_evidence_points, 0);
        assert!(native.extra_evidence_points >= 3);
        assert!(native.duplicate_only);
        assert!(native.suppress_penalty < dual_sink.suppress_penalty);
        assert_eq!(native.margin_delta, dual_sink.margin_delta);
        assert_eq!(
            native.dependency_threshold_delta,
            dual_sink.dependency_threshold_delta
        );
    }
}
