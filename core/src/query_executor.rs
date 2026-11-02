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

/// Answers questions from an in-memory [`Forest`]. Its answers never suspend or fail, but it has
/// the same shape as any other backing, so a query set can treat them alike
/// (`gv_client::queries::ClientQuery::execute`).
pub struct ForestExecutor<'f> {
    pub forest: &'f Forest,
}

impl<'f> ForestExecutor<'f> {
    pub fn new(forest: &'f Forest) -> Self {
        ForestExecutor { forest }
    }
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
