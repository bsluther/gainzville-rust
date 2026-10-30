//! Client questions answered from the in-memory forest.
//!
//! Defined here rather than in `gv-core` because their answers are UI types (`ChildSlot`) or
//! screen rules, and `gv-core` stays domain-only. Membership in the client set, and the choice to
//! answer them from the forest, live in `gv_client::queries`. Each question's answer is a
//! `ForestQuery` impl: a pure function of the forest.

use gv_core::{forest::Forest, query_executor::ForestQuery};
use uuid::Uuid;

use crate::child_slots::{ChildSlot, child_slots};

/// x-macro over the questions defined in `gv-ui`; see `gv_core::core_questions!`. `gv-ffi`
/// expands it into uniffi remote records.
#[macro_export]
macro_rules! ui_questions {
    ($($callback:tt)*) => { $($callback)*! {
        /// A sequence's children laid out for display: drop, child, drop, ..., drop
        /// (`gv_ui::child_slots`).
        pub struct ChildSlots { pub parent_id: Uuid } => Vec<ChildSlot>;
        /// Would moving `entry_id` under `proposed_parent_id` create a cycle? (Drag-time check.)
        pub struct WouldCreateCycle {
            pub entry_id: Uuid,
            pub proposed_parent_id: Uuid,
        } => bool;
    } };
}

ui_questions!(gv_core::define_questions);

impl ForestQuery for ChildSlots {
    fn answer(&self, forest: &Forest) -> Vec<ChildSlot> {
        child_slots(forest, self.parent_id)
    }
}

impl ForestQuery for WouldCreateCycle {
    fn answer(&self, forest: &Forest) -> bool {
        forest.would_create_cycle(self.entry_id, self.proposed_parent_id)
    }
}
