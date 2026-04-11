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
