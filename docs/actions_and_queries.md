# Actions and Queries

Actions and Queries are the two sides of the domain's input/output model.

**Actions** reify write intent — what a user or system wants to change. **Queries** reify read
intent — what a caller wants to know. Keeping these separate (CQRS) gives each side a clean
contract: actions produce `Mutation`s with auditable deltas; queries produce typed results from
whatever read backend is appropriate.

Both are first-class values, not just function calls. That makes them usable as units in
authorization checks, simulation, logging, and subscription registration.

---

## Actions

Defined in `core/src/actions.rs`.

Each action is a named struct. The `Action` enum groups them for dispatch:

```rust
pub enum Action {
    CreateEntry(CreateEntry),
    MoveEntry(MoveEntry),
    // ...
}

pub struct CreateEntry {
    pub actor_id: Uuid,
    pub entry: Entry,
}
```

- **One struct per variant.** Handlers and mutators name the specific struct they handle, not
  the enum. The enum is for grouping and dispatch only.
- **`actor_id` lives in each struct** that needs it, not at the enum level. Not all actions
  require a separate actor (e.g. `CreateUser` derives identity from the user itself). A helper
  method on the enum can surface actor_id without pattern matching where needed.
- **`From<StructType> for Action`** impls on each variant for ergonomic construction.

Actions are processed by mutators (`core/src/mutators.rs`), which validate the action against
current state (via `Reader`) and return a `Mutation` containing the resulting `Delta`s.

---

## Queries (planned)

To be defined in `core/src/queries.rs`. Same structural pattern as Actions:

```rust
pub enum Query {
    EntriesInDateRange(EntriesInDateRange),
    AllActivities(AllActivities),
    // ...
}

pub struct EntriesInDateRange {
    pub actor_id: Uuid,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}
```

- Same struct-per-variant pattern; enum is grouping only.
- `actor_id` per-struct. Reader methods currently ignore permissions; the field is present so
  authorization can be added per-query-type without changing the API.
- The existing `Reader` trait methods become internal implementations of query execution, not
  the public API.

Queries serve double duty: they are the subscription unit for the reactive iOS cache model
(Swift registers a `Query` to subscribe; core re-runs it on change) and the assertion unit for
deterministic simulation testing (run a sequence of Actions, assert on Query results).
