# Gainzville iOS Architecture: Current Design

> Definitive reference for the native Swift iOS app. Synthesizes research and assessment docs.
> For historical rationale see `gv-ios-architecture.md`; for codebase gaps see `codebase-assessment.md`.

---

## Framing

Swift owns the platform (main thread, navigation, view lifecycle). The Rust core is invisible infrastructure. The core never fights for the main thread.

This is the Ghostty principle applied: Swift owns `main()`, Rust is a library.

---

## Crate Structure

```
core/          — domain model, Actions, Queries, Reader trait, Forest, AppState
sqlite/        — SqliteClient, Reader impl, delta application
gv-ffi/        — UniFFI bridge (new crate, zero logic — wraps sqlite/)
  Cargo.toml   — crate-type = ["cdylib", "staticlib"]
  src/
    lib.rs     — uniffi::setup_scaffolding!()
    core.rs    — GainzvilleCore (UniFFI Object)
    types.rs   — FFI-safe mirror types (records, enums, errors)
```

No business logic in `gv-ffi`. It wraps `SqliteClient` and `GainzvilleCore` with a static tokio runtime and sync function signatures.

---

## Threading Model

Swift calls into Rust from its own thread pool — no tokio context. The bridge uses a static runtime:

```rust
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
});
```

All `#[uniffi::export]` methods are synchronous; async work is driven by `RUNTIME.block_on(...)`. This eliminates UniFFI issue #2576 (async trait methods) and panic risk from missing runtime context. SQLite operations are fast enough that sync wrappers are fine.

---

## CQRS: Actions and Queries

The core uses a command/query separation that mirrors the existing write-side pattern.

**Actions** (existing) — reify write intent. Each Action carries `actor_id`. Mutators validate the action, produce a `Mutation` with deltas, which are applied to SQLite and to the in-memory AppState.

**Queries** (new, in `core`) — reify read intent. Same pattern as `Action`: individual structs grouped by an enum. The enum provides grouping and dispatch; each struct specifies exactly what that query needs, enabling distinct impls per variant.

```rust
// core/src/queries.rs
pub enum Query {
    EntriesInDateRange(EntriesInDateRange),
    AllActivities(AllActivities),
    ActivityProfile(ActivityProfile),
    // ...
}

pub struct EntriesInDateRange {
    pub actor_id: Uuid,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

pub struct AllActivities {
    pub actor_id: Uuid,
}

pub struct ActivityProfile {
    pub actor_id: Uuid,
    pub activity_id: Uuid,
}
```

**`actor_id` placement:** same convention as Actions — in each struct that needs it, not hoisted to the enum level. The enum doesn't enforce its presence (mirroring the existing `Action` TODO comment), but each struct carries it for handlers. Reader methods currently ignore permissions; the `actor_id` on each query struct is where that enforcement will be added without changing the API.

Benefits of making queries first-class:
- **Authorization**: actor context is present at the query level; enforcement is addable per-struct without changing callers
- **Simulation/testing**: sequences of Actions and Queries are the unit of deterministic tests; both sides of the system are replayable
- **Subscription**: Swift submits a `Query` value to register a subscription; core stores it, re-runs it on change, Swift reads the result

The Reader trait methods become internal implementations of query execution, not the public API surface.

---

## Query Execution: In-Memory vs. Re-query

When a subscribed `Query` needs to be re-run after a mutation, there are two approaches and both are valid:

**Re-query SQLite** — run the existing Reader method against the pool. Simple, leverages SQL for complex filtering/sorting, no additional state to maintain. This is what the existing `stream_*` methods already do and is a fine starting point.

**In-memory model** — maintain a snapshot of the dataset in memory, kept in sync by applying deltas alongside each SQLite write. Queries execute against the in-memory state with no DB round-trip. Better for high-frequency reads and derived computation (Forest traversal, EntryPool join resolution).

The two approaches aren't mutually exclusive — simple queries (all activities) could always re-query SQLite while structural queries (subtree membership, can_drop) operate on the in-memory model. The `Query` enum design is the same either way; execution strategy is an implementation detail of the query executor.

### In-Memory Model Design

If/when an in-memory model is built, the natural structure is:

```rust
struct GvModel {
    entries: Vec<Entry>,
    activities: Vec<Activity>,
    attributes: Vec<Attribute>,
    values: Vec<Value>,
}
```

Kept in sync via an `Apply` trait parallel to `SqliteApply` and `PgApply`:

```rust
// gv-model (or core) — in-memory delta application
impl ModelApply for Delta<Entry> {
    fn apply(&mut self, model: &mut GvModel) { /* insert/update/delete in Vec */ }
}
```

This is also the right backing structure for **deterministic simulation testing** — a planned feature where sequences of Actions are applied to an in-memory model and Queries are run to assert invariants, with no database involved. The simulation feature and the iOS cache model converge on the same structure, which argues for a shared `gv-model` crate (separate from `core`) that implements `GvModel` + `ModelApply`.

### Forest and EntryPool

Two views over a loaded `Vec<Entry>` (from either AppState or a DB query result):

**Forest** (`core/src/forest.rs`) — structural traversal over the normalized entry hierarchy:
- `roots()`, `roots_in()`, `children()` — already implemented
- `parent()`, `ancestors()`, `descendants()` — to be added (naive O(n) over the Vec)
- `can_drop(dragged, target)` — composed from the above; belongs in core so the same invariant is enforced across iOS UI, sync rebase, and any future client

**EntryPool** (not yet implemented) — join resolution given a set of loaded entries, activities, attributes, and values. Resolves a full `EntryJoin` without a DB round-trip. Forest and EntryPool are complementary: Forest for "how are entries related?", EntryPool for "what is this entry's full data?"

---

## Subscription Model

```
Swift registers Query → GainzvilleCore stores (Query, CachedResult)
Any mutation completes → re-run all registered queries → notify Swift
Swift receives wakeup → reads CachedResult synchronously
```

**Naive implementation first:** re-run all registered queries on any change. This is identical semantics to the existing `stream_*` methods in `SqliteClient`. Each query re-executes via whichever strategy is appropriate (re-query SQLite or in-memory model). No subscription API complexity needed initially.

**Future optimization:** selective invalidation — annotate each `Query` variant with which model types can affect its results. Only re-run queries whose model types appear in the mutation's deltas. API is unchanged; only the invalidation logic improves.

### Notification path (Rust → Swift)

```rust
// Swift implements this protocol
#[uniffi::export(with_foreign)]
pub trait CoreListener: Send + Sync {
    fn on_data_changed(&self);  // lightweight wakeup
}

// GainzvilleCore holds the listener + crossbeam channel
#[derive(uniffi::Object)]
pub struct GainzvilleCore {
    state: Arc<RwLock<AppState>>,
    subscriptions: Arc<RwLock<Vec<Subscription>>>,
    listener: Arc<dyn CoreListener>,
}
```

Swift side — drain on wakeup:
```swift
func onDataChanged() {
    Task { @MainActor in
        viewModel.refresh()  // reads from cached results synchronously
    }
}
```

---

## Data Flow

### Write path (Swift → Rust)

```
core.runAction(FfiAction)           // fire-and-forget, synchronous at FFI boundary
  → RUNTIME.block_on(run_action)
  → mutator validates, produces Mutation
  → deltas applied to SQLite (transaction)
  → deltas applied to AppState (in-memory)
  → registered queries re-run
  → listener.on_data_changed() fires
```

Swift does not await or track the outcome. The result is observable via the next notification.

### Read path (Swift → Rust)

```
viewModel.refresh()
  → core.readCachedResult(queryId)  // synchronous FFI call
  → reads from pre-computed CachedResult in AppState
  → serializes to FFI Record types
  → Swift updates @Observable properties
  → SwiftUI re-renders
```

No async, no DB query on the read path. The cache was already updated before the notification fired.

---

## SwiftUI Integration

```swift
@Observable
@MainActor
class EntriesViewModel {
    var entries: [FfiEntryJoin] = []

    func refresh() {
        entries = core.readCachedEntries()  // synchronous FFI call
    }
}
```

- No `ObservableObject`, no `@Published`, no Combine
- `@Observable` macro gives fine-grained tracking
- `@MainActor` ensures mutations are always on the main thread
- Views receive value-type snapshots and render — no mutable state, no participation in change propagation
- UI-local ephemeral state (`@State`) is fine for form fields, focus, animation

---

## FFI Record Design

FFI-safe mirror types are defined in `gv-ffi/src/types.rs`. They are copied at the boundary — Swift owns its copy independently (UniFFI Record semantics).

Key open question: how is `EntryJoin` represented? Options:
- **Flat record** — attributes inlined as a `Vec<FfiAttributePair>`. Simple, complete per-entry.
- **Separate fetch** — entry record + separate `core.readAttributes(entryId)` call.

Flat record is preferred: the main log view needs entries with their attributes together, and the EntryPool backing the FFI should pre-join them.

---

## Build Concerns

**SQLX_OFFLINE:** `gv_postgres` uses sqlx compile-time query verification against a live Postgres instance. Building `gv_ffi` for iOS requires `SQLX_OFFLINE=true` + a sqlx-data.json cache checked in for the sqlite crate. Resolve before setting up CI.

**`ivm` crate:** Forces `serde_json`'s `arbitrary_precision` feature workspace-wide, breaking internally-tagged enums with numeric fields. Exclude `ivm` from workspace members during FFI builds or move it to a separate workspace before `gv_ffi` compiles cleanly.

**UniFFI version:** Architecture doc references 0.31.0; PoC doc references 0.28. Verify Rust edition 2024 compatibility before starting `gv_ffi`.

---

## Open Questions

| # | Question | Notes |
|---|---|---|
| 1 | **`Query` enum initial variants** | Maps to current Reader methods; enumerate the read use cases for the first iOS screens |
| 2 | **`gv-model` crate** | Should GvModel + ModelApply live in a new crate shared between iOS cache and simulation? Or stay in core? |
| 3 | **`EntryJoin` FFI representation** | Flat record with inlined attributes confirmed as preferred; finalize the type |
| 4 | **`actor_id` in `GainzvilleCore`** | Embed at construction for single-user iOS; confirm this is the approach |
| 5 | **`MoveEntry` FFI API** | `(id, after: Uuid?, before: Uuid?)` — Rust looks up neighbor indices from Forest |
| 6 | **`get_successor` definition** | Next sibling by frac_index, or next root by canonical_instant? |
| 7 | **UniFFI version** | Resolve 0.28 vs 0.31.0; verify edition 2024 |
| 8 | **`ivm` crate isolation** | Separate workspace or excluded members for FFI builds |
