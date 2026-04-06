# Query System Redesign: Concrete Adaptation Plan

> Builds on the type-safe request-response pairing research.
> Addresses all four requirements: compile-time type pairing, heterogeneous operation,
> DB-agnostic description, and swappable fulfillment — mapped onto the existing codebase.
>
> This document serves as both design rationale and migration roadmap.

---

## Current Architecture Summary

The system has three interlocking pieces:

**`Reader<DB: sqlx::Database>`** — A trait in `core` with one method per query. All methods
are `async fn` statics taking `&mut DB::Connection`. `PostgresReader` and `SqliteReader`
implement it. This is the *only* abstraction mutators use for reads.

**Query structs + `Query` enum** — First pass in `core/src/queries.rs`. Each struct has an
`execute<DB, R: Reader<DB>>` method that delegates to `Reader`. The enum wraps all variants
but cannot recover the typed result. The TODO at the top of the file captures the gap.

**Mutators** — Free functions in `core` generic over `<DB, R: Reader<DB>>`. They call
`R::method(...)` directly (not via query structs) and produce `Mutation`s containing `Delta`s.

**`SqliteClient`** — The reactive layer. Hand-written `stream_activities()`,
`stream_entries()`, etc. that each hard-code a `SqliteReader` call plus a
`broadcast::Receiver`. Five methods with identical structure differing only in which
reader method they call and what parameters they capture.

### The central friction

`core` depends on `sqlx` — not just for error types, but for `DB::Connection` in the
`Reader` trait signature and every mutator's generic bounds. This means the "DB-agnostic
description layer" doesn't exist yet; every query struct's `execute` method requires
`sqlx::Database` bounds. The `gv-ffi` crate planned for iOS will still use sqlx
transitively (through `sqlite/`), but sqlx types should not appear in core's public API
surface — the FFI boundary should see only core types (`AnyQuery`, `AnyQueryResponse`,
`Action`, domain models), not `sqlx::Database` or `Reader<DB>`.

---

## Layer 1: Query Trait with Associated Response Type

Replace the current `queries.rs` with a sealed trait that binds each request type to its
response type. No `execute` method, no `DB` generic, no `Reader` reference.

```rust
// core/src/query.rs

mod sealed { pub trait Sealed {} }

/// A reified read request. The associated `Response` type is the compile-time
/// guarantee of what executing this query produces.
///
/// Sealed: all variants are defined in core. Downstream crates cannot add new
/// query types — this is intentional for exhaustive dispatch, simulation replay,
/// and FFI serialization.
pub trait Query: sealed::Sealed + Clone + Debug + Send + 'static {
    type Response: Clone + Debug + Send + 'static;
}
```

### Query structs

Each struct stays structurally identical to today. The only change is adding the trait
impl and the seal. The `execute` method is removed — that responsibility moves to the
executor layer.

```rust
#[derive(Debug, Clone)]
pub struct FindEntryById {
    pub entry_id: Uuid,
}
impl sealed::Sealed for FindEntryById {}
impl Query for FindEntryById {
    type Response = Option<Entry>;
}

#[derive(Debug, Clone)]
pub struct AllActivities;
impl sealed::Sealed for AllActivities {}
impl Query for AllActivities {
    type Response = Vec<Activity>;
}

#[derive(Debug, Clone)]
pub struct EntriesRootedInTimeInterval {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}
impl sealed::Sealed for EntriesRootedInTimeInterval {}
impl Query for EntriesRootedInTimeInterval {
    type Response = Vec<Entry>;
}

// ... one impl per existing struct, ~18 total
```

### Macro to reduce boilerplate

The seal + trait impl is repetitive. A declarative macro keeps it DRY:

```rust
macro_rules! define_query {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident $body:tt => $response:ty
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        $vis struct $name $body

        impl sealed::Sealed for $name {}
        impl Query for $name {
            type Response = $response;
        }
    };
}

// Usage:
define_query! {
    pub struct FindEntryById { pub entry_id: Uuid } => Option<Entry>
}

define_query! {
    pub struct AllActivities; => Vec<Activity>
}
```

### The `AnyQuery` and `AnyQueryResponse` enums

Renamed from `Query` enum (which would now collide with the trait name) to `AnyQuery`.
A parallel `AnyQueryResponse` enum enables type-erased dispatch for FFI, logging,
and simulation replay.

```rust
pub enum AnyQuery {
    FindEntryById(FindEntryById),
    AllActivities(AllActivities),
    EntriesRootedInTimeInterval(EntriesRootedInTimeInterval),
    // ... all variants
}

pub enum AnyQueryResponse {
    FindEntryById(Option<Entry>),
    AllActivities(Vec<Activity>),
    EntriesRootedInTimeInterval(Vec<Entry>),
    // ... 1:1 with AnyQuery variants
}
```

**Decision: 1:1 pairing between `AnyQuery` and `AnyQueryResponse` variants.** Each
query gets its own response variant, even when the underlying type (e.g. `Vec<Entry>`)
is shared with other queries. The response represents the result of a *particular query*,
not just a data shape. Return signatures could change independently for each query in
the future, and 1:1 pairing makes FFI dispatch unambiguous. A macro could reduce the
boilerplate of maintaining both enums, but with LLM assistance maintaining verbose
types isn't a significant burden.

The pairing between `AnyQuery` and `AnyQueryResponse` is structural — maintained by
the `execute_any_query` dispatch function, not by the type system. Inside each match
arm, the `Query` trait's associated type provides compile-time safety (the compiler
knows `executor.execute(q)` returns `Q::Response`). The manual step is wrapping that
into the right `AnyQueryResponse` variant. A `wrap_response` method on the `Query`
trait (or generated by the `define_query!` macro) can make this compiler-enforced:

The `From` impls already written for each query struct → `Query` enum carry over
unchanged (just targeting `AnyQuery` now).

---

## Layer 2: QueryExecutor Trait

This replaces `Reader<DB>` as the abstraction that mutators and streaming code
program against.

```rust
// core/src/query_executor.rs

/// Executes queries against some backing store. Implementations include
/// database-backed executors (wrapping a connection or transaction),
/// in-memory model executors, mock executors, and recording executors.
#[allow(async_fn_in_trait)]
pub trait QueryExecutor {
    async fn execute<Q: Query>(&mut self, query: Q) -> Result<Q::Response>;
}
```

**Decision: `&mut self`, not `&self`.** Executing a query mutates connection state
(sqlx requires `&mut` for query execution). Using `&mut self` is honest about the
semantics. Mutators take `executor: &mut impl QueryExecutor`. In-memory executors
like `GvModel` that only need `&self` for reads can still satisfy the `&mut` bound —
having a mutable reference doesn't require mutation.

### Why `&mut self` instead of static methods

The current `Reader<DB>` uses static methods (`R::find_entry_by_id(...)`), which means
the reader is a zero-sized type used purely for its impl. This works but has two costs:
the connection must be passed as a parameter to every call, and you can't store an
executor as a field (there's nothing to store).

`&mut self` on `QueryExecutor` means the executor *owns* its connection/transaction.
This enables:

- Storing an executor in a struct (for `CachingExecutor<E>`, `RecordingExecutor<E>`)
- Passing `&mut executor` to mutators instead of `(&mut tx, PhantomData<R>)`
- Natural lifetime management: the executor borrows the connection for its lifetime
- Honest semantics: query execution mutates connection state

---

## Layer 3: Database-Specific Executors (Dispatch Mechanism)

The key question from the earlier analysis: how does `QueryExecutor::execute<Q: Query>`
dispatch to the right SQL implementation when `Q` is generic?

### The `ExecuteWith` bridge trait

Each DB crate defines a trait that individual query types implement:

```rust
// sqlite/src/execute_with.rs

/// Implemented by each Query type to provide SQLite-specific execution.
/// This trait is the bridge between the DB-agnostic Query trait in core
/// and the concrete sqlx queries in this crate.
#[allow(async_fn_in_trait)]
pub(crate) trait SqliteExecute: Query {
    async fn execute_sqlite(
        &self,
        conn: &mut SqliteConnection,
    ) -> Result<Self::Response>;
}
```

Each query type gets an impl. These are essentially the current `Reader` method
bodies extracted into per-query-type impls:

```rust
impl SqliteExecute for FindEntryById {
    async fn execute_sqlite(
        &self,
        conn: &mut SqliteConnection,
    ) -> Result<Option<Entry>> {
        sqlx::query_as::<_, EntryRow>(
            "SELECT ... FROM entries WHERE id = ?"
        )
        .bind(self.entry_id)
        .fetch_optional(&mut *conn)
        .await?
        .map(|e| e.to_entry())
        .transpose()
    }
}

impl SqliteExecute for AllActivities {
    async fn execute_sqlite(
        &self,
        conn: &mut SqliteConnection,
    ) -> Result<Vec<Activity>> {
        let activities = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities"
        )
        .fetch_all(&mut *conn)
        .await?;
        Ok(activities)
    }
}
```

### The executor struct

**Decision: a single executor struct, not separate connection/transaction structs.**
`Transaction<'_, Sqlite>` derefs to `SqliteConnection`, so the caller performs the
deref at construction time. One struct, one impl, two callsite patterns:

```rust
// sqlite/src/executor.rs

pub struct SqliteQueryExecutor<'c> {
    conn: &'c mut SqliteConnection,
}

impl<'c> SqliteQueryExecutor<'c> {
    pub fn new(conn: &'c mut SqliteConnection) -> Self {
        Self { conn }
    }
}

impl QueryExecutor for SqliteQueryExecutor<'_> {
    async fn execute<Q: Query + SqliteExecute>(&mut self, query: Q) -> Result<Q::Response> {
        query.execute_sqlite(self.conn).await
    }
}
```

Callsite for non-transaction reads:

```rust
let mut conn = pool.acquire().await?;
let mut executor = SqliteQueryExecutor::new(&mut *conn);
```

Callsite for transaction-scoped reads (e.g. inside mutators):

```rust
let mut tx = pool.begin().await?;
let mut executor = SqliteQueryExecutor::new(&mut *tx);  // deref at construction
```

Both work because `PoolConnection<Sqlite>` and `Transaction<'_, Sqlite>` both deref
to `SqliteConnection`.

**The bound change**: `QueryExecutor` in core says `Q: Query`. The SQLite impl adds
`Q: Query + SqliteExecute`. This compiles because Rust monomorphizes — at every callsite
where `SqliteQueryExecutor` is used, the compiler knows the concrete `Q` and checks that
it implements `SqliteExecute`. If you try to execute a query type that hasn't been
implemented for SQLite, you get a compile error at the callsite. This is exactly what
you want.

### Per-executor query subsets

A natural consequence of the per-query `SqliteExecute` / `PgExecute` impl approach:
**not every query needs an implementation in every executor.** If `AdminAuditLog` only
has a `PgExecute` impl and no `SqliteExecute` impl, the compiler rejects any attempt
to execute it through a `SqliteQueryExecutor`. No runtime error, no stub, no
`unimplemented!()` — it's a compile-time guarantee that a given executor supports
a given query.

This extends to mutators and actions: a mutator that internally uses server-only
queries will fail to compile when called with a client executor. The type system
enforces capability boundaries without an explicit capability system.

This is not a requirement now, but falls out for free from the design.

### What happens to `Reader<DB>`?

**Decision: fully retire `Reader<DB>` in favor of per-query `SqliteExecute` impls.**

The per-query approach is preferred because:

- Each query's SQL is co-located with its type definition (in the DB crate)
- No single 20-method trait to keep synchronized
- Adding a new query type means adding one struct in core + one impl per DB crate,
  with no changes to existing code
- Per-executor query subsets (above) are free — impossible with a monolithic trait
- The sealed `Query` trait + exhaustive `AnyQuery` enum still guarantee completeness

During migration, `SqliteExecute` impls can delegate to existing `SqliteReader`
methods as a bridge:

```rust
impl SqliteExecute for FindEntryById {
    async fn execute_sqlite(&self, conn: &mut SqliteConnection) -> Result<Option<Entry>> {
        SqliteReader::find_entry_by_id(conn, self.entry_id).await
    }
}
```

This lets you migrate query-by-query without a flag day.

---

## Adapting Mutators

### Before

```rust
pub async fn create_entry<'t, DB, R>(
    tx: &mut Transaction<'t, DB>,
    action: CreateEntry,
) -> Result<Mutation>
where
    DB: Database,
    R: Reader<DB>,
{
    if R::find_activity_by_id(&mut **tx, activity_id).await?.is_none() { ... }
    let entry = R::find_entry_by_id(&mut **tx, action.entry_id).await?;
}
```

### After

```rust
pub async fn create_entry(
    executor: &mut impl QueryExecutor,
    action: CreateEntry,
) -> Result<Mutation>
{
    if executor.execute(FindActivityById { id: activity_id }).await?.is_none() { ... }
    let entry = executor.execute(FindEntryById { entry_id: action.entry_id }).await?;
}
```

Key changes:

- Two generic parameters (`DB`, `R`) collapse to one (`impl QueryExecutor`)
- `&mut Transaction` disappears from the mutator signature — it's inside the executor
- Reader method calls become `executor.execute(QueryStruct { ... })` — explicit,
  greppable query construction
- Mutators no longer import `Reader` or `sqlx::Database`
- The return type of each `execute` call is inferred from the query struct's
  `type Response` — no change needed

### Impact on `run_action` in SqliteClient / PostgresServer

```rust
// sqlite/client.rs — after

pub async fn run_action(&self, action: Action) -> Result<()> {
    let mut tx = self.pool.begin().await?;

    // Wrap transaction in an executor (deref at construction)
    let mut executor = SqliteQueryExecutor::new(&mut *tx);

    let mx = match action {
        Action::CreateActivity(action) => {
            mutators::create_activity(&mut executor, action).await?
        }
        Action::CreateEntry(action) => {
            mutators::create_entry(&mut executor, action).await?
        }
        // ...
    };

    // Apply deltas (unchanged)
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await?;
    for delta in mx.changes {
        delta.apply_delta(&mut tx).await?;
    }
    tx.commit().await?;

    let _ = self.change_transmitter.send(());
    Ok(())
}
```

The `mutators::create_entry::<sqlx::Sqlite, SqliteReader>(&mut tx, action)` turbofish
disappears. The executor carries all the type information implicitly.

---

## Collapsing SqliteClient's Streaming Boilerplate

### Before: 5 hand-written stream methods

```rust
pub fn stream_activities(&self) -> impl Stream<Item = Result<Vec<Activity>>> { ... }
pub fn stream_attributes(&self) -> impl Stream<Item = Result<Vec<Attribute>>> { ... }
pub fn stream_entries(&self) -> impl Stream<Item = Result<Vec<Entry>>> { ... }
pub fn stream_entries_rooted_in_time_interval(&self, min, max) -> ... { ... }
pub fn stream_entry_join_by_id(&self, id) -> ... { ... }
```

Each has identical structure: acquire connection → execute reader method → yield →
wait for broadcast → repeat.

### After: one generic method

```rust
pub fn stream<Q: Query + SqliteExecute + Clone>(
    &self,
    query: Q,
) -> impl Stream<Item = Result<Q::Response>> + use<Q> {
    let pool = self.pool.clone();
    let mut change_rx = self.change_transmitter.subscribe();

    async_stream::stream! {
        // Initial fetch
        let mut conn = pool.acquire().await
            .map_err(DomainError::from)?;
        let mut executor = SqliteQueryExecutor::new(&mut conn);
        yield executor.execute(query.clone()).await;

        // Re-execute on every change notification
        while let Ok(()) = change_rx.recv().await {
            let mut conn = pool.acquire().await
                .map_err(DomainError::from)?;
            let mut executor = SqliteQueryExecutor::new(&mut conn);
            yield executor.execute(query.clone()).await;
        }
    }
}
```

Callsite:

```rust
// Before:
client.stream_activities()
client.stream_entries_rooted_in_time_interval(from, to)
client.stream_entry_join_by_id(id)

// After:
client.stream(AllActivities)
client.stream(EntriesRootedInTimeInterval { from, to })
client.stream(FindEntryJoinById { entry_id: id })
```

The return type is fully inferred: `stream(AllActivities)` yields `Result<Vec<Activity>>`,
`stream(FindEntryById { entry_id })` yields `Result<Option<Entry>>`. No runtime type
confusion possible.

---

## FFI / Swift Subscription Model

The design doc in `docs/swift-architecture/design.md` describes:

```
Swift registers Query → GainzvilleCore stores (Query, CachedResult)
Any mutation completes → re-run all registered queries → notify Swift
Swift reads CachedResult synchronously
```

The `AnyQuery` enum is the serialization boundary. Swift sends `AnyQuery` values
(via UniFFI records/enums), Rust dispatches them, and stores results as
`AnyQueryResponse` values in the cache.

```rust
// gv-ffi/src/core.rs

#[derive(uniffi::Object)]
pub struct GainzvilleCore {
    client: SqliteClient,
    subscriptions: RwLock<Vec<AnyQuery>>,
    cache: RwLock<HashMap</* subscription index */ usize, AnyQueryResponse>>,
    listener: Arc<dyn CoreListener>,
}

impl GainzvilleCore {
    /// Register a subscription. Returns a subscription ID.
    pub fn subscribe(&self, query: FfiQuery) -> u64 {
        let query: AnyQuery = query.into();
        let mut subs = self.subscriptions.write().unwrap();
        let id = subs.len();
        subs.push(query);
        // Run initial query and cache result
        self.refresh_subscription(id);
        id as u64
    }

    /// Called after every mutation. Re-runs all subscriptions.
    fn refresh_all(&self) {
        let subs = self.subscriptions.read().unwrap();
        for (i, _) in subs.iter().enumerate() {
            self.refresh_subscription(i);
        }
        self.listener.on_data_changed();
    }

    fn refresh_subscription(&self, id: usize) {
        let subs = self.subscriptions.read().unwrap();
        let query = &subs[id];

        let result = RUNTIME.block_on(async {
            let mut conn = self.client.pool.acquire().await?;
            execute_any_query(query, &mut conn).await
        });

        if let Ok(response) = result {
            self.cache.write().unwrap().insert(id, response);
        }
    }
}
```

The `execute_any_query` function dispatches through the enum:

```rust
async fn execute_any_query(
    query: &AnyQuery,
    conn: &mut SqliteConnection,
) -> Result<AnyQueryResponse> {
    let mut executor = SqliteQueryExecutor::new(conn);
    match query {
        AnyQuery::FindEntryById(q) => {
            Ok(AnyQueryResponse::FindEntryById(executor.execute(q.clone()).await?))
        }
        AnyQuery::AllActivities(q) => {
            Ok(AnyQueryResponse::AllActivities(executor.execute(q.clone()).await?))
        }
        // ... exhaustive match
    }
}
```

The sealed `Query` trait guarantees this match is exhaustive. Adding a new query type
forces you to handle it here — the compiler catches it.

---

## Simulation and Property-Based Testing

The `generation` crate already has `Arbitrary` traits for domain types. The query
system extends this naturally.

### Recording executor

```rust
/// Wraps a real executor and records every query + response pair.
pub struct RecordingExecutor<E: QueryExecutor> {
    inner: E,
    log: Vec<(AnyQuery, AnyQueryResponse)>,
}

impl<E: QueryExecutor> QueryExecutor for RecordingExecutor<E> {
    async fn execute<Q: Query>(&mut self, query: Q) -> Result<Q::Response> {
        let result = self.inner.execute(query.clone()).await?;
        // Convert to AnyQuery/AnyQueryResponse for storage
        self.log.push((
            query.clone().into(),  // From<Q> for AnyQuery
            result.clone().into(), // From<Q::Response> for AnyQueryResponse
        ));
        Ok(result)
    }
}
```

### Deterministic simulation

The design doc mentions `GvModel` as an in-memory model for simulation testing.
With the query system, `GvModel` implements `QueryExecutor`:

```rust
impl QueryExecutor for GvModel {
    async fn execute<Q: Query>(&mut self, query: Q) -> Result<Q::Response> {
        // Dispatch to in-memory data structures
        // No database needed
        todo!()
    }
}
```

A simulation test looks like:

```rust
#[test]
fn simulation_roundtrip() {
    let mut model = GvModel::new();
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let ctx = SimulationContext::default();

    // Generate and apply random actions
    for _ in 0..100 {
        let action = Action::arbitrary(&mut rng, &ctx);
        let mx = match action {
            Action::CreateEntry(a) => mutators::create_entry(&mut model, a).await?,
            // ...
        };
        model.apply_mutation(&mx);
    }

    // Assert properties on query results
    let all_entries = model.execute(AllEntries).await?;
    let forest = Forest::from(all_entries);

    // Property: every child's parent exists
    for entry in forest.data() {
        if let Some(parent_id) = entry.parent_id() {
            assert!(forest.entry(parent_id).is_some());
        }
    }
}
```

Because mutators are generic over `QueryExecutor`, the same mutator code
runs against both `SqliteQueryExecutor` (real DB) and `GvModel` (in-memory).
No test doubles needed for the query layer — the in-memory model *is* the
test implementation.

---

## Removing sqlx from core's public API

### The goal

The objective is not to remove sqlx from core's dependency tree — sqlx will remain
a transitive dependency through the sqlite and postgres crates. The goal is to remove
sqlx types from core's **public API surface**. Today, `Reader<DB: sqlx::Database>`,
`Transaction<'t, DB>`, and `DB::Connection` appear in core's trait signatures and
mutator parameters. After the migration, core exports `QueryExecutor`, `Query`,
`AnyQuery`, `AnyQueryResponse`, and domain types — none of which mention sqlx.

This matters most for the FFI boundary: `gv-ffi` wraps `SqliteClient` and sees
core types. The fewer implementation-specific types that cross that boundary, the
cleaner the UniFFI bridge stays.

### Current dependency chain

```
core (depends on sqlx for Reader<DB>, DomainError, Transaction)
  ├── Reader<DB: sqlx::Database>     ← sqlx in the trait signature
  ├── DomainError::Database(sqlx::Error)  ← sqlx in the error type
  └── mutators: Transaction<'t, DB>  ← sqlx in parameter types
```

### After the migration

With `QueryExecutor` replacing `Reader<DB>`, core no longer needs `sqlx::Database`,
`sqlx::Connection`, or `sqlx::Transaction` in any public signature. The remaining
dependency is `DomainError::Database(#[from] sqlx::Error)`.

**Options for the error type:**

1. **Keep sqlx in core's error** — pragmatic. `DomainError` already exists, sqlx errors
   are a real category. The dependency is minimal (just the Error type, not connection
   types). This is fine for now.

2. **Box the database error** — `DomainError::Database(Box<dyn std::error::Error + Send + Sync>)`.
   Removes the sqlx dependency from core entirely but loses `sqlx::Error` match arms.

3. **Feature-gate** — `#[cfg(feature = "sqlx")] Database(sqlx::Error)`. Allows core
   to compile without sqlx for pure-simulation or FFI builds.

Recommendation: option 1 for now. The sqlx error type in core is a minor dependency
compared to having `sqlx::Database` in trait signatures. Consider option 2 if removing
sqlx from core's `Cargo.toml` entirely becomes worthwhile.

---

## Migration Path (incremental, not a flag day)

### Phase 1: Add the Query trait (purely additive)

- Add `sealed` module and `Query` trait to `core/src/query.rs`
- Add `impl Query for T { type Response = ... }` to each existing struct
- Rename the `Query` enum to `AnyQuery`
- Add `AnyQueryResponse` enum (1:1 with `AnyQuery` variants)
- Remove `execute` methods from query structs
- **Nothing breaks.** `Reader<DB>` and mutators are untouched.

### Phase 2: Add QueryExecutor + bridge impls

- Add `QueryExecutor` trait (`&mut self`) to `core/src/query_executor.rs`
- In `sqlite/`: add `SqliteExecute` trait, implement for each query type by
  delegating to existing `SqliteReader` methods (bridge pattern)
- Add `SqliteQueryExecutor` (single struct, accepts `&mut SqliteConnection`)
- **Nothing breaks.** Both old and new paths coexist.

### Phase 3: Migrate mutators (one at a time)

- Change one mutator from `<DB, R: Reader<DB>>` to `executor: &mut impl QueryExecutor`
- Update `run_action` in sqlite/postgres to construct a `SqliteQueryExecutor`
  from the transaction (`SqliteQueryExecutor::new(&mut *tx)`) and pass it to mutators
- Repeat for each mutator
- **Each mutator migrates independently.** Mixed old/new is fine during transition.

### Phase 4: Replace stream methods

- Add generic `stream<Q>` method to `SqliteClient`
- Migrate Dioxus app callsites from `stream_activities()` to `stream(AllActivities)`
- Remove old `stream_*` methods
- **Callsite changes are mechanical.**

### Phase 5: Remove Reader<DB> from core

- Once all mutators use `QueryExecutor`, remove `Reader<DB>` trait from core
- Inline remaining `SqliteReader` / `PostgresReader` logic into `SqliteExecute` /
  `PgExecute` impls (if not already done via bridge in Phase 2)
- Remove `sqlx::Database` from core's public API
- `Reader` impls may be kept temporarily as private helpers within DB crates during
  the transition, but the goal is full retirement

### Phase 6: Build FFI layer

- Implement `execute_any_query` dispatch in `gv-ffi`
- Build subscription/cache system using `AnyQuery` / `AnyQueryResponse`
- Wire up `CoreListener` notification path

---

## Summary of Type Flow

```
                    core (DB-agnostic)
                    ┌──────────────────────────────────┐
                    │                                  │
  Query trait       │   FindEntryById { entry_id }     │
  (type-level)      │     type Response = Option<Entry> │
                    │                                  │
  QueryExecutor     │   trait QueryExecutor {           │
  (abstraction)     │     async fn execute<Q: Query>   │
                    │       (&mut self, Q) -> Result<Q::R>│
                    │   }                               │
                    │                                  │
  Mutators          │   async fn create_entry(         │
  (use executor)    │     executor: &mut impl QExecutor │
                    │   )                               │
                    │                                  │
  AnyQuery enum     │   enum AnyQuery { ... }           │
  (for FFI/sim)     │   enum AnyQueryResponse { ... }   │
                    │   (1:1 variant pairing)           │
                    └────────────┬─────────────────────┘
                                 │
              ┌──────────────────┼──────────────────┐
              │                  │                  │
     ┌────────▼────────┐ ┌──────▼──────┐ ┌─────────▼────────┐
     │   sqlite/       │ │  postgres/  │ │    gv-model/     │
     │                 │ │             │ │   (simulation)   │
     │ SqliteExecute   │ │ PgExecute   │ │                  │
     │ (per-query impl)│ │(per-query)  │ │ impl QueryExec   │
     │                 │ │             │ │   for GvModel    │
     │ SqliteQExecutor │ │ PgQExecutor │ │                  │
     │ (single struct, │ │ (single     │ │ (queries in-mem  │
     │  holds &mut conn│ │  struct)    │ │  Vec<Entry> etc) │
     │  or deref'd tx) │ │             │ │                  │
     └─────────────────┘ └─────────────┘ └──────────────────┘
```

Each layer has a single responsibility:
- **Core** defines what queries exist and what they return (pure description)
- **DB crates** define how to fulfill each query against their specific database
  via per-query `SqliteExecute` / `PgExecute` impls (replaces `Reader<DB>`)
- **Mutators** consume queries through the executor abstraction, no sqlx types visible
- **Streaming/FFI** use `AnyQuery`/`AnyQueryResponse` for dynamic dispatch
- **Simulation** provides an in-memory `QueryExecutor` implementation

The compile-time guarantee flows through: `executor.execute(FindEntryById { entry_id })`
always returns `Result<Option<Entry>>`, regardless of which executor backs it.

---

## Design Decisions Summary

Decisions made during design review, collected here for reference.

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **`QueryExecutor::execute` takes `&mut self`** | Honest about connection mutation semantics. sqlx requires `&mut` for query execution. In-memory executors that don't need mutation can still satisfy the bound. |
| 2 | **Single executor struct per DB** (e.g. `SqliteQueryExecutor`), not separate connection/transaction structs | `Transaction` derefs to `Connection`, so the caller derefs at construction. One struct, one `QueryExecutor` impl, two callsite patterns. Less surface area. |
| 3 | **1:1 pairing between `AnyQuery` and `AnyQueryResponse` variants** | Each response variant represents the result of a *particular query*, not just a data shape. Return types could change independently per query. Unambiguous for FFI dispatch. Verbose but manageable with LLM assistance; a macro can reduce boilerplate later. |
| 4 | **Fully retire `Reader<DB>`** in favor of per-query `SqliteExecute` / `PgExecute` impls | Per-query impls co-locate SQL with the query type. No monolithic trait to keep synchronized across DB backends. Enables per-executor query subsets for free. During migration, `SqliteExecute` impls can delegate to existing `SqliteReader` methods as a bridge. |
| 5 | **Per-executor query subsets are free** (not a requirement, but a natural consequence) | If a query type has a `PgExecute` impl but no `SqliteExecute` impl, the compiler rejects client-side execution at compile time. Server-only queries, admin queries, etc. are enforced by the type system without an explicit capability mechanism. |
| 6 | **sqlx removal from core is about API surface, not compilation** | `gv-ffi` will still depend on sqlx transitively through `sqlite/`. The goal is that core's public abstractions (`QueryExecutor`, `Query`, `AnyQuery`, mutator signatures) don't mention sqlx types. The FFI boundary sees only core types. |
| 7 | **`PgApply` pattern is fine as-is** | The same executor/bridge pattern *could* apply to the write side, but `PgApply` already uses a closed `ModelDelta` enum with per-type dispatch. The read side benefits more from the refactor (type pairing, streaming, executor swapping). No need to change the write path for symmetry alone. |
| 8 | **Keep sqlx in `DomainError` for now** | `DomainError::Database(sqlx::Error)` is a minor dependency compared to having `sqlx::Database` in trait signatures. Can be boxed later if fully removing sqlx from core's `Cargo.toml` becomes worthwhile. |
