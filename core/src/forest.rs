use crate::models::entry::{Entry, Position};
use chrono::{DateTime, Duration, Utc};
use fractional_index::FractionalIndex;
use std::{collections::HashSet, ops::Range};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct Forest(Vec<Entry>);

impl From<Vec<Entry>> for Forest {
    fn from(entries: Vec<Entry>) -> Self {
        Forest(entries)
    }
}

impl Forest {
    fn data(&self) -> &[Entry] {
        &self.0
    }

    /// Get an entry in the forest by id.
    pub fn entry(&self, id: Uuid) -> Option<&Entry> {
        self.data().iter().find(|e| e.id == id)
    }

    /// All parentless entries, templates included. Internal: structural root
    /// set used for tree validation. Public callers want `roots` (log) or
    /// `template_root`.
    fn all_roots(&self) -> Vec<&Entry> {
        self.data()
            .iter()
            .filter(|e| e.parent_id().is_none())
            .collect()
    }

    /// Get all log root entries in the forest, sorted by canonical instant.
    /// Template entries are excluded explicitly: they are structurally rootable
    /// (no parent) but live outside the timeline, so they are reached via
    /// `template_root`, not the log root list.
    pub fn roots(&self) -> Vec<&Entry> {
        let mut roots: Vec<&Entry> = self
            .all_roots()
            .into_iter()
            .filter(|e| !e.is_template)
            .collect();
        roots.sort_by_key(|e| e.temporal.canonical_instant());
        roots
    }

    /// The root template entry for an activity: the parentless template entry
    /// whose `activity_id` matches. `CreateActivity` guarantees exactly one.
    pub fn template_root(&self, activity_id: Uuid) -> Option<&Entry> {
        self.data().iter().find(|e| {
            e.is_template && e.parent_id().is_none() && e.activity_id == Some(activity_id)
        })
    }

    /// Get all root entries whose start time falls within the provided interval, sorted by time.
    /// Entries without a definite start
    pub fn roots_in(&self, interval: Range<DateTime<Utc>>) -> Vec<&Entry> {
        let mut roots: Vec<_> = self
            .roots()
            .into_iter()
            .filter(|e| {
                e.temporal
                    .canonical_instant()
                    .is_some_and(|t| interval.contains(&t))
            })
            .collect();
        roots.sort_by_key(|e| e.temporal.canonical_instant());
        roots
    }

    /// Get the direct children of the provided parent_id, sorted by fractional index.
    pub fn children(&self, parent_id: Uuid) -> Vec<&Entry> {
        let mut children: Vec<_> = self
            .data()
            .iter()
            .filter(|e| e.parent_id().is_some_and(|id| id == parent_id))
            .collect();
        children.sort_by(|a, b| {
            a.frac_index()
                .expect("child entries must have a defined fractional index")
                .cmp(
                    b.frac_index()
                        .expect("child entries must have a defined fractional index"),
                )
        });
        children
    }

    pub fn siblngs(&self, entry_id: Uuid) -> Vec<&Entry> {
        let Some(parent_id) = self.entry(entry_id).and_then(|e| e.parent_id()) else {
            return Vec::new();
        };
        self.data()
            .iter()
            .filter(|e| e.parent_id().is_some_and(|id| id == parent_id))
            .collect()
    }

    /// Get the ancestors of an entry from immediate parent up to the root.
    pub fn ancestors(&self, entry_id: Uuid) -> Vec<&Entry> {
        let mut ancestors = Vec::new();
        let Some(start) = self.entry(entry_id) else {
            return ancestors;
        };
        let mut current_parent = start.parent_id();
        while let Some(parent_id) = current_parent {
            let Some(parent) = self.entry(parent_id) else {
                break;
            };
            ancestors.push(parent);
            current_parent = parent.parent_id();
        }
        ancestors
    }

    fn descendants_recursive<'a>(&'a self, entry_id: Uuid, acc: &mut Vec<&'a Entry>) {
        for child in self.children(entry_id) {
            acc.push(child);
            self.descendants_recursive(child.id, acc);
        }
    }

    /// Get all descendants of an entry, including the entry itself.
    pub fn descendants(&self, entry_id: Uuid) -> Vec<&Entry> {
        let mut result: Vec<&Entry> = self.entry(entry_id).into_iter().collect();
        if !result.is_empty() {
            self.descendants_recursive(entry_id, &mut result);
        }
        let set: HashSet<_> = result.iter().map(|e| e.id).collect();
        assert_eq!(
            result.len(),
            set.len(),
            "entry descendants should not contain duplicates"
        );
        result
    }

    /// Check if target is a descendant of entry.
    pub fn is_descendant_of(&self, target_id: Uuid, entry_id: Uuid) -> bool {
        self.ancestors(target_id)
            .into_iter()
            .any(|a| a.id == entry_id)
    }

    /// Returns true if moving `entry_id` under `proposed_parent_id` would create a cycle.
    pub fn would_create_cycle(&self, entry_id: Uuid, proposed_parent_id: Uuid) -> bool {
        entry_id == proposed_parent_id || self.is_descendant_of(proposed_parent_id, entry_id)
    }

    /// Get the position between two entries in a sequence. Returns `None` when
    /// the request can't be satisfied against the current forest — the parent is
    /// missing or not a sequence, or a given `pred_id`/`succ_id` is missing or
    /// lacks a fractional index. These can occur transiently during UI
    /// re-render races (e.g. the parent's cardinality flips while drop-target
    /// views from the prior state are still mounted), so this is total rather
    /// than panicking — it's a query over live, changing data, and a panic here
    /// would cross the FFI boundary and crash the app.
    pub fn position_between(
        &self,
        parent_id: Uuid,
        pred_id: Option<Uuid>,
        succ_id: Option<Uuid>,
    ) -> Option<Position> {
        if !self.entry(parent_id).is_some_and(|p| p.is_sequence) {
            return None;
        }

        // TODO: assert that pred/succ are adjacent.
        let pred_fi = match pred_id {
            Some(id) => Some(self.entry(id).and_then(|e| e.frac_index())?),
            None => None,
        };
        let succ_fi = match succ_id {
            Some(id) => Some(self.entry(id).and_then(|e| e.frac_index())?),
            None => None,
        };

        let frac_index = match (pred_fi, succ_fi) {
            (None, None) => FractionalIndex::default(),
            (Some(pred), None) => FractionalIndex::new_after(pred),
            (None, Some(succ)) => FractionalIndex::new_before(succ),
            // `new_between` yields None only if pred doesn't precede succ — a
            // stale pred/succ pairing during a race; treat as no valid position.
            (Some(pred), Some(succ)) => FractionalIndex::new_between(pred, succ)?,
        };

        Some(Position {
            parent_id,
            frac_index,
        })
    }

    /// Get the position immediately succeeding the last child of a sequence.
    pub fn position_after_children(&self, parent_id: Uuid) -> Option<Position> {
        if let Some(parent) = self.entry(parent_id) {
            // Total over live data (see `position_between`): a non-sequence
            // parent yields no append position rather than panicking.
            if !parent.is_sequence {
                return None;
            }
            let children = self.children(parent_id);
            return children
                .last()
                .map(|c| {
                    c.frac_index()
                        .expect("child entries must have a fractional index")
                        .clone()
                })
                .map(|fi| FractionalIndex::new_after(&fi))
                .or_else(|| Some(FractionalIndex::default()))
                .map(|fi| Position {
                    parent_id,
                    frac_index: fi,
                });
        };

        return None;
    }

    /// Get a suggested start time for a newly created root-level entry.
    pub fn suggested_root_day_insertion_time(&self, day_start: DateTime<Utc>) -> DateTime<Utc> {
        let day_end = day_start + Duration::days(1);
        let interval = day_start..day_end;
        let now = Utc::now();
        if interval.contains(&now) {
            return now;
        }
        let noon = day_start + Duration::hours(12);

        let last_entry = self.roots_in(interval).last().copied();
        if let Some(entry) = last_entry {
            let inferred_end = entry.temporal.infer_end();
            assert!(
                inferred_end.is_some(),
                "root entries must have a canonical instant"
            );

            // Pick a time in the interval:
            // - If inferred_end is in the interval, use that.
            // - If canonical_instant + 1 min is the interval, use that.
            // - Otherwise, pick the latest value in the interval.
            // - Repeating this process should result in new entries all being created at the last
            // millisecond of the day.
            // TODO: use a property-based test for the above property.
            let base = inferred_end
                .filter(|&end| end < day_end)
                .or_else(|| entry.temporal.canonical_instant())
                .unwrap();
            return (base + Duration::minutes(1))
                .clamp(day_start, day_end - Duration::milliseconds(1));
        }

        // If inserting into a day that is not today with no entries, default to noon.
        noon
    }

    /// Returns true if the forest forms a single tree that contains at least one entry.
    pub fn is_tree(&self) -> bool {
        self.tree_root().is_some()
    }

    /// Returns the single root if the forest is one connected tree, else `None`.
    /// In general a (finite) graph is a tree iff it is connected and has n-1 edges.
    /// Because each entry has at most one parent, if the graph is connected then it has either
    /// n-1 or n edges.
    /// Therefore, connected && exactly one root => n-1 edges => graph is a tree.
    /// Additionally enforces that the tree is non-trivial - must contain at least one entry.
    pub fn tree_root(&self) -> Option<&Entry> {
        // Template trees are validated here too, so consider all structural
        // roots (templates included), not just log roots.
        let roots = self.all_roots();
        // Return None if there is not exactly one root.
        let [root] = roots.as_slice() else {
            return None;
        };
        // Connected iff every entry is reachable downward from the unique root.
        (self.descendants(root.id).len() == self.data().len()).then_some(*root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::entry::Temporal;

    fn root_entry(is_template: bool, activity_id: Option<Uuid>) -> Entry {
        Entry {
            id: Uuid::new_v4(),
            activity_id,
            owner_id: Uuid::new_v4(),
            name: None,
            position: None,
            is_template,
            display_as_sets: false,
            is_sequence: true,
            is_complete: false,
            temporal: Temporal::None,
        }
    }

    #[test]
    fn roots_excludes_templates() {
        let log = root_entry(false, None);
        let template = root_entry(true, Some(Uuid::new_v4()));
        let forest = Forest::from(vec![log.clone(), template]);
        let roots = forest.roots();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].id, log.id);
    }

    #[test]
    fn template_root_matches_activity() {
        let activity_id = Uuid::new_v4();
        let template = root_entry(true, Some(activity_id));
        let other_template = root_entry(true, Some(Uuid::new_v4()));
        let log = root_entry(false, None);
        let forest = Forest::from(vec![template.clone(), other_template, log]);

        assert_eq!(
            forest.template_root(activity_id).map(|e| e.id),
            Some(template.id)
        );
        assert!(forest.template_root(Uuid::new_v4()).is_none());
    }

    #[test]
    fn position_queries_are_total_on_non_sequence_parent() {
        // Regression: a UI re-render race can call these against a parent whose
        // cardinality just flipped to scalar. They must return None, not panic
        // (a panic would cross the FFI boundary and crash the app).
        let scalar = root_entry(false, None); // root_entry sets is_sequence = true
        let mut scalar = scalar;
        scalar.is_sequence = false;
        let forest = Forest::from(vec![scalar.clone()]);

        assert!(forest.position_between(scalar.id, None, None).is_none());
        assert!(forest.position_after_children(scalar.id).is_none());
        // Unknown parent id is also handled.
        assert!(
            forest
                .position_between(Uuid::new_v4(), None, None)
                .is_none()
        );
    }

    #[test]
    fn position_between_empty_sequence_returns_default() {
        let seq = root_entry(false, None); // is_sequence = true
        let forest = Forest::from(vec![seq.clone()]);
        assert!(forest.position_between(seq.id, None, None).is_some());
    }

    #[test]
    fn tree_root_still_finds_template_root() {
        // A template-only forest must still validate as a tree (used by
        // create_activity), even though `roots` excludes templates.
        let template = root_entry(true, Some(Uuid::new_v4()));
        let forest = Forest::from(vec![template.clone()]);
        assert_eq!(forest.tree_root().map(|e| e.id), Some(template.id));
    }
}
