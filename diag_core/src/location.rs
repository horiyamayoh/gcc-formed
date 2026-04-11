use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    BoundarySemantics, ColumnUnit, LocationRole, LocationSourceKind, Ownership, PathKind,
    PathStyle, Provenance, Score,
};

/// A source-code location associated with a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// Unique identifier for this location within the document.
    pub id: String,
    /// File reference (path, ownership, etc.).
    pub file: FileRef,
    /// Single-point anchor (caret position).
    pub anchor: Option<SourcePoint>,
    /// Span range, if the location covers more than one point.
    pub range: Option<SourceRange>,
    /// Role this location plays for the diagnostic.
    pub role: LocationRole,
    /// How the location was derived (caret, range, token, etc.).
    pub source_kind: LocationSourceKind,
    /// Optional human-readable label for this location.
    pub label: Option<String>,
    /// Ownership override specific to this location.
    pub ownership_override: Option<OwnershipInfo>,
    /// Provenance override specific to this location.
    pub provenance_override: Option<Provenance>,
    /// Reference to a captured source excerpt artifact.
    pub source_excerpt_ref: Option<String>,
}

/// Reference to a source file, with optional display path and ownership.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileRef {
    /// Raw path as reported by the compiler.
    pub path_raw: String,
    /// Shortened or user-friendly display path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_path: Option<String>,
    /// File URI (e.g. `file:///...`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// Detected path style (POSIX, Windows, URI, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_style: Option<PathStyle>,
    /// Detected path kind (absolute, relative, virtual, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_kind: Option<PathKind>,
    /// Ownership classification for this file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<OwnershipInfo>,
    /// Whether the file existed on disk at capture time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exists_at_capture: Option<bool>,
}

/// Ownership classification for a file, with a reason and optional confidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnershipInfo {
    /// Who owns this file.
    pub owner: Ownership,
    /// Machine-readable reason key explaining the classification.
    pub reason: String,
    /// Confidence score for the classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Score>,
}

/// A single point in a source file (line plus multi-representation column).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourcePoint {
    /// 1-based line number.
    pub line: u32,
    /// Origin of column numbering (typically 0 or 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_origin: Option<u32>,
    /// Byte-offset column.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_byte: Option<u32>,
    /// Display-width column (accounts for tab stops, wide characters).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_display: Option<u32>,
    /// Column in the compiler's native unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_native: Option<u32>,
    /// Unit used by `column_native`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_native_unit: Option<ColumnUnit>,
}

/// A range between two [`SourcePoint`]s in a source file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceRange {
    /// Start of the range.
    pub start: SourcePoint,
    /// End of the range.
    pub end: SourcePoint,
    /// Whether the end point is inclusive or exclusive.
    pub boundary_semantics: BoundarySemantics,
}

impl OwnershipInfo {
    /// Creates a new ownership record with the given owner and reason.
    pub fn new(owner: Ownership, reason: impl Into<String>) -> Self {
        Self {
            owner,
            reason: reason.into(),
            confidence: None,
        }
    }
}

impl FileRef {
    /// Creates a [`FileRef`] from a raw path, inferring style and kind.
    pub fn new(path_raw: impl Into<String>) -> Self {
        let path_raw = path_raw.into();
        let (path_style, path_kind) = infer_path_metadata(&path_raw);
        Self {
            path_raw,
            display_path: None,
            uri: None,
            path_style: Some(path_style),
            path_kind: Some(path_kind),
            ownership: None,
            exists_at_capture: None,
        }
    }
}

impl SourcePoint {
    /// Creates a new source point at the given 1-based line and display column.
    pub fn new(line: u32, column: u32) -> Self {
        Self {
            line,
            column_origin: Some(1),
            column_byte: None,
            column_display: Some(column),
            column_native: Some(column),
            column_native_unit: Some(ColumnUnit::Display),
        }
    }
}

impl Location {
    /// Creates a caret (single-point) location at the given file, line, and column.
    pub fn caret(path: impl Into<String>, line: u32, column: u32, role: LocationRole) -> Self {
        let path = path.into();
        let anchor = SourcePoint::new(line, column);
        Self {
            id: synthetic_location_id(&path, &anchor, None),
            file: FileRef::new(path),
            anchor: Some(anchor),
            range: None,
            role,
            source_kind: LocationSourceKind::Caret,
            label: None,
            ownership_override: None,
            provenance_override: None,
            source_excerpt_ref: None,
        }
    }

    /// Extends this location with a range end point, converting it from caret to range.
    pub fn with_range_end(
        mut self,
        end_line: u32,
        end_column: u32,
        boundary_semantics: BoundarySemantics,
    ) -> Self {
        let start = self
            .anchor
            .clone()
            .unwrap_or_else(|| SourcePoint::new(end_line, end_column));
        let end = SourcePoint::new(end_line, end_column);
        self.id = synthetic_location_id(&self.file.path_raw, &start, Some(&end));
        self.range = Some(SourceRange {
            start,
            end,
            boundary_semantics,
        });
        self.source_kind = LocationSourceKind::Range;
        self
    }

    /// Sets the display path on the underlying file reference.
    pub fn with_display_path(mut self, display_path: impl Into<String>) -> Self {
        self.file.display_path = Some(display_path.into());
        self
    }

    /// Sets file-level ownership on this location.
    pub fn with_ownership(mut self, owner: Ownership, reason: impl Into<String>) -> Self {
        self.file.ownership = Some(OwnershipInfo::new(owner, reason));
        self
    }

    /// Replaces the raw file path and regenerates the location id.
    pub fn set_path_raw(&mut self, path: impl Into<String>) {
        self.file.path_raw = path.into();
        let start = self
            .anchor
            .as_ref()
            .or_else(|| self.range.as_ref().map(|range| &range.start));
        let end = self.range.as_ref().map(|range| &range.end);
        if let Some(start) = start {
            self.id = synthetic_location_id(&self.file.path_raw, start, end);
        }
    }

    /// Replaces the anchor point and updates the range start and location id.
    pub fn set_anchor(&mut self, line: u32, column: u32) {
        let anchor = SourcePoint::new(line, column);
        self.anchor = Some(anchor.clone());
        if let Some(range) = self.range.as_mut() {
            range.start = anchor.clone();
        }
        self.id = synthetic_location_id(
            &self.file.path_raw,
            &anchor,
            self.range.as_ref().map(|range| &range.end),
        );
    }

    /// Sets file-level ownership on the underlying [`FileRef`].
    pub fn set_ownership(&mut self, owner: Ownership, reason: impl Into<String>) {
        self.file.ownership = Some(OwnershipInfo::new(owner, reason));
    }

    /// Returns the raw file path.
    pub fn path_raw(&self) -> &str {
        &self.file.path_raw
    }

    /// Returns the display path, falling back to the raw path.
    pub fn display_path(&self) -> &str {
        self.file
            .display_path
            .as_deref()
            .unwrap_or(&self.file.path_raw)
    }

    /// Returns the 1-based line number from the anchor or range start, defaulting to 1.
    pub fn line(&self) -> u32 {
        self.anchor
            .as_ref()
            .map(|point| point.line)
            .or_else(|| self.range.as_ref().map(|range| range.start.line))
            .unwrap_or(1)
    }

    /// Returns the best-available column from the anchor or range start, defaulting to 1.
    pub fn column(&self) -> u32 {
        self.anchor
            .as_ref()
            .and_then(source_point_column)
            .or_else(|| {
                self.range
                    .as_ref()
                    .and_then(|range| source_point_column(&range.start))
            })
            .unwrap_or(1)
    }

    /// Returns the end line of the range, if present.
    pub fn end_line(&self) -> Option<u32> {
        self.range.as_ref().map(|range| range.end.line)
    }

    /// Returns the end column of the range, if present.
    pub fn end_column(&self) -> Option<u32> {
        self.range
            .as_ref()
            .and_then(|range| source_point_column(&range.end))
    }

    /// Returns the effective ownership, preferring the location override over the file default.
    pub fn ownership(&self) -> Option<&Ownership> {
        self.ownership_override
            .as_ref()
            .map(|info| &info.owner)
            .or_else(|| self.file.ownership.as_ref().map(|info| &info.owner))
    }
}

// --- Serde wire types for backward-compatible Location serialization ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LocationCurrent {
    pub id: String,
    pub file: FileRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<SourcePoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<SourceRange>,
    pub role: LocationRole,
    pub source_kind: LocationSourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership_override: Option<OwnershipInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance_override: Option<Provenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_excerpt_ref: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LocationLegacy {
    pub path: String,
    pub line: u32,
    pub column: u32,
    #[serde(default)]
    pub end_line: Option<u32>,
    #[serde(default)]
    pub end_column: Option<u32>,
    #[serde(default)]
    pub display_path: Option<String>,
    #[serde(default)]
    pub ownership: Option<Ownership>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum LocationWire {
    Current(Box<LocationCurrent>),
    Legacy(LocationLegacy),
}

impl From<Location> for LocationCurrent {
    fn from(location: Location) -> Self {
        Self {
            id: location.id,
            file: location.file,
            anchor: location.anchor,
            range: location.range,
            role: location.role,
            source_kind: location.source_kind,
            label: location.label,
            ownership_override: location.ownership_override,
            provenance_override: location.provenance_override,
            source_excerpt_ref: location.source_excerpt_ref,
        }
    }
}

impl From<LocationCurrent> for Location {
    fn from(location: LocationCurrent) -> Self {
        Self {
            id: location.id,
            file: location.file,
            anchor: location.anchor,
            range: location.range,
            role: location.role,
            source_kind: location.source_kind,
            label: location.label,
            ownership_override: location.ownership_override,
            provenance_override: location.provenance_override,
            source_excerpt_ref: location.source_excerpt_ref,
        }
    }
}

impl From<LocationLegacy> for Location {
    fn from(location: LocationLegacy) -> Self {
        let mut converted = Location::caret(
            location.path,
            location.line,
            location.column,
            LocationRole::Primary,
        );
        if let Some(display_path) = location.display_path {
            converted = converted.with_display_path(display_path);
        }
        if let Some(owner) = location.ownership {
            converted = converted.with_ownership(owner, ownership_reason_key(owner));
        }
        if let (Some(end_line), Some(end_column)) = (location.end_line, location.end_column) {
            converted = converted.with_range_end(end_line, end_column, BoundarySemantics::Unknown);
        }
        converted
    }
}

impl From<LocationWire> for Location {
    fn from(location: LocationWire) -> Self {
        match location {
            LocationWire::Current(location) => (*location).into(),
            LocationWire::Legacy(location) => location.into(),
        }
    }
}

impl Serialize for Location {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        LocationCurrent::from(self.clone()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Location {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LocationWire::deserialize(deserializer)?;
        Ok(wire.into())
    }
}

// --- Private helpers ---

pub(crate) fn ownership_reason_key(owner: Ownership) -> &'static str {
    match owner {
        Ownership::User => "user_workspace",
        Ownership::Vendor => "vendor_path",
        Ownership::System => "system_path",
        Ownership::Generated => "generated_path",
        Ownership::Tool => "tool_generated",
        Ownership::Unknown => "unknown",
    }
}

fn source_point_column(point: &SourcePoint) -> Option<u32> {
    point
        .column_display
        .or(point.column_native)
        .or(point.column_byte)
}

fn infer_path_metadata(path: &str) -> (PathStyle, PathKind) {
    if path.starts_with("file://") {
        return (PathStyle::Uri, PathKind::Absolute);
    }
    if path.starts_with('/') {
        return (PathStyle::Posix, PathKind::Absolute);
    }
    if path.contains(":\\") {
        return (PathStyle::Windows, PathKind::Absolute);
    }
    if path.starts_with('<') && path.ends_with('>') {
        return (PathStyle::Virtual, PathKind::Virtual);
    }
    (PathStyle::Posix, PathKind::Relative)
}

fn synthetic_location_id(path: &str, start: &SourcePoint, end: Option<&SourcePoint>) -> String {
    let end = end.unwrap_or(start);
    format!(
        "loc:{}:{}:{}:{}:{}",
        path,
        start.line,
        source_point_column(start).unwrap_or(1),
        end.line,
        source_point_column(end).unwrap_or(source_point_column(start).unwrap_or(1))
    )
}
