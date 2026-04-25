use crate::models::entry::Entry;
use chrono::{DateTime, Duration, Utc};
use std::ops::Range;
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
            // - If canononical_instant + 1 min is the interval, use that.
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
