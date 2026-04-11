use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use crate::{
    CONFIDENCE_CERTAIN_THRESHOLD, CONFIDENCE_LIKELY_THRESHOLD, CONFIDENCE_POSSIBLE_THRESHOLD, Score,
};

/// Discrete confidence bucket for analysis results.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// High confidence (score >= 0.85).
    High,
    /// Medium confidence (score >= 0.60).
    Medium,
    /// Low confidence (score >= 0.35).
    Low,
    /// Confidence is unknown or below the minimum threshold.
    Unknown,
}

/// Renderer-facing confidence tier controlling what analysis details are disclosed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DisclosureConfidence {
    /// Full disclosure -- analysis title and first-action are shown.
    Certain,
    /// Most details are shown; first-action is included.
    Likely,
    /// Limited disclosure; a low-confidence notice is required.
    Possible,
    /// Analysis details are suppressed entirely.
    Hidden,
}

impl Confidence {
    /// Returns a representative numeric score for this confidence bucket.
    pub fn score(self) -> Score {
        OrderedFloat(match self {
            Self::High => 0.9,
            Self::Medium => 0.65,
            Self::Low => 0.35,
            Self::Unknown => 0.0,
        })
    }

    /// Converts an optional numeric score into a [`Confidence`] bucket.
    pub fn from_score(score: Option<Score>) -> Self {
        match DisclosureConfidence::from_score(score) {
            DisclosureConfidence::Certain => Self::High,
            DisclosureConfidence::Likely => Self::Medium,
            DisclosureConfidence::Possible => Self::Low,
            DisclosureConfidence::Hidden => Self::Unknown,
        }
    }
}

impl DisclosureConfidence {
    /// Maps an optional numeric score to a disclosure tier using the threshold constants.
    pub fn from_score(score: Option<Score>) -> Self {
        let Some(score) = score else {
            return Self::Hidden;
        };
        let score = score.into_inner();
        if score >= CONFIDENCE_CERTAIN_THRESHOLD {
            Self::Certain
        } else if score >= CONFIDENCE_LIKELY_THRESHOLD {
            Self::Likely
        } else if score >= CONFIDENCE_POSSIBLE_THRESHOLD {
            Self::Possible
        } else {
            Self::Hidden
        }
    }

    /// Returns `true` if the analysis headline may be shown to the user.
    pub fn allows_analysis_title(self) -> bool {
        matches!(self, Self::Certain | Self::Likely)
    }

    /// Returns `true` if the first-action hint may be shown to the user.
    pub fn allows_first_action(self) -> bool {
        matches!(self, Self::Certain | Self::Likely)
    }

    /// Returns `true` if a low-confidence notice should accompany the output.
    pub fn requires_low_confidence_notice(self) -> bool {
        matches!(self, Self::Possible | Self::Hidden)
    }
}
