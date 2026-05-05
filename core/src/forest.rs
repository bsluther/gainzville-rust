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

    /// Get all root entries in the forest.
    pub fn roots(&self) -> Vec<&Entry> {
        let mut roots: Vec<&Entry> = self
            .data()
            .iter()
            .filter(|e| e.parent_id().is_none())
            .collect();
        roots.sort_by_key(|e| e.temporal.canonical_instant());
        roots
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

    /// Get the position between two entries in a sequence.
    pub fn position_between(
        &self,
        parent_id: Uuid,
        pred_id: Option<Uuid>,
        succ_id: Option<Uuid>,
    ) -> Position {
        assert!(
            self.entry(parent_id).is_some_and(|p| p.is_sequence),
            "parent_id must correspond to a sequence"
        );

        // TODO: assert that pred/succ are adjacent.
        let pred_fi = pred_id.map(|id| {
            self.entry(id)
                .and_then(|e| e.frac_index())
                .expect("pred_id must correspond to an entry with a defined fractional index")
        });
        let succ_fi = succ_id.map(|id| {
            self.entry(id)
                .and_then(|e| e.frac_index())
                .expect("succ_id must correspond to an entry with a defined fractional index")
        });

        let frac_index = match (pred_fi, succ_fi) {
            (None, None) => FractionalIndex::default(),
            (Some(pred), None) => FractionalIndex::new_after(pred),
            (None, Some(succ)) => FractionalIndex::new_before(succ),
            (Some(pred), Some(succ)) => {
                FractionalIndex::new_between(pred, succ).expect("pred must precede succ")
            }
        };

        Position {
            parent_id,
            frac_index,
        }
    }

    /// Get the position immediately succeeding the last child of a sequence.
    pub fn position_after_children(&self, parent_id: Uuid) -> Option<Position> {
        if let Some(parent) = self.entry(parent_id) {
            assert!(
                parent.is_sequence,
                "provided parent_id must correspond to a sequence, found a scalar entry"
            );
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
}
