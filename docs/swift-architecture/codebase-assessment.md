# iOS Architecture: Codebase Assessment

> Assessment of `gv-ios-architecture.md` against the concrete codebase state.
> Conducted 2026-04-03. Focus: FFI/Swift-relevant gaps, open questions, and design decisions.

---

## What the doc gets right

The high-level architecture is sound. The codebase is well-positioned:
- `SqliteClient` already has a proto-reactive pattern (broadcast channel + stream methods)
- `Reader` trait is the right abstraction layer; sqlx stays internal to Rust
- Actions/mutators match the fire-and-forget command model exactly
- Forest structure exists in `core/src/forest.rs`
- Multi-actor design, error types, and validation are in place

---

## Gap 1: Cache strategy and subscription model — resolved direction

The doc describes a `QueryCache` without specifying how subscriptions are registered or what the cache holds. The agreed direction:

**In-memory AppState as the cache backing store.** Load all entries, activities, attributes, and values into memory at startup; keep in sync by applying deltas in-memory alongside each SQLite write. For a fitness app this is fine — even heavy users won't approach memory limits. This gives all read operations access to the full dataset without DB round-trips.

**`Query` enum in `core` as reified read intent** — parallel to `Action` for writes. Each read use case becomes a named `Query` variant carrying its parameters and `actor_id`. The Reader trait methods become internal implementations of query variants rather than the public API. Benefits beyond the cache:
- Authorization: queries carry actor context, the query executor enforces read permissions the same way mutators enforce write permissions
- Simulation/testing: sequences of Actions and Queries become the unit of deterministic test
- FFI: Swift submits a `Query` value to subscribe; core stores it, re-runs it on change, caches the result

**Subscription model (naive first, improvable):**
```
Swift registers Query → core stores (Query, result_slot)
Any mutation fires → core re-runs all registered queries → notifies Swift
Swift reads result_slot synchronously
```
The naive "re-run everything on any change" is exactly what the current `stream_*` methods do — same semantics, just in-memory instead of a DB query. Selective invalidation (re-run only queries affected by a given delta's model type) is an optimization added later without changing the API.

**What changes in the existing code:** `SqliteClient`'s `stream_*` methods and `broadcast::Sender<()>` remain valid for the Dioxus app. The FFI layer builds the `AppState` + `Query` model on top of `SqliteClient`, using its mutation path and broadcast as the trigger.

---

## Gap 2 & 3: Forest as derived state — traversal API across FFI

The doc correctly identifies Forest as a domain concern that belongs in core and is exposed via methods, not raw data. The layering:

1. **Reader / DB layer** — loads a scope's entries via SQL (recursive CTEs for `entries_rooted_in_time_interval`, `find_descendants`, etc.). Correct and stays as-is.
2. **Forest layer** — receives the already-loaded `Vec<Entry>` and provides a traversal API over that normalized data. No further DB access; this is derived state computation.

The distinction matters for FFI: the live query cache (Gap 1) is about *keeping data fresh reactively*. The Forest is about *computing derived results from data already in hand* — parent chains, subtree membership, drop validity. These are different use cases with different ownership patterns across the FFI boundary.

**What Forest currently has:** `roots()`, `roots_in()`, `children()` — enough to render a flat view.

**What it's missing for the FFI use cases:** `parent()`, `ancestors()`, `descendants()`, `can_drop()`. Naive O(n) implementations over the Vec are appropriate — the loaded subtree is small.

**`can_drop` is the canonical example** of why this belongs in core: it needs to verify the drag target is not a descendant of the dragged entry, and that the target accepts children (`is_sequence`). It composes from Forest traversal and enforces the same invariant regardless of call site (iOS UI, sync rebase, future API client).

**The `entry_pool` concept (not yet documented):** Forest holds the normalized structure. Resolving a full `EntryJoin` (entry + activity + attributes) from a Forest entry requires something akin to an entry pool — a lookup structure over the same loaded data that resolves joins without going back to the DB. This is a design decision worth documenting: Forest for structural traversal, entry_pool for join resolution. The FFI layer may need both — Forest methods for tree operations, entry_pool for reading full records.

---

## Gap 6: `EntryJoin` is richer than "flat EntryRecord"

The doc says "flat `EntryRecord` values and IDs" cross the FFI. The actual main read type is:

```rust
pub struct EntryJoin {
    entry: Entry,
    activity: Option<Activity>,
    attributes: HashMap<Uuid, AttributePair>,
}
```

No flat representation exists today. The FFI record design needs to decide: attributes inlined into the entry record, or a separate fetch pass? The main log view needs many entries with their attributes simultaneously — this shapes the cache scope design and the entry_pool concept above.

---

## Gap 10: Multi-actor context for the FFI client

Every Action requires `actor_id: Uuid`. The doc describes `GainzvilleCore` initialized with a `CoreListener` but no actor context. A single-user iOS app needs the user's `actor_id` embedded.

**Options:**
- Embed `actor_id` in `GainzvilleCore` at construction — simple, matches single-user iOS
- Pass per-action from Swift — more flexible (multi-account), more verbose

The doc should take a position. Embedding at construction is the right default for iOS.

---

## Gap 11: `MoveEntry` / reordering and the Forest

The doc lists `get_successor` as a Forest operation but it's undefined in the codebase. More importantly, the general problem is: reordering entries (drag-and-drop) requires producing a valid `FractionalIndex` between two existing siblings. Maintaining fractional indices locally in Swift is fragile — it requires knowing the current state of all siblings.

The right approach is exactly what Forest enables: traverse the normalized structure to find the neighbors, compute the new index in Rust, and expose a high-level API:

```swift
core.moveEntry(id: entryId, after: prevSiblingId?, before: nextSiblingId?)
```

Rust looks up neighbors' indices from the loaded Forest, computes the new fractional index, and produces the `MoveEntry` action. Swift never deals with fractional indices directly. `get_successor` (next sibling in order) is a natural Forest method that supports this and other navigation patterns.

---

## Gap 12: Build concerns for the FFI crate

**`gv_postgres` and SQLX_OFFLINE:** The workspace includes `gv_postgres` which uses sqlx compile-time query verification against a live Postgres instance. Building `gv_ffi` for iOS requires `SQLX_OFFLINE=true` and a sqlx-data.json cache. Resolve early — blocks CI for the FFI crate.

**`ivm` crate and serde feature unification:** `ivm` pulls in DBSP which forces `serde_json`'s `arbitrary_precision` feature workspace-wide. This breaks internally-tagged enums with numeric fields. `gv_ffi` will inherit this. Options: exclude `ivm` from workspace members during FFI builds, or move it to a separate workspace.

---

## Gap 13: UniFFI version

The architecture doc specifies UniFFI 0.31.0; the PoC doc specifies 0.28. Resolve before starting the `gv_ffi` crate. Rust edition 2024 + UniFFI compatibility needs explicit verification.

---

## Gap 14: Platform strategy

The doc should state explicitly: native Swift iOS is the **replacement** for Dioxus on mobile, not a parallel product. This frames the investment correctly and clarifies that the FFI layer is the long-term mobile architecture.

---

## Open questions

| # | Question | Notes |
|---|---|---|
| 1 | **`Query` enum design** | What are the initial variants? Maps directly to current Reader methods. |
| 2 | **AppState / entry_pool design** | Document the AppState → Forest + entry_pool split: structural traversal vs. join resolution |
| 3 | **`EntryJoin` FFI representation** | Flat record with inlined attributes, or separate fetch? Affects entry_pool design. |
| 4 | **FFI actor context** | Embed `actor_id` in `GainzvilleCore` at init? |
| 5 | **`MoveEntry` FFI API** | `(after: Uuid?, before: Uuid?)` — confirm this is the interface |
| 6 | **`get_successor` definition** | Next sibling by frac_index? Nail down before implementing |
| 7 | **UniFFI version** | 0.28 or 0.31.0? Verify Rust edition 2024 compatibility |
| 8 | **IVM crate** | Exclude from workspace during FFI builds or move to separate workspace? |
