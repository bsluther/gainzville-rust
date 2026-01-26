use chrono::NaiveDateTime;
use gv_core::models::entry::Entry;
use rkyv::{Archive, Serialize};
use size_of::SizeOf;
use uuid::Uuid;

#[derive(
    Clone,
    Default,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    // SizeOf,
    Archive,
    Serialize,
    rkyv::Deserialize,
    serde::Deserialize,
    feldera_macros::IsNone,
)]
#[archive_attr(derive(Ord, Eq, PartialEq, PartialOrd))]
pub struct Id(pub Uuid);

impl SizeOf for Id {
    fn size_of_children(&self, _context: &mut size_of::Context) {}
}

#[derive(
    Clone,
    Default,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    SizeOf,
    Archive,
    Serialize,
    rkyv::Deserialize,
    serde::Deserialize,
    feldera_macros::IsNone,
)]
#[archive_attr(derive(Ord, Eq, PartialEq, PartialOrd))]
pub struct IvmEntry {
    pub id: Id,
    pub activity_id: Option<Id>,
    pub owner_id: Id,
    pub parent_id: Option<Id>,
    pub frac_index: Option<String>,
    pub is_template: bool,
    pub display_as_sets: bool,
    pub is_sequence: bool,
    pub start_time: Option<NaiveDateTime>,
    pub end_time: Option<NaiveDateTime>,
    pub duration_ms: Option<i64>,
}

impl From<Entry> for IvmEntry {
    fn from(e: Entry) -> Self {
        IvmEntry {
            id: Id(e.id),
            activity_id: e.activity_id.map(Id),
            owner_id: Id(e.owner_id),
            parent_id: e.parent_id().map(Id),
            frac_index: e.frac_index().map(|f| f.to_string()),
            is_template: e.is_template,
            display_as_sets: e.display_as_sets,
            is_sequence: e.is_sequence,
            start_time: e.temporal.start().map(|t| t.naive_utc()),
            end_time: e.temporal.start().map(|t| t.naive_utc()),
            duration_ms: e.temporal.duration().map(|d| d as i64),
        }
    }
}
