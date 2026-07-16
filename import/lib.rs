//! LLM-driven import of unstructured logs into Gainzville.
//!
//! The pivot format is the [`document::DayDocument`]: a denormalized,
//! name-based description of one source file's day. The model produces
//! documents; [`import::Importer::import_day`] resolves names against the
//! registry, mints deterministic ids, derives positions, validates, and
//! applies through the ordinary `Action` interface. The same handler backs the
//! MCP tool surface (`bin/gv_import.rs`) and any future batch loader.
//!
//! Design record: `docs/import-design.md`.

pub mod document;
pub mod ident;
pub mod import;
pub mod registry;
pub mod temporal;
pub mod values;
pub mod workspace;
