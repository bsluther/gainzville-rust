//! The client's query set: every question Swift can subscribe to or read once, and how each is
//! answered. Swift sees this set and nothing about the backing.
//!
//! Membership, not nesting: questions shared with the domain set (`FindEntryJoinById`, ...) are
//! listed here as plain variants, defined once in `gv-core`. Forest-backed questions are answered
//! from the version's in-memory forest (`ForestQuery`); the rest by the SQLite executor in the
//! version's read transaction (`ClientSqlExecutor`). Moving a question between the two groups is
//! the whole backing change (plus a `ForestQuery` or `QueryExecutor` impl if it lacks one).

// Members are named by type; the set's module (and gv-ffi's) must have them in scope.
use gv_core::queries::*;
use gv_ui::queries::*;

/// x-macro over the client set's declaration; see `gv_core::core_questions!`. `gv-ffi` expands it
/// into uniffi remote enums.
#[macro_export]
macro_rules! client_query_set {
    ($($callback:tt)*) => { $($callback)*! {
        /// A question the client can subscribe to or read once. The variant says *what* is
        /// asked, never how it is answered.
        pub enum ClientQuery -> ClientQueryResponse {
            forest {
                ChildSlots,
                WouldCreateCycle,
            }
            executor ClientSqlExecutor {
                // Client-only (SQLite)
                RootEntriesInInterval,
                // Shared with the domain set
                FindEntryById,
                FindEntryJoinById,
                FindAncestors,
                FindDescendants,
                FindActivityById,
                AllActivities,
                FindActivityTemplateRoot,
                FindAttributeById,
                AllAttributes,
                FindAttributesByOwner,
                FindAttributePairsForEntry,
                DistinctTextValuesForAttribute,
            }
        }
    } };
}

client_query_set!(gv_core::define_query_set);
