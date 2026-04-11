use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::{
    DiagnosticDocument, DiagnosticNode, FingerprintSet, IR_SPEC_VERSION, Location, Ownership,
    SnapshotKind,
};

impl DiagnosticDocument {
    /// Recomputes fingerprints for all nodes and the document itself.
    pub fn refresh_fingerprints(&mut self) {
        for node in &mut self.diagnostics {
            refresh_node_fingerprints(node);
        }
        self.fingerprints = None;
        self.fingerprints = Some(FingerprintSet {
            raw: fingerprint_for(&self.diagnostics),
            structural: fingerprint_for(&canonical_snapshot_value(self)),
            family: fingerprint_for(
                &self
                    .diagnostics
                    .iter()
                    .map(|node| {
                        node.analysis
                            .as_ref()
                            .and_then(|analysis| analysis.family.clone())
                            .unwrap_or_else(|| "unknown".into())
                    })
                    .collect::<Vec<_>>(),
            ),
        });
    }

    /// Serialises this document to deterministic, sorted-key, pretty-printed JSON.
    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        canonical_json(self)
    }
}

fn refresh_node_fingerprints(node: &mut DiagnosticNode) {
    for child in &mut node.children {
        refresh_node_fingerprints(child);
    }
    node.fingerprints = None;
    let family_seed = node
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.family.as_deref())
        .unwrap_or("unknown");
    node.fingerprints = Some(FingerprintSet {
        raw: fingerprint_for(&node.message.raw_text),
        structural: fingerprint_for(&canonical_snapshot_value(node)),
        family: fingerprint_for(&format!(
            "{}:{}:{}:{}",
            family_seed,
            normalize_message(&node.message.raw_text),
            node.phase,
            node.primary_location()
                .and_then(Location::ownership)
                .map(Ownership::to_string)
                .unwrap_or_else(|| "unknown".to_string())
        )),
    });
}

/// Serialises any `Serialize` value to deterministic, sorted-key, pretty-printed JSON.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let value = canonical_snapshot_value(value);
    serde_json::to_string_pretty(&value)
}

/// Converts any `Serialize` value to a [`serde_json::Value`] with recursively sorted keys.
pub fn canonical_snapshot_value<T: Serialize>(value: &T) -> Value {
    match serde_json::to_value(value) {
        Ok(value) => sort_value(value),
        Err(error) => Value::String(format!("serialization_error:{error}")),
    }
}

/// Produces a normalised copy of a document suitable for snapshot testing (analysis included).
pub fn normalize_for_snapshot(document: &DiagnosticDocument) -> DiagnosticDocument {
    normalize_for_snapshot_kind(document, SnapshotKind::AnalysisIncluded)
}

/// Produces a normalised copy of a document for the given [`SnapshotKind`].
///
/// Volatile fields (IDs, versions, tool versions, digests) are replaced with
/// stable placeholders so that snapshots are deterministic across runs.
pub fn normalize_for_snapshot_kind(
    document: &DiagnosticDocument,
    kind: SnapshotKind,
) -> DiagnosticDocument {
    let mut copy = document.clone();
    copy.document_id = "<document>".to_string();
    copy.schema_version = IR_SPEC_VERSION.to_string();
    copy.producer.version = "<normalized>".to_string();
    copy.producer.git_revision = None;
    copy.producer.build_profile = None;
    copy.run.invocation_id = "<invocation>".to_string();
    if let Some(cwd) = copy.run.cwd_display.as_mut() {
        *cwd = "<cwd>".to_string();
    }
    copy.run.primary_tool.version = None;
    for tool in &mut copy.run.secondary_tools {
        tool.version = None;
    }
    for capture in &mut copy.captures {
        if capture.external_ref.is_some() {
            capture.external_ref = Some(format!("<capture:{}>", capture.id));
        }
        capture.digest_sha256 = None;
        if let Some(tool) = capture.produced_by.as_mut() {
            tool.version = None;
        }
    }
    if matches!(kind, SnapshotKind::FactsOnly) {
        for diagnostic in &mut copy.diagnostics {
            strip_analysis(diagnostic);
        }
    }
    copy.refresh_fingerprints();
    copy
}

/// Convenience: normalise a document and return its canonical JSON for the given snapshot kind.
pub fn snapshot_json(
    document: &DiagnosticDocument,
    kind: SnapshotKind,
) -> Result<String, serde_json::Error> {
    canonical_json(&normalize_for_snapshot_kind(document, kind))
}

/// Normalises a message string by replacing all numeric literals with `<n>`.
pub fn normalize_message(message: &str) -> String {
    let number_re = Regex::new(r"\d+").expect("compile-time regex");
    number_re.replace_all(message, "<n>").into_owned()
}

/// Computes a SHA-256 fingerprint of the canonical JSON representation of `value`.
pub fn fingerprint_for<T: Serialize>(value: &T) -> String {
    let canonical = canonical_snapshot_value(value);
    let payload = serde_json::to_vec(&canonical).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

fn strip_analysis(node: &mut DiagnosticNode) {
    node.analysis = None;
    for child in &mut node.children {
        strip_analysis(child);
    }
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(sort_value).collect()),
        Value::Object(object) => {
            let mut sorted = BTreeMap::new();
            for (key, value) in object {
                sorted.insert(key, sort_value(value));
            }
            let mut result = Map::new();
            for (key, value) in sorted {
                result.insert(key, value);
            }
            Value::Object(result)
        }
        other => other,
    }
}
