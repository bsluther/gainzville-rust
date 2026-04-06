# Actions and Queries

Actions and Queries are the two sides of the domain model's I/O contract.

**Actions** reify write intent — what a user or system wants to change. **Queries** reify read
intent — what a caller wants to know. Both are first-class values, not just function calls. That
makes them usable as units in authorization checks, simulation, logging, and subscription
registration (for the reactive iOS cache model).

## Quick Reference

| Component | Location | Role |
|-----------|----------|------|
| `Action` enum + structs | `core/src/actions.rs` | Named write intent; one struct per variant, enum for dispatch |
| Mutators | `core/src/mutators.rs` | Validate an action against current state; produce a `Mutation` |
| `Delta<M>` / `ModelDelta` | `core/src/delta.rs` | Typed (Insert/Update/Delete) and type-erased change records |
| `Mutation` | `core/src/delta.rs` | Bundles action + timestamp + `Vec<ModelDelta>` for apply/audit |
| `SqliteApply` / `PgApply` | `sqlite/apply.rs`, `postgres/apply.rs` | Write `ModelDelta` to DB inside the current transaction |
| `Query` (sealed trait) | `core/src/queries.rs` | Binds each request type to its `Response` type at compile time |
| `define_query!` macro | `core/src/queries.rs` | One-line declaration: struct + seal impl + `Query` impl |
| `AnyQuery` enum | `core/src/queries.rs` | Type-erased wrapper for all query types; used in FFI / streaming |
| `QueryExecutor<Q>` | `core/src/query_executor.rs` | One impl per (executor, query) pair; SQL lives in the DB crate |
| `AnyQueryExecutor` | `core/src/query_executor.rs` | Marker supertrait; single bound used by all mutator signatures |
| `SqliteQueryExecutor` | `sqlite/sqlite_executor.rs` | Holds `&mut SqliteConnection`; implements all 19 query impls |
| `PostgresQueryExecutor` | `postgres/postgres_executor.rs` | Mirror of above for Postgres (`$1/$2` placeholders, `ANY($1)`) |

---

## Write Path

### Actions (`core/src/actions.rs`)

Each action is a named struct. The `Action` enum groups them for dispatch.

```rust
pub enum Action {
    CreateUser(CreateUser),
    CreateActivity(CreateActivity),
    CreateEntry(CreateEntry),
    MoveEntry(MoveEntry),
    DeleteEntryRecursive(DeleteEntryRecursive),
    CreateAttribute(CreateAttribute),
    CreateValue(CreateValue),
    UpdateEntryCompletion(UpdateEntryCompletion),
    UpdateAttributeValue(UpdateAttributeValue),
}

pub struct CreateEntry {
    pub actor_id: Uuid,
    pub entry: Entry,
}
```

- **One struct per variant.** Mutators name the specific struct, not the enum. The enum is for
  dispatch only.
- **`actor_id` lives in each struct** that needs it, not at the enum level. Not all actions
  carry a separate actor (e.g. `CreateUser` derives identity from the user itself).
- **`From<Struct> for Action`** impls on each variant for ergonomic construction:
  `client.run_action(CreateEntry::from(entry).into())`.

### Mutators (`core/src/mutators.rs`)

Mutators are free async functions that validate an action against current state and produce a
`Mutation`. They take an executor for reads, not a database connection or transaction directly —
sqlx types do not appear in their signatures.

```rust
pub async fn create_entry(
    executor: &mut impl AnyQueryExecutor,
    action: CreateEntry,
) -> Result<Mutation> {
    // Validate: check referenced activity exists, check permissions, etc.
    if let Some(activity_id) = action.entry.activity_id {
        if executor.execute(FindActivityById { id: activity_id }).await?.is_none() {
            return Err(DomainError::Other(...));
        }
    }
    // Produce delta.
    let insert_entry = Delta::Insert { new: action.entry.clone() };
    Ok(Mutation { action: Action::CreateEntry(action), changes: vec![insert_entry.into()], ... })
}
```

### Mutations and Deltas (`core/src/delta.rs`)

```rust
pub enum Delta<M> {
    Insert { new: M },
    Update { old: M, new: M },
    Delete { old: M },
}

pub enum ModelDelta {
    User(Delta<User>),
    Actor(Delta<Actor>),
    Activity(Delta<Activity>),
    Entry(Delta<Entry>),
    Attribute(Delta<Attribute>),
    Value(Delta<Value>),
}

pub struct Mutation {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub action: Action,
    pub changes: Vec<ModelDelta>,
}
```

`Delta<M>` is typed per model; `ModelDelta` erases the type for heterogeneous collections.
`Mutation` bundles user intent (`action`) with its effects (`changes`) for logging, sync, and
auditing.

### Apply (`sqlite/apply.rs`, `postgres/apply.rs`)

The `SqliteApply` / `PgApply` traits write deltas to the database inside the current transaction.

```rust
pub trait SqliteApply: Sized {
    async fn apply_delta(self, tx: &mut Transaction<'_, Sqlite>) -> Result<()>;
}

impl SqliteApply for ModelDelta {
    async fn apply_delta(self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        match self {
            ModelDelta::Actor(delta) => delta.apply_delta(tx).await,
            ModelDelta::Entry(delta) => delta.apply_delta(tx).await,
            // ... one arm per model
        }
    }
}
```

Each model type gets its own `impl SqliteApply for Delta<T>` with the concrete INSERT / UPDATE /
DELETE SQL.

### Full Write Flow (`sqlite/client.rs`)

```rust
pub async fn run_action(&self, action: Action) -> Result<()> {
    let mut tx = self.pool.begin().await?;
    let mut executor = SqliteQueryExecutor::new(&mut tx); // executor borrows tx

    let mx = match action {
        Action::CreateEntry(action) => mutators::create_entry(&mut executor, action).await?,
        // ... all variants
    };
    // executor borrow ends here; tx is usable again

    sqlx::query("PRAGMA defer_foreign_keys = ON").execute(&mut *tx).await?;
    for delta in mx.changes {
        delta.apply_delta(&mut tx).await?;
    }
    tx.commit().await?;
    let _ = self.change_transmitter.send(()); // notify subscribers
    Ok(())
}
```

The executor wraps the transaction for reads inside the mutator. After the mutator returns,
Rust's borrow checker releases the executor's borrow of `tx`, and the same transaction is used
to apply deltas and commit.

---

## Read Path

### Query Trait (`core/src/queries.rs`)

```rust
mod sealed { pub trait Sealed {} }

pub trait Query: sealed::Sealed + Clone + Debug + Send + 'static {
    type Response: Clone + Debug + Send + 'static;
}
```

The trait is **sealed** — only `core` can implement it. This guarantees:
- Every query type is listed in `core`, enabling exhaustive dispatch
- Downstream crates (FFI, db crates) cannot introduce new query types that bypass the system
- The `AnyQuery` enum stays complete

The `define_query!` macro keeps the per-query boilerplate DRY:

```rust
macro_rules! define_query {
    ($(#[$meta:meta])* $vis:vis struct $name:ident $body:tt => $response:ty) => {
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
define_query! { pub struct FindEntryById { pub entry_id: Uuid } => Option<Entry> }
define_query! { pub struct AllActivities; => Vec<Activity> }
```

Each query struct carries its response type as an associated type. `executor.execute(FindEntryById { entry_id })` always returns `Result<Option<Entry>>` — the compiler enforces this regardless of which executor is backing the call.

### Query Structs

18 query types, grouped by domain:

| Group | Queries |
|-------|---------|
| Auth | `IsEmailRegistered`, `FindUserById`, `FindUserByUsername`, `AllActorIds` |
| Activity | `FindActivityById`, `AllActivities` |
| Entry | `AllEntries`, `EntriesRootedInTimeInterval`, `FindAncestors`, `FindEntryById`, `FindEntryJoinById`, `FindDescendants` |
| Attribute | `FindAttributeById`, `AllAttributes`, `FindAttributesByOwner` |
| Value | `FindValueByKey`, `FindValuesForEntry`, `FindValuesForEntries`, `FindAttributePairsForEntry` |

### `AnyQuery` Enum

All query structs are also wrapped in `AnyQuery` for type-erased dispatch — used in streaming,
FFI, and simulation:

```rust
pub enum AnyQuery {
    FindEntryById(FindEntryById),
    AllActivities(AllActivities),
    // ... all 18 variants
}
```

`From<QueryStruct> for AnyQuery` impls allow ergonomic construction.

### `QueryExecutor<Q>` Trait (`core/src/query_executor.rs`)

```rust
#[allow(async_fn_in_trait)]
pub trait QueryExecutor<Q: Query> {
    async fn execute(&mut self, query: Q) -> Result<Q::Response>;
}
```

The trait is **generic over the query type** rather than having a single generic method. This is
intentional — see Design Decisions below.

`AnyQueryExecutor` is a marker trait alias combining `QueryExecutor<Q>` for all 18 query types:

```rust
pub trait AnyQueryExecutor:
    QueryExecutor<FindEntryById>
    + QueryExecutor<AllActivities>
    // ... all 18
{}

impl<T> AnyQueryExecutor for T where T: QueryExecutor<FindEntryById> + QueryExecutor<AllActivities> + ... {}
```

Mutators take `executor: &mut impl AnyQueryExecutor` — a single clean bound that means "can
execute any query."

### DB-Specific Executors (`sqlite/sqlite_executor.rs`, `postgres/postgres_executor.rs`)

```rust
pub struct SqliteQueryExecutor<'c> {
    conn: &'c mut SqliteConnection,
}

impl<'c> SqliteQueryExecutor<'c> {
    pub fn new(conn: &'c mut SqliteConnection) -> Self { Self { conn } }
}

impl QueryExecutor<FindEntryById> for SqliteQueryExecutor<'_> {
    async fn execute(&mut self, query: FindEntryById) -> Result<<FindEntryById as Query>::Response> {
        sqlx::query_as::<_, EntryRow>("SELECT ... FROM entries WHERE id = ?")
            .bind(query.entry_id)
            .fetch_optional(&mut *self.conn)
            .await?
            .map(|r| r.to_entry())
            .transpose()
    }
}
// ... one impl per query type
```

`SqliteQueryExecutor` holds a `&mut SqliteConnection`. Since `Transaction<'_, Sqlite>` derefs to
`SqliteConnection`, a single struct handles both standalone reads and transaction-scoped reads:

```rust
// Standalone read:
let mut conn = pool.acquire().await?;
let mut executor = SqliteQueryExecutor::new(&mut *conn);

// Transaction-scoped (inside run_action):
let mut tx = pool.begin().await?;
let mut executor = SqliteQueryExecutor::new(&mut *tx); // deref at construction
```

`PostgresQueryExecutor` mirrors this structure exactly; the only differences are `PgConnection`
instead of `SqliteConnection` and `$1`/`$2` parameter placeholders instead of `?`.

The return type of each impl uses the associated type form (`<QueryStruct as Query>::Response`)
rather than the concrete type. This ensures the impl stays synchronized with the `define_query!`
declaration — if the response type ever changes, the body breaks at the impl site rather than
silently returning the wrong type.

---

## Design Decisions

### 1. Sealed `Query` trait

Downstream crates cannot implement `Query` for new types. This makes `AnyQuery` exhaustive —
the compiler rejects any match that omits a variant — and ensures the FFI dispatch function
covers every possible query.

### 2. `define_query!` macro

Each query's response type is declared exactly once, co-located with the struct. Without the
macro, each query would require three separate items: the struct definition, a `sealed::Sealed`
impl, and a `Query` impl.

### 3. Per-query executor impls (vs. a monolithic Reader trait)

The old `Reader<DB>` trait had one method per query (20+ methods). The per-query impl approach:
- Co-locates each query's SQL with its type in the db crate
- Requires no central trait to keep synchronized across backends
- Enables per-executor query subsets: if a query type has a `PostgresQueryExecutor` impl but
  no `SqliteQueryExecutor` impl, any attempt to execute it via SQLite fails at compile time

### 4. `QueryExecutor<Q>` generic over Q (vs. a single-method generic trait)

This design is inspired by [Tower's `Service` trait](https://tokio.rs/blog/2021-05-14-inventing-the-service-trait).
Tower parameterizes over the request type — `impl Service<HttpRequest> { type Response = HttpResponse }`
— so the compiler knows concretely what response type a given service produces for a given request.
We want the same property inside `core`: `FindUserById` should statically return `Option<User>`.

The alternative would be to merge the executor and query into a single trait:

```rust
#[allow(async_fn_in_trait)]
pub trait ExecutableQuery<Request> {
    type Response;
    async fn execute(req: Request) -> Result<Self::Response>;
}
```

But `core` cannot implement this — `core` defines the request-to-response connection, but doesn't
know how to execute. Moving the impl to the DB crate breaks the connection: consumers lose the
compile-time guarantee that `FindUserById` returns `Option<User>` unless they're already coupled
to a specific executor.

The solution is the split:
- `Query` (in `core`) — declares the request→response association once, sealed so only `core` defines it
- `QueryExecutor<Q>` (in `core`) — the execution interface, generic over Q so each DB crate can impl it per query

A single-method approach — `trait QueryExecutor { async fn execute<Q: Query>(&mut self, Q) }` —
cannot narrow the bound in a concrete impl. You cannot write
`impl QueryExecutor for SqliteQueryExecutor { async fn execute<Q: Query + SqliteSpecific>(...)` —
the impl must satisfy the trait for *all* `Q: Query`, not a subset.

The parameterized trait allows each `impl QueryExecutor<ConcreteQuery> for SqliteQueryExecutor`
to contain exactly the SQL for that query, with no dispatch overhead or runtime type erasure.
`AnyQueryExecutor` recovers the ergonomic single-bound callsite.

### 5. `Reader<DB>` fully retired

`Reader<DB>` required `sqlx::Database` and `sqlx::Connection` in core's public API surface.
After the migration, core exports `Query`, `QueryExecutor<Q>`, `AnyQueryExecutor`, and domain
types. No sqlx types appear in core's public signatures. The FFI boundary (`gv-ffi`) will see
only core types.

`DomainError::Database(sqlx::Error)` remains in core as a pragmatic concession — it is a minor
dependency compared to having `sqlx::Database` in trait signatures and can be boxed later if
fully removing sqlx from core's `Cargo.toml` becomes worthwhile.

---

## What's Planned / Deferred

**Generic stream method**: `SqliteClient` currently has five hand-written stream methods
(`stream_activities`, `stream_entries`, etc.) with identical structure. A generic
`stream<Q: QueryExecutor<Q> + Clone>(query: Q) -> impl Stream<Item = Result<Q::Response>>`
method would collapse these, with callsites like `client.stream(AllActivities {})`. Deferred
until the streaming API is stabilized.

**FFI dispatch**: The `gv-ffi` crate will use `AnyQuery` as the serialization boundary. Swift
sends `AnyQuery` values (via UniFFI), Rust dispatches them through an exhaustive match, and
stores results as `AnyQueryResponse` values in a subscription cache. The sealed trait guarantees
the match is exhaustive — adding a new query type forces a compiler error at the dispatch site.
