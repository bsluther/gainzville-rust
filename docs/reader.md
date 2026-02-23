# Reader Execution Model

## Current Design

`Reader<DB>` methods take `&mut DB::Connection`.

This is the current and intended API contract across all reader methods, not just methods that run multiple queries.

```rust
async fn find_entry_by_id(
    connection: &mut DB::Connection,
    entry_id: Uuid,
) -> Result<Option<Entry>>;
```

### What this means in practice

- Reader methods always run against an explicit checked-out connection.
- Mutators can call readers inside an existing transaction by passing `&mut **tx`.
- Non-transaction callsites can acquire from a pool and pass `&mut *connection`.
- Multi-query reader methods can safely reuse the same connection for all sub-queries.

### Typical call patterns

From a transaction-scoped flow:

```rust
let mut tx = pool.begin().await?;
let entry = R::find_entry_by_id(&mut **tx, entry_id).await?;
```

From a non-transaction read path:

```rust
let mut connection = pool.acquire().await?;
let activities = SqliteReader::all_activities(&mut *connection).await?;
```

## Rationale

## Previous approach

Reader methods previously accepted an `Executor` argument (for example, `impl Executor<'e, Database = DB>`), which made callsites concise when using `&Pool`.

## Why the connection-based API is preferred

- It enforces transaction compatibility as the default reader contract.
- It avoids trait-bound issues around executor reuse in multi-query methods.
- It provides explicit connection affinity, which is often what read consistency assumptions rely on.
- It aligns with `sqlx 0.8` guidance where transaction/pool-connection `Executor` impls are not relied on directly and callsites use deref to the inner connection (`&mut *tx`, `&mut *connection`).

In short: connection-based Reader signatures make behavior explicit and stable across both transaction and non-transaction call paths, with one consistent API shape.
