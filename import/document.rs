//! The day-document: the single pivot format of the import pipeline. It is
//! the MCP tool parameter, the persisted per-file artifact, and the loader
//! input — one shape, three roles (docs/import-design.md).
//!
//! Documents are name-based and denormalized: no UUIDs, no fractional
//! indices, no plan/actual mechanics. Everything the model is bad at is the
//! handler's job.

use chrono::{DateTime, NaiveDate, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Current document schema version. Bump on breaking shape changes so old
/// artifacts remain interpretable.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DayDocument {
    /// Document shape version (see [`SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Registered source name, e.g. "obsidian-training-log". Part of the
    /// deterministic-id key, so it must be stable across runs of one source.
    pub source: String,
    /// Vault-relative path of the source file. Part of the deterministic-id
    /// key and the file↔data attribution handle.
    pub source_file: String,
    /// sha256 of the source file at extraction time, if the extractor
    /// computed it ("sha256:<hex>"). Not verified by the loader in v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// The day this document describes. Derived from the filename by the
    /// extraction workflow, never by the model.
    pub date: NaiveDate,
    /// Extraction provenance, for artifact diffing across prompt iterations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_version: Option<String>,
    /// Root entries of the day, in source order.
    pub entries: Vec<DocEntry>,
    /// Every source line not consumed by an entry must be accounted for here
    /// — omissions are visible, never silent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skips: Vec<Skip>,
    /// Unresolvable items encountered during extraction. Appended to the
    /// workspace questions file; the affected lines also appear in `skips`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<String>,
    /// Written by the loader after a successful apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<Applied>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocEntry {
    /// Activity name, resolved against the registry (exact case-insensitive
    /// match or alias). Absent = anonymous entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
    /// Entry name. Required for anonymous scalars, forbidden on anonymous
    /// sequences (the sequence renders as its members).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Local wall-clock times as written in the log ("3:03pm", "15:03").
    /// The handler combines them with the document date and the configured
    /// timezone. Roots must carry a start or an end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    /// Duration like "40s", "25m", "1h30m".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    /// Attribute values by attribute name. Value shapes per config type:
    /// Text/Select = string; Numeric = number or {min,max}; Multiselect =
    /// [string]; Mass/Length = {value,unit} or {min,max,unit}.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, serde_json::Value>,
    /// Child entries in source order. A non-empty list makes this entry a
    /// sequence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DocEntry>,
    /// Force sequence-ness for an entry without children (rare).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Skip {
    /// Line range in the source file, e.g. "31-38" or "12".
    pub lines: String,
    pub reason: String,
}

/// Apply provenance, written back into the artifact by the loader.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Applied {
    pub imported_at: DateTime<Utc>,
    /// Ids of every entry this document's apply created or re-confirmed, in
    /// document order — the data side of file↔data attribution.
    pub entry_ids: Vec<Uuid>,
}

impl DayDocument {
    /// Strip fields that legitimately differ between extraction runs, for
    /// gold-set comparison (`gv-import check`): apply provenance and
    /// extraction provenance are volatile; content is not.
    pub fn normalized_for_check(&self) -> DayDocument {
        DayDocument {
            content_hash: None,
            model: None,
            prompt_version: None,
            applied: None,
            ..self.clone()
        }
    }
}
