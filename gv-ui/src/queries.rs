//! Client questions answered from the in-memory forest.
//!
//! Defined here rather than in `gv-core` because their answers are UI types (`ChildSlot`) or
//! screen rules, and `gv-core` stays domain-only. Membership in the client set, and the choice to
//! answer them from the forest, live in `gv_client::queries`. Each question's answer is a
//! `ForestQuery` impl: a pure function of the forest.

use chrono::{DateTime, Utc};
use gv_core::{forest::Forest, queries::Query, query_executor::ForestQuery};
use uuid::Uuid;

use crate::child_slots::{ChildSlot, child_slots};

/// A sequence's children laid out for display: drop, child, drop, ..., drop
/// (`gv_ui::child_slots`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChildSlots {
    pub parent_id: Uuid,
}

impl Query for ChildSlots {
    type Response = Vec<ChildSlot>;
}

impl ForestQuery for ChildSlots {
    fn answer(&self, forest: &Forest) -> Vec<ChildSlot> {
        child_slots(forest, self.parent_id)
    }
}

/// Would moving `entry_id` under `proposed_parent_id` create a cycle? (Drag-time check.)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WouldCreateCycle {
    pub entry_id: Uuid,
    pub proposed_parent_id: Uuid,
}

impl Query for WouldCreateCycle {
    type Response = bool;
}

impl ForestQuery for WouldCreateCycle {
    fn answer(&self, forest: &Forest) -> bool {
        forest.would_create_cycle(self.entry_id, self.proposed_parent_id)
    }
}

/// When a new log root on the day starting at `day_start` should be placed: now if the
/// day is today, else just after the day's last root, else noon.
///
/// Depends on the current time, which no data version captures: read it once at the
/// moment of use; a subscription would go stale between writes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SuggestedRootInsertionTime {
    pub day_start: DateTime<Utc>,
}

impl Query for SuggestedRootInsertionTime {
    type Response = DateTime<Utc>;
}

impl ForestQuery for SuggestedRootInsertionTime {
    fn answer(&self, forest: &Forest) -> DateTime<Utc> {
        forest.suggested_root_day_insertion_time(self.day_start)
    }
}
