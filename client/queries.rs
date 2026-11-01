//! The client's query set: every question Swift can subscribe to or read once, and how each is
//! answered. Swift sees this set and nothing about the backing.
//!
//! Membership, not nesting: questions shared with the domain set (`FindEntryJoinById`, ...) are
//! listed here as plain variants, defined once in `gv-core`. Forest-backed questions are answered
//! from the version's in-memory forest (`ForestQuery`); the rest by the SQLite executor in the
//! version's read transaction. [`ClientQuery::is_forest_backed`] is the backing decision; moving a
//! question means moving its arm there and in the two dispatch functions below it (plus a
//! `ForestQuery` or `QueryExecutor` impl if it lacks one).

use chrono::{DateTime, Utc};
use gv_core::{
    error::Result,
    forest::Forest,
    models::{
        activity::Activity, attribute::Attribute, attribute_pair::AttributePair, entry::Entry,
        entry_join::EntryJoin,
    },
    queries::{
        AllActivities, AllAttributes, DistinctTextValuesForAttribute, FindActivityById,
        FindActivityTemplateRoot, FindAncestors, FindAttributeById, FindAttributePairsForEntry,
        FindAttributesByOwner, FindDescendants, FindEntryById, FindEntryJoinById,
        RootEntriesInInterval,
    },
    query_executor::{ForestQuery, QueryExecutor},
};
use gv_sql::sqlite::SqliteQueryExecutor;
use gv_ui::{
    ChildSlot,
    queries::{ChildSlots, SuggestedRootInsertionTime, WouldCreateCycle},
};
use uuid::Uuid;

/// A question the client can subscribe to or read once. The variant says *what* is
/// asked, never how it is answered.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientQuery {
    // Forest-backed
    ChildSlots(ChildSlots),
    WouldCreateCycle(WouldCreateCycle),
    SuggestedRootInsertionTime(SuggestedRootInsertionTime),
    // Client-only (SQLite)
    RootEntriesInInterval(RootEntriesInInterval),
    // Shared with the domain set
    FindEntryById(FindEntryById),
    FindEntryJoinById(FindEntryJoinById),
    FindAncestors(FindAncestors),
    FindDescendants(FindDescendants),
    FindActivityById(FindActivityById),
    AllActivities(AllActivities),
    FindActivityTemplateRoot(FindActivityTemplateRoot),
    FindAttributeById(FindAttributeById),
    AllAttributes(AllAttributes),
    FindAttributesByOwner(FindAttributesByOwner),
    FindAttributePairsForEntry(FindAttributePairsForEntry),
    DistinctTextValuesForAttribute(DistinctTextValuesForAttribute),
}

/// The answer to a [`ClientQuery`]: one variant per member.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientQueryResponse {
    // Forest-backed
    ChildSlots(Vec<ChildSlot>),
    WouldCreateCycle(bool),
    SuggestedRootInsertionTime(DateTime<Utc>),
    // Client-only (SQLite)
    RootEntriesInInterval(Vec<Entry>),
    // Shared with the domain set
    FindEntryById(Option<Entry>),
    FindEntryJoinById(Option<EntryJoin>),
    FindAncestors(Vec<Uuid>),
    FindDescendants(Vec<Entry>),
    FindActivityById(Option<Activity>),
    AllActivities(Vec<Activity>),
    FindActivityTemplateRoot(Option<Entry>),
    FindAttributeById(Option<Attribute>),
    AllAttributes(Vec<Attribute>),
    FindAttributesByOwner(Vec<Attribute>),
    FindAttributePairsForEntry(Vec<AttributePair>),
    DistinctTextValuesForAttribute(Vec<String>),
}

impl ClientQuery {
    /// The backing decision: whether this question is answered from the in-memory forest
    /// (otherwise by SQLite). Swift can't see this choice.
    pub fn is_forest_backed(&self) -> bool {
        match self {
            ClientQuery::ChildSlots(_)
            | ClientQuery::WouldCreateCycle(_)
            | ClientQuery::SuggestedRootInsertionTime(_) => true,

            ClientQuery::RootEntriesInInterval(_)
            | ClientQuery::FindEntryById(_)
            | ClientQuery::FindEntryJoinById(_)
            | ClientQuery::FindAncestors(_)
            | ClientQuery::FindDescendants(_)
            | ClientQuery::FindActivityById(_)
            | ClientQuery::AllActivities(_)
            | ClientQuery::FindActivityTemplateRoot(_)
            | ClientQuery::FindAttributeById(_)
            | ClientQuery::AllAttributes(_)
            | ClientQuery::FindAttributesByOwner(_)
            | ClientQuery::FindAttributePairsForEntry(_)
            | ClientQuery::DistinctTextValuesForAttribute(_) => false,
        }
    }

    /// Answer a forest-backed question; `None` for SQLite-backed ones. No wildcard arm, so a new
    /// question can't be missed here.
    pub fn answer_from_forest(&self, forest: &Forest) -> Option<ClientQueryResponse> {
        use ClientQueryResponse as R;
        match self {
            ClientQuery::ChildSlots(q) => Some(R::ChildSlots(q.answer(forest))),
            ClientQuery::WouldCreateCycle(q) => Some(R::WouldCreateCycle(q.answer(forest))),
            ClientQuery::SuggestedRootInsertionTime(q) => {
                Some(R::SuggestedRootInsertionTime(q.answer(forest)))
            }

            ClientQuery::RootEntriesInInterval(_)
            | ClientQuery::FindEntryById(_)
            | ClientQuery::FindEntryJoinById(_)
            | ClientQuery::FindAncestors(_)
            | ClientQuery::FindDescendants(_)
            | ClientQuery::FindActivityById(_)
            | ClientQuery::AllActivities(_)
            | ClientQuery::FindActivityTemplateRoot(_)
            | ClientQuery::FindAttributeById(_)
            | ClientQuery::AllAttributes(_)
            | ClientQuery::FindAttributesByOwner(_)
            | ClientQuery::FindAttributePairsForEntry(_)
            | ClientQuery::DistinctTextValuesForAttribute(_) => None,
        }
    }

    /// Answer a SQLite-backed question; `None` for forest-backed ones. No wildcard arm, so a new
    /// question can't be missed here.
    pub async fn execute(
        self,
        exec: &mut SqliteQueryExecutor<'_>,
    ) -> Option<Result<ClientQueryResponse>> {
        use ClientQueryResponse as R;
        match self {
            ClientQuery::ChildSlots(_)
            | ClientQuery::WouldCreateCycle(_)
            | ClientQuery::SuggestedRootInsertionTime(_) => None,

            ClientQuery::RootEntriesInInterval(q) => {
                Some(exec.execute(q).await.map(R::RootEntriesInInterval))
            }
            ClientQuery::FindEntryById(q) => Some(exec.execute(q).await.map(R::FindEntryById)),
            ClientQuery::FindEntryJoinById(q) => {
                Some(exec.execute(q).await.map(R::FindEntryJoinById))
            }
            ClientQuery::FindAncestors(q) => Some(exec.execute(q).await.map(R::FindAncestors)),
            ClientQuery::FindDescendants(q) => Some(exec.execute(q).await.map(R::FindDescendants)),
            ClientQuery::FindActivityById(q) => {
                Some(exec.execute(q).await.map(R::FindActivityById))
            }
            ClientQuery::AllActivities(q) => Some(exec.execute(q).await.map(R::AllActivities)),
            ClientQuery::FindActivityTemplateRoot(q) => {
                Some(exec.execute(q).await.map(R::FindActivityTemplateRoot))
            }
            ClientQuery::FindAttributeById(q) => {
                Some(exec.execute(q).await.map(R::FindAttributeById))
            }
            ClientQuery::AllAttributes(q) => Some(exec.execute(q).await.map(R::AllAttributes)),
            ClientQuery::FindAttributesByOwner(q) => {
                Some(exec.execute(q).await.map(R::FindAttributesByOwner))
            }
            ClientQuery::FindAttributePairsForEntry(q) => {
                Some(exec.execute(q).await.map(R::FindAttributePairsForEntry))
            }
            ClientQuery::DistinctTextValuesForAttribute(q) => {
                Some(exec.execute(q).await.map(R::DistinctTextValuesForAttribute))
            }
        }
    }
}

impl From<ChildSlots> for ClientQuery {
    fn from(query: ChildSlots) -> Self {
        ClientQuery::ChildSlots(query)
    }
}

impl From<WouldCreateCycle> for ClientQuery {
    fn from(query: WouldCreateCycle) -> Self {
        ClientQuery::WouldCreateCycle(query)
    }
}

impl From<SuggestedRootInsertionTime> for ClientQuery {
    fn from(query: SuggestedRootInsertionTime) -> Self {
        ClientQuery::SuggestedRootInsertionTime(query)
    }
}

impl From<RootEntriesInInterval> for ClientQuery {
    fn from(query: RootEntriesInInterval) -> Self {
        ClientQuery::RootEntriesInInterval(query)
    }
}

impl From<FindEntryById> for ClientQuery {
    fn from(query: FindEntryById) -> Self {
        ClientQuery::FindEntryById(query)
    }
}

impl From<FindEntryJoinById> for ClientQuery {
    fn from(query: FindEntryJoinById) -> Self {
        ClientQuery::FindEntryJoinById(query)
    }
}

impl From<FindAncestors> for ClientQuery {
    fn from(query: FindAncestors) -> Self {
        ClientQuery::FindAncestors(query)
    }
}

impl From<FindDescendants> for ClientQuery {
    fn from(query: FindDescendants) -> Self {
        ClientQuery::FindDescendants(query)
    }
}

impl From<FindActivityById> for ClientQuery {
    fn from(query: FindActivityById) -> Self {
        ClientQuery::FindActivityById(query)
    }
}

impl From<AllActivities> for ClientQuery {
    fn from(query: AllActivities) -> Self {
        ClientQuery::AllActivities(query)
    }
}

impl From<FindActivityTemplateRoot> for ClientQuery {
    fn from(query: FindActivityTemplateRoot) -> Self {
        ClientQuery::FindActivityTemplateRoot(query)
    }
}

impl From<FindAttributeById> for ClientQuery {
    fn from(query: FindAttributeById) -> Self {
        ClientQuery::FindAttributeById(query)
    }
}

impl From<AllAttributes> for ClientQuery {
    fn from(query: AllAttributes) -> Self {
        ClientQuery::AllAttributes(query)
    }
}

impl From<FindAttributesByOwner> for ClientQuery {
    fn from(query: FindAttributesByOwner) -> Self {
        ClientQuery::FindAttributesByOwner(query)
    }
}

impl From<FindAttributePairsForEntry> for ClientQuery {
    fn from(query: FindAttributePairsForEntry) -> Self {
        ClientQuery::FindAttributePairsForEntry(query)
    }
}

impl From<DistinctTextValuesForAttribute> for ClientQuery {
    fn from(query: DistinctTextValuesForAttribute) -> Self {
        ClientQuery::DistinctTextValuesForAttribute(query)
    }
}
