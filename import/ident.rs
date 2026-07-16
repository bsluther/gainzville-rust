//! Deterministic entry identity: UUIDv5 over (source, source_file, date,
//! tree-path). Re-running an import re-derives the same ids, so re-runs
//! collide-and-skip instead of duplicating, and rollback/vault edits can't
//! orphan an id mapping — the mapping is a pure function.
//!
//! The tree-path is content-anchored, not positional: each component is
//! `<label>#<occurrence-among-same-label-siblings>`. Inserting an unrelated
//! sibling in a re-extraction does not shift the ids of existing entries;
//! only same-label reordering does (docs/import-design.md, D3). Labels are
//! **registry-canonical** (the caller resolves aliases before minting
//! components), so alias/casing wobble between extraction runs can't re-key
//! ids either.

use chrono::NaiveDate;
use uuid::Uuid;

/// Fixed namespace for gv-import entry ids (itself a UUIDv5 of the crate name
/// under the DNS namespace, generated once and frozen — the value matters
/// only in that it never changes).
pub const IMPORT_NAMESPACE: Uuid = uuid::uuid!("f1bd2f61-6b3b-5d2e-9f6a-3a7c9e4b8d10");

/// Normalize a label for identity: trimmed and lowercased (extraction-side
/// case/whitespace wobble must not change ids), with the path
/// metacharacters '/' and '#' replaced so a free-form name can never forge a
/// component boundary and collide with a genuinely nested path. Empty labels
/// degrade to "anonymous".
pub fn sanitize_label(s: &str) -> String {
    let cleaned = s.trim().to_lowercase().replace(['/', '#'], "_");
    if cleaned.is_empty() {
        "anonymous".to_string()
    } else {
        cleaned
    }
}

/// Tree-path components for a sibling list of (already canonical, sanitized)
/// labels, in order: `<label>#<occurrence among equal labels before it>`.
pub fn components(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let occurrence = labels[..i].iter().filter(|l| *l == label).count();
            format!("{label}#{occurrence}")
        })
        .collect()
}

/// Deterministic id for the entry at `tree_path` (root-to-leaf components
/// joined with '/').
pub fn entry_id(source: &str, source_file: &str, date: NaiveDate, tree_path: &str) -> Uuid {
    let key = format!("{source}\n{source_file}\n{date}\n{tree_path}");
    Uuid::new_v5(&IMPORT_NAMESPACE, key.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn sanitize_normalizes_and_defuses_metacharacters() {
        assert_eq!(sanitize_label("  Bench Press "), "bench press");
        // A name that would otherwise forge "a#0/b#0" — a nested-looking path.
        assert_eq!(sanitize_label("a#0/b"), "a_0_b");
        assert_eq!(sanitize_label("  "), "anonymous");
    }

    #[test]
    fn unrelated_sibling_insertion_preserves_components() {
        let before = vec!["autobelay".to_string(), "autobelay".to_string()];
        let after = vec![
            "autobelay".to_string(),
            "stretch".to_string(),
            "autobelay".to_string(),
        ];
        let before_c = components(&before);
        let after_c = components(&after);
        // The two autobelays keep their components despite the insertion.
        assert_eq!(before_c[0], after_c[0]);
        assert_eq!(before_c[1], after_c[2]);
    }
}
