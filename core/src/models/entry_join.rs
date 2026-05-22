use super::activity::Activity;
use super::entry::Entry;
use crate::models::attribute_pair::AttributePair;

/// Domain model representing an Entry with all its joined relations.
/// Currently includes the optional Activity and Attribute-Value pairs.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryJoin {
    pub entry: Entry,
    pub activity: Option<Activity>,
    pub attributes: Vec<AttributePair>,
    pub display_name: String,
}

fn compute_display_name(entry: &Entry, activity: Option<&Activity>) -> String {
    if let Some(name) = entry.name.as_deref() {
        if !name.is_empty() {
            return name.to_string();
        }
    }
    if let Some(activity) = activity {
        return activity.name.to_string();
    }
    "Unnamed".to_string()
}

impl EntryJoin {
    pub fn is_sequence(&self) -> bool {
        self.entry.is_sequence
    }

    /// Stitch an already-parsed entry, optional activity, and attribute
    /// pairs into an `EntryJoin`. Computes `display_name` via the
    /// canonical fallback rule.
    pub fn new(entry: Entry, activity: Option<Activity>, attributes: Vec<AttributePair>) -> Self {
        let display_name = compute_display_name(&entry, activity.as_ref());
        EntryJoin {
            entry,
            activity,
            attributes,
            display_name,
        }
    }
}
