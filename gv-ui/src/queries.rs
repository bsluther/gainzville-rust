//! Client questions answered from the in-memory forest.
//!
//! Defined here rather than in `gv-core` because their answers are UI types (`ChildSlot`) or
//! screen rules, and `gv-core` stays domain-only. Membership in the client set, and the choice to
//! answer them from the forest, live in `gv_client::queries`. Each answer is a thin
//! `ForestExecutor` impl over a pure function of the forest.

use chrono::{DateTime, Utc};
use gv_core::{
    error::Result,
    queries::Query,
    query_executor::{ForestExecutor, QueryExecutor},
};
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

impl QueryExecutor<ChildSlots> for ForestExecutor<'_> {
    async fn execute(&mut self, query: ChildSlots) -> Result<Vec<ChildSlot>> {
        Ok(child_slots(self.forest, query.parent_id))
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

impl QueryExecutor<WouldCreateCycle> for ForestExecutor<'_> {
    async fn execute(&mut self, query: WouldCreateCycle) -> Result<bool> {
        Ok(self
            .forest
            .would_create_cycle(query.entry_id, query.proposed_parent_id))
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

impl QueryExecutor<SuggestedRootInsertionTime> for ForestExecutor<'_> {
    async fn execute(&mut self, query: SuggestedRootInsertionTime) -> Result<DateTime<Utc>> {
        Ok(self
            .forest
            .suggested_root_day_insertion_time(query.day_start))
    }
}
