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
    models::{activity::Activity, attribute::Attribute, entry::Entry, entry_join::EntryJoin},
    queries::{
        ActivityTemplateJoin, AllActivities, AllAttributes, DistinctTextValuesForAttribute,
        FindActivityTemplateRoot, FindAttributeById, FindAttributesByOwner, FindEntryJoinById,
        RootEntriesInInterval,
    },
    query_executor::{ForestExecutor, QueryExecutor},
};
use gv_sql::sqlite::SqliteQueryExecutor;
use gv_ui::{
    ChildSlot,
    queries::{ChildSlots, SetsMembers, SuggestedRootInsertionTime, WouldCreateCycle},
};

/// A question the client can subscribe to or read once. The variant says *what* is
/// asked, never how it is answered.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientQuery {
    // Forest-backed
    ChildSlots(ChildSlots),
    SetsMembers(SetsMembers),
    WouldCreateCycle(WouldCreateCycle),
    SuggestedRootInsertionTime(SuggestedRootInsertionTime),
    // Client-only (SQLite)
    RootEntriesInInterval(RootEntriesInInterval),
    ActivityTemplateJoin(ActivityTemplateJoin),
    // Shared with the domain set
    FindEntryJoinById(FindEntryJoinById),
    AllActivities(AllActivities),
    FindActivityTemplateRoot(FindActivityTemplateRoot),
    FindAttributeById(FindAttributeById),
    AllAttributes(AllAttributes),
    FindAttributesByOwner(FindAttributesByOwner),
    DistinctTextValuesForAttribute(DistinctTextValuesForAttribute),
}

/// The answer to a [`ClientQuery`]: one variant per member.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientQueryResponse {
    // Forest-backed
    ChildSlots(Vec<ChildSlot>),
    SetsMembers(Vec<Entry>),
    WouldCreateCycle(bool),
    SuggestedRootInsertionTime(DateTime<Utc>),
    // Client-only (SQLite)
    RootEntriesInInterval(Vec<Entry>),
    ActivityTemplateJoin(Option<EntryJoin>),
    // Shared with the domain set
    FindEntryJoinById(Option<EntryJoin>),
    AllActivities(Vec<Activity>),
    FindActivityTemplateRoot(Option<Entry>),
    FindAttributeById(Option<Attribute>),
    AllAttributes(Vec<Attribute>),
    FindAttributesByOwner(Vec<Attribute>),
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
            ClientQuery::SetsMembers(q) => forest.execute(q).await.map(R::SetsMembers),
            ClientQuery::WouldCreateCycle(q) => forest.execute(q).await.map(R::WouldCreateCycle),
            ClientQuery::SuggestedRootInsertionTime(q) => {
                forest.execute(q).await.map(R::SuggestedRootInsertionTime)
            }
            // SQLite-backed
            ClientQuery::RootEntriesInInterval(q) => {
                sql.execute(q).await.map(R::RootEntriesInInterval)
            }
            ClientQuery::ActivityTemplateJoin(q) => {
                sql.execute(q).await.map(R::ActivityTemplateJoin)
            }
            ClientQuery::FindEntryJoinById(q) => sql.execute(q).await.map(R::FindEntryJoinById),
            ClientQuery::AllActivities(q) => sql.execute(q).await.map(R::AllActivities),
            ClientQuery::FindActivityTemplateRoot(q) => {
                sql.execute(q).await.map(R::FindActivityTemplateRoot)
            }
            ClientQuery::FindAttributeById(q) => sql.execute(q).await.map(R::FindAttributeById),
            ClientQuery::AllAttributes(q) => sql.execute(q).await.map(R::AllAttributes),
            ClientQuery::FindAttributesByOwner(q) => {
                sql.execute(q).await.map(R::FindAttributesByOwner)
            }
            ClientQuery::DistinctTextValuesForAttribute(q) => {
                sql.execute(q).await.map(R::DistinctTextValuesForAttribute)
            }
        }
    }
}
