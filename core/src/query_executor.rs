use crate::{
    error::Result,
    queries::{
        AllActivities, AllActorIds, AllAttributes, AllEntries, EntriesRootedInTimeInterval,
        FindActivityById, FindAncestors, FindAttributeById, FindAttributePairsForEntry,
        FindAttributesByOwner, FindDescendants, FindEntryById, FindEntryJoinById, FindUserById,
        FindUserByUsername, FindValueByKey, FindValuesForEntries, FindValuesForEntry,
        IsEmailRegistered, Query,
    },
};

/// Executes queries against some backing store. Implementations include database-backed executors
/// (wrapping a connection or transaction), in-memory model executors, mock executors, and recording
/// executors.

#[allow(async_fn_in_trait)]
pub trait QueryExecutor<Q: Query> {
    async fn execute(&mut self, query: Q) -> Result<Q::Response>;
}

/// Marker trait for executors that can run all queries. Used as a convenient single bound
/// for mutators; callers can narrow to specific query bounds later if desired.
pub trait AnyQueryExecutor:
    QueryExecutor<IsEmailRegistered>
    + QueryExecutor<FindUserById>
    + QueryExecutor<FindUserByUsername>
    + QueryExecutor<AllActorIds>
    + QueryExecutor<FindActivityById>
    + QueryExecutor<AllActivities>
    + QueryExecutor<AllEntries>
    + QueryExecutor<EntriesRootedInTimeInterval>
    + QueryExecutor<FindAncestors>
    + QueryExecutor<FindEntryById>
    + QueryExecutor<FindEntryJoinById>
    + QueryExecutor<FindDescendants>
    + QueryExecutor<FindAttributeById>
    + QueryExecutor<AllAttributes>
    + QueryExecutor<FindAttributesByOwner>
    + QueryExecutor<FindValueByKey>
    + QueryExecutor<FindValuesForEntry>
    + QueryExecutor<FindValuesForEntries>
    + QueryExecutor<FindAttributePairsForEntry>
{
}

impl<T> AnyQueryExecutor for T where
    T: QueryExecutor<IsEmailRegistered>
        + QueryExecutor<FindUserById>
        + QueryExecutor<FindUserByUsername>
        + QueryExecutor<AllActorIds>
        + QueryExecutor<FindActivityById>
        + QueryExecutor<AllActivities>
        + QueryExecutor<AllEntries>
        + QueryExecutor<EntriesRootedInTimeInterval>
        + QueryExecutor<FindAncestors>
        + QueryExecutor<FindEntryById>
        + QueryExecutor<FindEntryJoinById>
        + QueryExecutor<FindDescendants>
        + QueryExecutor<FindAttributeById>
        + QueryExecutor<AllAttributes>
        + QueryExecutor<FindAttributesByOwner>
        + QueryExecutor<FindValueByKey>
        + QueryExecutor<FindValuesForEntry>
        + QueryExecutor<FindValuesForEntries>
        + QueryExecutor<FindAttributePairsForEntry>
{
}
