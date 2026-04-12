use ordered_float::OrderedFloat;
use serde::{Deserialize, Deserializer, Serialize};
use std::borrow::Cow;

use crate::{Confidence, DisclosureConfidence, Score};

/// Enrichment-stage analysis annotations attached to a [`crate::DiagnosticNode`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisOverlay {
    /// Diagnostic family identifier (e.g. `"syntax"`, `"linker"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<Cow<'static, str>>,
    /// Version of the family classification rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family_version: Option<String>,
    /// Confidence score for the family classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family_confidence: Option<Score>,
    /// Score indicating how likely this node is the root cause.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_cause_score: Option<Score>,
    /// Score indicating how actionable this diagnostic is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actionability_score: Option<Score>,
    /// Priority score for user-owned code vs. system/vendor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_code_priority: Option<Score>,
    /// Short headline suitable for a title bar or summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headline: Option<Cow<'static, str>>,
    /// Suggested first action the user should take.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_action_hint: Option<Cow<'static, str>>,
    /// Overall analysis confidence score (0.0..=1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "deserialize_confidence_score_opt")]
    pub confidence: Option<Score>,
    /// ID of the preferred primary location for rendering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_primary_location_id: Option<String>,
    /// Rule identifier that matched this diagnostic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<Cow<'static, str>>,
    /// Conditions from the rule that matched.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub matched_conditions: Vec<Cow<'static, str>>,
    /// Reason this diagnostic was suppressed, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppression_reason: Option<String>,
    /// IDs of child nodes that should be collapsed in rendering.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub collapsed_child_ids: Vec<String>,
    /// IDs of context chains that should be collapsed in rendering.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub collapsed_chain_ids: Vec<String>,
    /// Group reference for deduplication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_ref: Option<String>,
    /// Human-readable reasons explaining analysis decisions.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub reasons: Vec<String>,
    /// Policy profile name applied during analysis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_profile: Option<String>,
    /// Version of the analysis producer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer_version: Option<String>,
}

/// Document-wide cascade analysis attached to a [`crate::DiagnosticDocument`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DocumentAnalysis {
    /// Policy profile name applied during analysis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_profile: Option<String>,
    /// Version of the analysis producer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer_version: Option<String>,
    /// Episode graph produced by document-wide cascade analysis.
    #[serde(default)]
    pub episode_graph: EpisodeGraph,
    /// Per-group cascade analysis materialized for renderer/debug consumers.
    #[serde(default)]
    pub group_analysis: Vec<GroupCascadeAnalysis>,
    /// Aggregate counts for the cascade analysis result.
    #[serde(default)]
    pub stats: CascadeStats,
}

/// Graph describing how logical diagnostic groups relate to each other.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EpisodeGraph {
    /// Episode records identified for this document.
    #[serde(default)]
    pub episodes: Vec<DiagnosticEpisode>,
    /// Relations detected between logical diagnostic groups.
    #[serde(default)]
    pub relations: Vec<EpisodeRelation>,
}

/// A cluster of logically related diagnostic groups.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticEpisode {
    /// Unique episode identifier within the document.
    pub episode_ref: String,
    /// The lead logical group for this episode.
    pub lead_group_ref: String,
    /// All member logical groups belonging to this episode.
    #[serde(default)]
    pub member_group_refs: Vec<String>,
    /// Coarse diagnostic family, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Score indicating how strongly the lead group appears to be the root cause.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead_root_score: Option<Score>,
    /// Overall confidence for the episode grouping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Score>,
}

/// Cascade analysis for one logical diagnostic group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupCascadeAnalysis {
    /// Unique logical group identifier within the document.
    pub group_ref: String,
    /// Episode this group belongs to, if assigned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_ref: Option<String>,
    /// Role of this group within its episode.
    pub role: GroupCascadeRole,
    /// Best candidate parent group when the group looks dependent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_parent_group_ref: Option<String>,
    /// Score indicating how likely this group is a root cause.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_score: Option<Score>,
    /// Score indicating how independent this group is from surrounding groups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub independence_score: Option<Score>,
    /// Score indicating how likely this group can be safely suppressed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress_likelihood: Option<Score>,
    /// Score indicating how likely this group should be rendered as summary-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_likelihood: Option<Score>,
    /// Minimum visibility guarantee for this group.
    pub visibility_floor: VisibilityFloor,
    /// Evidence labels supporting the cascade decision.
    #[serde(default)]
    pub evidence_tags: Vec<String>,
}

/// Role assigned to a logical group by cascade analysis.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GroupCascadeRole {
    /// The lead root for an episode.
    LeadRoot,
    /// An additional independent root error.
    IndependentRoot,
    /// A follow-on diagnostic likely caused by another group.
    FollowOn,
    /// A near-duplicate of another group.
    Duplicate,
    /// Not enough evidence to safely classify the group.
    #[default]
    Uncertain,
}

/// Minimum visibility guarantee for a logical group.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityFloor {
    /// The group must remain visible and may not be hidden.
    #[default]
    NeverHidden,
    /// The group may be summary-only but must not be fully hidden.
    SummaryOrExpandedOnly,
    /// The group may be hidden when policy and evidence allow it.
    HiddenAllowed,
}

/// Directed relationship between two logical diagnostic groups.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpisodeRelation {
    /// Source logical group for the relation.
    pub from_group_ref: String,
    /// Target logical group for the relation.
    pub to_group_ref: String,
    /// Kind of relationship detected between the groups.
    pub kind: EpisodeRelationKind,
    /// Confidence score for the relation.
    pub confidence: Score,
    /// Evidence labels supporting the relation.
    #[serde(default)]
    pub evidence_tags: Vec<String>,
}

/// Relation class between logical diagnostic groups.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeRelationKind {
    /// A likely parent/child cascade relation.
    Cascade,
    /// A likely duplicate relation.
    Duplicate,
    /// A context-only relation without suppression implications.
    Context,
}

/// Summary counts for a document-wide cascade analysis result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CascadeStats {
    /// Number of groups classified as independent roots.
    pub independent_root_count: u32,
    /// Number of dependent follow-on groups.
    pub dependent_follow_on_count: u32,
    /// Number of groups classified as duplicates.
    pub duplicate_count: u32,
    /// Number of groups left uncertain.
    pub uncertain_count: u32,
}

/// Resolved cascade policy derived from built-ins, config, and CLI flags.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CascadePolicySnapshot {
    /// Coarse compression profile controlling suppression aggressiveness.
    pub compression_level: CompressionLevel,
    /// Minimum score required before hidden suppression is allowed.
    pub suppress_likelihood_threshold: f32,
    /// Minimum score required before summary compaction is allowed.
    pub summary_likelihood_threshold: f32,
    /// Minimum margin required between parent and child candidates.
    pub min_parent_margin: f32,
    /// Maximum number of independent roots expanded in the default view.
    pub max_expanded_independent_roots: usize,
    /// Policy controlling whether suppressed counts are shown.
    pub show_suppressed_count: SuppressedCountVisibility,
}

impl Default for CascadePolicySnapshot {
    fn default() -> Self {
        Self {
            compression_level: CompressionLevel::Aggressive,
            suppress_likelihood_threshold: 0.78,
            summary_likelihood_threshold: 0.55,
            min_parent_margin: 0.12,
            max_expanded_independent_roots: 2,
            show_suppressed_count: SuppressedCountVisibility::Always,
        }
    }
}

/// Compression profile used by cascade analysis.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompressionLevel {
    /// Disable hidden suppression and summary compaction.
    Off,
    /// Suppress only the safest duplicates.
    Conservative,
    /// Allow duplicate suppression and very strong follow-on compaction.
    Balanced,
    /// Use the most aggressive shipped compaction profile.
    #[default]
    Aggressive,
}

/// Visibility policy for the suppressed-count disclosure line.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SuppressedCountVisibility {
    /// Let the renderer decide whether to show the count line.
    Auto,
    /// Always show the suppressed-count line when suppression happened.
    #[default]
    Always,
    /// Never show the suppressed-count line.
    Never,
}

/// A set of deterministic SHA-256 fingerprints used for drift detection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FingerprintSet {
    /// Hash of the raw message text.
    pub raw: String,
    /// Hash of the canonical (sorted-key) JSON snapshot.
    pub structural: String,
    /// Hash incorporating the diagnostic family classification.
    pub family: String,
}

/// Error returned when [`crate::DiagnosticDocument::validate`] finds problems.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("document validation failed")]
pub struct ValidationErrors {
    /// Individual validation error messages.
    pub errors: Vec<String>,
}

impl AnalysisOverlay {
    /// Sets the confidence from a raw `f32` score.
    pub fn set_confidence_score(&mut self, score: f32) {
        self.confidence = Some(OrderedFloat(score));
    }

    /// Sets the confidence from a discrete [`Confidence`] bucket.
    pub fn set_confidence_bucket(&mut self, confidence: Confidence) {
        self.confidence = Some(confidence.score());
    }

    /// Returns the raw confidence score, if set.
    pub fn confidence_score(&self) -> Option<Score> {
        self.confidence
    }

    /// Returns the confidence as a discrete bucket, if set.
    pub fn confidence_bucket(&self) -> Option<Confidence> {
        self.confidence
            .map(|score| Confidence::from_score(Some(score)))
    }

    /// Maps the confidence score to a [`DisclosureConfidence`] tier for the renderer.
    pub fn disclosure_confidence(&self) -> DisclosureConfidence {
        DisclosureConfidence::from_score(self.confidence)
    }
}

fn deserialize_confidence_score_opt<'de, D>(deserializer: D) -> Result<Option<Score>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ConfidenceWire {
        Score(f32),
        Bucket(Confidence),
    }

    let confidence = Option::<ConfidenceWire>::deserialize(deserializer)?;
    Ok(confidence.map(|confidence| match confidence {
        ConfidenceWire::Score(score) => OrderedFloat(score),
        ConfidenceWire::Bucket(bucket) => bucket.score(),
    }))
}
