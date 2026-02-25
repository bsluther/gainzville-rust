use crate::models::entry::Entry;
use chrono::{DateTime, Utc};
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
}
