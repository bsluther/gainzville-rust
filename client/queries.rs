//! The client's query set: every question Swift can subscribe to or read once, and how each is
//! answered. Swift sees this set and nothing about the backing.
//!
//! Membership, not nesting: questions shared with the domain set (`FindEntryJoinById`, ...) are
//! listed here as plain variants, defined once in `gv-core`. [`ClientQuery::execute`] is the only
//! statement of which backing answers each question; moving a question means changing its one arm
//! there (plus a `QueryExecutor` impl if that backing lacks one).

use chrono::{DateTime, Utc};
use gv_core::{
    error::Result,
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
    query_executor::{ForestExecutor, QueryExecutor},
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
    /// Answer this question, from whichever backing answers it. Both executors read the same
    /// data version when the caller gives them one (`LiveStore`'s rebuild does).
    pub async fn execute(
        self,
        forest: &mut ForestExecutor<'_>,
        sql: &mut SqliteQueryExecutor<'_>,
    ) -> Result<ClientQueryResponse> {
        use ClientQueryResponse as R;
        match self {
            // Forest-backed
            ClientQuery::ChildSlots(q) => forest.execute(q).await.map(R::ChildSlots),
            ClientQuery::WouldCreateCycle(q) => forest.execute(q).await.map(R::WouldCreateCycle),
            ClientQuery::SuggestedRootInsertionTime(q) => {
                forest.execute(q).await.map(R::SuggestedRootInsertionTime)
            }
            // SQLite-backed
            ClientQuery::RootEntriesInInterval(q) => {
                sql.execute(q).await.map(R::RootEntriesInInterval)
            }
            ClientQuery::FindEntryById(q) => sql.execute(q).await.map(R::FindEntryById),
            ClientQuery::FindEntryJoinById(q) => sql.execute(q).await.map(R::FindEntryJoinById),
            ClientQuery::FindAncestors(q) => sql.execute(q).await.map(R::FindAncestors),
            ClientQuery::FindDescendants(q) => sql.execute(q).await.map(R::FindDescendants),
            ClientQuery::FindActivityById(q) => sql.execute(q).await.map(R::FindActivityById),
            ClientQuery::AllActivities(q) => sql.execute(q).await.map(R::AllActivities),
            ClientQuery::FindActivityTemplateRoot(q) => {
                sql.execute(q).await.map(R::FindActivityTemplateRoot)
            }
            ClientQuery::FindAttributeById(q) => sql.execute(q).await.map(R::FindAttributeById),
            ClientQuery::AllAttributes(q) => sql.execute(q).await.map(R::AllAttributes),
            ClientQuery::FindAttributesByOwner(q) => {
                sql.execute(q).await.map(R::FindAttributesByOwner)
            }
            ClientQuery::FindAttributePairsForEntry(q) => {
                sql.execute(q).await.map(R::FindAttributePairsForEntry)
            }
            ClientQuery::DistinctTextValuesForAttribute(q) => {
                sql.execute(q).await.map(R::DistinctTextValuesForAttribute)
            }
        }
    }
}
