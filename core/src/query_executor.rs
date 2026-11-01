use crate::{
    error::Result,
    forest::Forest,
    queries::{
        AllActivities, AllActorIds, AllAttributes, AllEntries, DistinctTextValuesForAttribute,
        EntriesRootedInTimeInterval, FindActivityById, FindActivityTemplateRoot, FindAncestors,
        FindAttributeById, FindAttributePairsForEntry, FindAttributesByOwner, FindDescendants,
        FindEntryById, FindEntryJoinById, FindUserById, FindUserByUsername, FindValueByKey,
        FindValuesForEntries, FindValuesForEntry, IsEmailRegistered, Query,
    },
};

/// Executes queries against some backing store. Implementations include database-backed executors
/// (wrapping a connection or transaction), in-memory model executors, mock executors, and recording
/// executors.
#[allow(async_fn_in_trait)]
pub trait QueryExecutor<Q: Query> {
    async fn execute(&mut self, query: Q) -> Result<Q::Response>;
}

/// A question answered from an in-memory [`Forest`] alone: synchronous and pure. The other way to
/// answer a question (besides a [`QueryExecutor`]); which one a context uses is decided by its
/// query set (`gv_client::queries::ClientQuery::is_forest_backed`).
pub trait ForestQuery: Query {
    fn answer(&self, forest: &Forest) -> Self::Response;
}

/// Executors that answer every member of [`DomainQuery`](crate::queries::DomainQuery). A single
/// bound for mutators: `&mut impl DomainQueryExecutor`.
pub trait DomainQueryExecutor:
    QueryExecutor<IsEmailRegistered>
    + QueryExecutor<FindUserById>
    + QueryExecutor<FindUserByUsername>
    + QueryExecutor<AllActorIds>
    + QueryExecutor<FindActivityById>
    + QueryExecutor<AllActivities>
    + QueryExecutor<FindActivityTemplateRoot>
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
    + QueryExecutor<DistinctTextValuesForAttribute>
{
}

impl<T> DomainQueryExecutor for T where
    T: QueryExecutor<IsEmailRegistered>
        + QueryExecutor<FindUserById>
        + QueryExecutor<FindUserByUsername>
        + QueryExecutor<AllActorIds>
        + QueryExecutor<FindActivityById>
        + QueryExecutor<AllActivities>
        + QueryExecutor<FindActivityTemplateRoot>
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
        + QueryExecutor<DistinctTextValuesForAttribute>
{
}
