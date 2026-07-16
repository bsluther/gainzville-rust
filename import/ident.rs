//! Deterministic entry identity: UUIDv5 over (source, source_file, date,
//! tree-path). Re-running an import re-derives the same ids, so re-runs
//! collide-and-skip instead of duplicating, and rollback/vault edits can't
//! orphan an id mapping — the mapping is a pure function.
//!
//! The tree-path is content-anchored, not positional: each component is
//! `<label>#<occurrence-among-same-label-siblings>`. Inserting an unrelated
//! sibling in a re-extraction does not shift the ids of existing entries;
//! only same-label reordering does (docs/import-design.md, D3).

use crate::document::DocEntry;
use chrono::NaiveDate;
use uuid::Uuid;

/// Fixed namespace for gv-import entry ids (itself a UUIDv5 of the crate name
/// under the DNS namespace, generated once and frozen — the value matters
/// only in that it never changes).
pub const IMPORT_NAMESPACE: Uuid = uuid::uuid!("f1bd2f61-6b3b-5d2e-9f6a-3a7c9e4b8d10");

/// The label a doc entry contributes to its tree-path component: activity
/// name, else entry name, else "anonymous" — lowercased so extraction-side
/// case wobble doesn't change identity.
pub fn entry_label(entry: &DocEntry) -> String {
    entry
        .activity
        .as_deref()
        .or(entry.name.as_deref())
        .unwrap_or("anonymous")
        .to_lowercase()
}

/// Path component for `entry`, given how many earlier siblings share its
/// label.
pub fn path_component(entry: &DocEntry, occurrence: usize) -> String {
    format!("{}#{}", entry_label(entry), occurrence)
}

/// Deterministic id for the entry at `tree_path` (root-to-leaf components
/// joined with '/').
pub fn entry_id(source: &str, source_file: &str, date: NaiveDate, tree_path: &str) -> Uuid {
    let key = format!("{source}\n{source_file}\n{date}\n{tree_path}");
    Uuid::new_v5(&IMPORT_NAMESPACE, key.as_bytes())
}

/// Tree-path components for a sibling list, in order: each entry paired with
/// its occurrence index among same-label predecessors.
pub fn sibling_components(siblings: &[DocEntry]) -> Vec<String> {
    let mut components = Vec::with_capacity(siblings.len());
    for (i, entry) in siblings.iter().enumerate() {
        let label = entry_label(entry);
        let occurrence = siblings[..i]
            .iter()
            .filter(|s| entry_label(s) == label)
            .count();
        components.push(path_component(entry, occurrence));
    }
    components
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_entry(activity: Option<&str>, name: Option<&str>) -> DocEntry {
        DocEntry {
            activity: activity.map(str::to_string),
            name: name.map(str::to_string),
            start: None,
            end: None,
            duration: None,
            attributes: Default::default(),
            children: vec![],
            sequence: None,
        }
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        let date = NaiveDate::from_ymd_opt(2026, 2, 24).unwrap();
        let a = entry_id("src", "2026-02/2026-02-24.md", date, "climbing#0/autobelay#0");
        let b = entry_id("src", "2026-02/2026-02-24.md", date, "climbing#0/autobelay#0");
        let c = entry_id("src", "2026-02/2026-02-24.md", date, "climbing#0/autobelay#1");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn unrelated_sibling_insertion_preserves_components() {
        let before = vec![
            doc_entry(Some("Autobelay"), None),
            doc_entry(Some("Autobelay"), None),
        ];
        let after = vec![
            doc_entry(Some("Autobelay"), None),
            doc_entry(Some("Stretch"), None),
            doc_entry(Some("Autobelay"), None),
        ];
        let before_c = sibling_components(&before);
        let after_c = sibling_components(&after);
        // The two autobelays keep their components despite the insertion.
        assert_eq!(before_c[0], after_c[0]);
        assert_eq!(before_c[1], after_c[2]);
    }
}
