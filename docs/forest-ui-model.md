# Forest UI Model

## Summary

UI components need access to two kinds of data: **structural** (how entries relate to each other as
a tree/forest) and **rich** (the full joined data for a given entry — activity, attributes, temporal
info). The current approach of having each component independently subscribe to a per-entry DB
stream doesn't scale and couples component structure to data fetching. The plan is to introduce a
`Forest` abstraction for structure and an `EntrySource` trait for rich data resolution, both
provided reactively via Dioxus context.

---

## Objectives

- Decouple component tree shape from data fetching
- Enable structural navigation (parent, children, depth, temporal predecessor, subtree bounds)
  without ad-hoc prop drilling or per-component DB subscriptions
- Support the same UI components working against local SQLite data, network-fetched data, and test
  fixtures
- Lay groundwork for offline-first sync and browsing of remote libraries

---

## Assumptions

- For a user's own data, the full forest is available locally and can be loaded eagerly
- Remote data (global library, shared exercises) arrives as **complete subtrees** — all descendants
  of the returned root are present, though the root itself may have a `parent_id` pointing outside
  the result (see expanded notes)
- Temporal data is structural: operations like "temporal predecessor of this root entry" and
  "inferred bounds of a subtree" are navigation concerns, not display concerns
- A local SQLite query rebuilding the full `Vec<Entry>` on each DB change is acceptable performance
  for the foreseeable future (IVM is a future optimization)

---

## Constraints

- Must integrate with Dioxus signals/memos and the existing `use_stream` reactive pattern
- Remote data may never be fully available locally; the model must accommodate partial forests
  (bounded subtrees) where the root may have a `parent_id` pointing to context not in scope
- Should not require UI components to know whether their data comes from local SQLite or the network

---

## Current Plan

**Two separate value types, provided via Dioxus context:**

1. **`Forest`** — a value type wrapping `Vec<Entry>` (with full temporal data) that exposes
   structural navigation. Provided as `Memo<Forest>`, recomputed when the DB stream emits.

2. **`HashMap<Uuid, EntryJoin>`** — a reactive map of fully-joined entry data, computed from a
   single `stream_all_entry_joins` query. Provided as `Memo<HashMap<Uuid, EntryJoin>>`.

Components `consume_context` for whichever they need. Structure-only components (drag-and-drop,
depth rendering) take only `Forest`. Components needing rich data also reach for the join map.

**Longer term:** Extract an `EntrySource` trait with both `forest_stream()` and `resolve()`, so
local and network implementations are swappable without touching UI components.

---

## Motivation

### Why not use the component tree as the data structure?

Each `EntryView` currently subscribes to `stream_entry_join_by_id` independently, creating N
database subscriptions for N visible entries. Beyond the subscription cost, this couples structural
concerns (what to render) to data fetching (how to get the data). The problems that appeared in
practice:

- More and more structural information needed to be threaded through components as props (fractional
  indices, depth, parent timestamps)
- Drag-and-drop introduced additional data needs that didn't fit the per-component model
- Mocking/testing required a database, not just in-memory data
- Different contexts (e.g. rendering activity templates vs. log entries) demanded different data
  access patterns with no clean abstraction

### Why Forest as a value type?

A `Forest` derived from the full entry list is cheap to construct, easy to test (pass
`Forest::from(vec![...])` in tests), and keeps structural queries as pure functions over data
rather than async calls. Temporal data belongs here because temporal navigation *is* structural.

### Why a single join map instead of per-entry streams?

Replacing N per-entry subscriptions with one `stream_all_entry_joins` query reduces subscription
overhead and simplifies component code. On any DB change, one query runs, one map is rebuilt, and
all dependent components update.

---

## Alternatives

### Per-entry streams (current approach)

Each component subscribes to `stream_entry_join_by_id`. Simple to reason about locally, but doesn't
scale with the number of visible entries and couples component structure to data fetching.

### Forest as interface (hide the data source)

Forest could be a trait rather than a value type — hiding whether it's backed by SQLite, a network
request, or test data. Rejected for now as premature: the common interface concern is better
addressed by `EntrySource` at the provision layer, leaving Forest as a simple, testable value.

### Lazy/partial forests with hole-filling

Forest could know about missing nodes and issue requests to fill them. Considered for the
remote-browsing use case, but unnecessary for the common cases: remote data arrives as complete
subtrees downward, so there are no holes within the result. `forest.parent(id) == None` is
interpreted as "root in this context." See expanded notes for the sharing/collaboration case where
this interpretation becomes ambiguous.

### Coupling Forest and Resolver into one type

Combining structural navigation and rich-data resolution into a single type was considered. Rejected
because some contexts need only structure (drag-and-drop, depth), and the coupling would add IO
concerns to what is otherwise a pure value type. Instead, both are provided via context and
components consume whichever they need.

---

## Expanded Notes

### Temporal data as structural

Examples of structural queries that require temporal data:

- `forest.temporal_predecessor(id)` — the root-level entry immediately before this one in time;
  needed for drag-and-drop insertion ordering
- `forest.inferred_start(id)` — earliest `start_time` among all descendants; needed for entries
  that inherit temporal bounds from their children
- `forest.inferred_end(id)` — latest `end_time` among all descendants

These are navigation operations over the tree, not display concerns. Stripping temporal data out of
Forest would require callers to join it back in for every structural query that touches time.

### Subtrees from network: complete downward, open at the root

Entries use child-to-parent pointers (`parent_id`). So a "complete subtree" rooted at node X means:
all descendants of X are present, but X itself may have a `parent_id` pointing to something outside
the result. This is expected and handled — `forest.parent(X)` returns `None`, meaning "root in this
context."

This is unambiguous for **library search results**: the parent above the search result is broader
library context you didn't ask for, and treating the result root as a forest root is correct.

It becomes ambiguous in **sharing/collaboration**: if a user shares a single workout that lives
inside a larger training block, the recipient gets a subtree whose root has a `parent_id` pointing
to context that *was* meaningful to the sharer. The recipient's forest can still render the workout
correctly, but the missing parent is not irrelevant — it may represent "Set 1 of 3 in a block" or
similar context the sharer intended. How to surface this (a "shared from within a larger context"
indicator, a permission to request the parent, etc.) is an open design question.

The local forest and each network result are **different Forest instances** rendered by the same UI
components — not one forest with holes. `Forest` being a value type makes this natural: construct
one for local data, a separate one for the network result, pass both to the same components.

### EntrySource trait (future)

```rust
trait EntrySource {
    fn forest_stream(&self) -> impl Stream<Item = Forest>;
    fn resolve_stream(&self, id: Uuid) -> impl Stream<Item = Option<EntryJoin>>;
}
```

Local implementation: `forest_stream` re-emits on each DB change; `resolve_stream` is backed by
the join map. Network implementation: `forest_stream` emits once on fetch completion;
`resolve_stream` emits `None` immediately then `Some(join)` when the network call resolves.

```rust
// Network resolve pattern
async_stream::stream! {
    yield None;                                    // loading state
    yield Some(fetch_from_network(id).await);      // resolved
}
```

Components handle both identically — the existing `let Some(join) = entry_join() else { return
rsx! {} }` pattern already covers the loading state implicitly.

### IVM and incremental Forest updates

Currently, `Stream<Item = Forest>` means rebuilding `Vec<Entry>` on every DB change. This is
acceptable for local SQLite with modest data. If this becomes a bottleneck, incremental view
maintenance (IVM via the `ivm` crate or a delta-based Forest API) would allow the Forest to apply
deltas rather than full rebuilds. This is an optimization, not a design change — the `Memo<Forest>`
interface to components stays the same.
