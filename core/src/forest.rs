use crate::models::entry::Entry;
use std::cmp::Ordering;
use uuid::Uuid;

/// Find an entry by id.
pub fn find_by_id(id: Uuid, entries: &[Entry]) -> Option<&Entry> {
    entries.iter().find(|e| e.id == id)
}

/// Returns the root entries (entries with no parent entry) ordered by time;
/// entries without a definite time are placed at the start.
pub fn roots(entries: &[Entry]) -> Vec<&Entry> {
    let mut roots: Vec<_> = entries.iter().filter(|e| e.parent_id().is_none()).collect();
    roots.sort_by(|a, b| match (a.temporal.start(), b.temporal.start()) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(a), Some(b)) => a.cmp(&b),
    });
    roots
}

/// Returns the children of the entry with the given id, ordered by fractional index.
pub fn children_of(parent_id: Uuid, entries: &[Entry]) -> Vec<&Entry> {
    let mut children: Vec<_> = entries
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
