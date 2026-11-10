# RFD 0004: Actions

- State: discussion
- Linear: none yet

## Question

Two questions about the action system:

1. **Shapes.** Which actions should change so that they capture what the user meant?
2. **Checks.** How should mutators organize their preconditions and invariants so that the checks
   compose, stay simple, and have a road to good performance?

Both started under [RFD 3](../0003-core-swift-boundary/0003-core-swift-boundary.md), because
`GvWrites.swift` is where the gaps showed. Neither is about the FFI. Every sender of actions is
affected: the Swift app, LLM import, the HTTP API, and sync rebase.

Inputs: the [actions discussion](../0003-core-swift-boundary/comment-2026-11-04-actions-discussion.md)
comment on RFD 3, and the [discussion notes](./source-2026-11-10-discussion-notes.md) that followed,
including the prior-art research.

## What an action carries

An action carries what the user decided, and nothing the client read from its own snapshot of the
data. Specific actions often follow from this, but specificity isn't the goal.

This matters because an action is re-run against state the user never saw: in the write
transaction, on sync rebase, and from the log. A field copied from the client's snapshot is a stale
read baked into a write.

**Test:** replayed against slightly different state, does the action still do what the user meant?

**Cost:** once the mutation log stores real data, each action shape is part of its format. Shapes
should be settled, not multiplied.

## Shapes

### Moves (lean, 2026-11-04)

First recorded in RFD 3. The actions-discussion comment proposed naming placement as "first, after a
given sibling, or at the end", computed into an index by the mutator. That is reversed here.

A fractional index marks a place in the order, not a reference to a neighbour. It still lands where
the user meant after neighbours are deleted or reordered, whereas "after X" breaks exactly when X
goes away. The index passes the test above. The copied `temporal` in `moveEntry`, and the copied
`position` in `updateEntryTemporal`, fail it.

No new mechanism is needed to supply the index. `ChildSlots` (`gv-ui`) lays out a sequence's drop
slots in Rust, each with the `Position` that lands an entry there, and GvKit keeps it current
through the normal subscription. A drop captures the slot's parent and index.

**Lean:**

- **The move's target says what it carries.**
  - Under a parent: the parent and fractional index.
  - To the root: a start time. A root log entry must be on the timeline, so position and time change
    in one atomic step. The mutator, not Swift, decides what happens to the existing duration.
- **Moving from the root into a parent keeps the entry's time.** The user didn't decide to change it.
- **Editing time alone is its own action,** so it no longer carries a copy of the position.
- **Concurrent inserts into the same gap** can mint equal indices on two devices. That needs a
  tiebreaker (for example the entry id) when sync lands, not a different action shape.

### Other candidates (not yet discussed)

From the actions-discussion comment. These are its recommendations, not leans:

- **Ids go into the create actions.** `CreateEntryFromActivity` and `DuplicateEntry` mint ids in the
  mutator (`mint_id_map`), so a replay mints different ids and a later action that refers to them
  points at nothing. `ConvertToSets` already carries its id.
- **Derive `is_template` in the mutator** for child creation and `CreateEntryFromActivity`, instead of
  passing the parent's flag from Swift and rejecting a mismatch.
- **Thin Rust-side conveniences:** default start times, filling in the actor, returning created ids
  and a version.
- **Timing:** settle shapes before the mutation log stores real data
  ([RFD 2](../0002-dogfooding/0002-dogfooding.md)).

## Checks

### The problem

`move_entry` keeps all its checks in one function. That's easy to follow, and it reads each piece of
data once. But splitting moves by target, or sharing checks between actions, means either copying
logic or pulling each condition into its own helper. If each helper queries for itself, one write can
run many queries, several of them the same.

Taking the data as a parameter doesn't fix that. What a check needs depends on the property: some
inputs aren't natural types ("the siblings of this entry"), and some checks are really queries (a
duplicate-name check shouldn't take every entry as an argument).

### Prior art

Summarized from the [discussion notes](./source-2026-11-10-discussion-notes.md), which have links.

- **TigerBeetle:** an explicit prefetch phase loads a declared read set. Then a flat, deliberately
  ordered list of inline checks reads through getters that assert the key was prefetched.
- **FoundationDB:** library functions take a transaction and do their own reads. The transaction
  caches what it reads, so repeated reads are local.
- **Turso:** runtime rules are schema constraints. Properties with preconditions and assertions live
  in the simulator.
- **Ecto:** pure validations first, then database constraints. A query-based uniqueness check is
  labeled "unsafe" because it races.
- **Datomic:** entity predicates are functions of (database after the transaction, entity id) that
  query freely. A transaction opts in to the ones it must satisfy, and fails if any don't hold.
- **DDD:** aggregates put each invariant's data in one loadable unit. Uniqueness is the known
  exception, left to an index.
- **Haxl / DataLoader:** independent fetches, with a layer underneath that batches and deduplicates.

The common pattern has three layers:

1. **Pure field checks.**
2. **Preconditions as queries.**
3. **Invariants as queries against the post-change state**, failing the transaction.

### Current thinking

Not a lean yet.

- **A check is a function of the query executor and parameters** that does its own reads and returns
  a rejection or the value it found. Checks compose by calling each other. This gives callers the
  most expressivity.
- **Run every query separately for now,** but keep a road to optimization:
  - A cache for one mutator run, keyed by the query value (FoundationDB). Queries are already
    reified values, and most derive `Hash`/`Eq`. Mutators don't write until their deltas are
    applied, so the cache can't go stale within a run. It only removes identical queries, and it
    adds real complexity.
  - A declared read set loaded up front (TigerBeetle), with getters that assert they were
    prefetched.
- **Invariants run against the post-change state** inside the SQLite write transaction, which rolls
  back on failure (Datomic). `DomainError` already separates `Rejected` from `InvariantViolation`.
- **Some invariants belong in the schema,** such as a unique index (Ecto, Turso).
- **Check order is part of the design** (TigerBeetle), because it decides which error a caller sees.

## Open

- **Caching:** worth its complexity, or not needed at GV's scale?
- **Invariant checks:** where they're listed, either opted into per action (as with Datomic's
  `:db/ensure`) or run for every action. They might also feed deterministic simulation as properties.
- **Typed check results** ("parse, don't validate"): a successful check returns a value the delta
  builder requires.
- **Other action shapes:** the candidates above, and a pass over the rest of `Action`.
- **Linear issue.**
