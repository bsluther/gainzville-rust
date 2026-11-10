# Source: discussion notes, 2026-11-04 to 2026-11-10

Notes from a voice-narrated discussion between the author and an agent, which started under RFD 3
and moved here. Kept in the order it happened. The author's points are marked; the rest is the
agent's contribution as it stood after the author's pushback. Web research results are summarized
with links; a few hosts (docs.datomic.com, blog.datomic.com, cljdoc.org, hexdocs.pm) were outside
the session's network allowlist, so those parts rest on search snippets and the agent's own
knowledge and are marked.

---

## 1. Why actions should capture intent

- **Author:** the generic `MoveEntry` (position plus temporal) forces Swift helpers like "move to
  root", "move into parent" and "update time" to fill in fields. More specific actions say more
  about what the user meant, but specificity is a trade-off, not automatically better. Hard to
  articulate why intent is desirable.
- **Agent:** the property that matters isn't specificity. An action should carry only what the user
  decided, nothing the client read from its snapshot. Actions are re-run against state the user
  never saw: in the write transaction, on sync rebase, and from the log. A snapshot-derived field is
  a stale read baked into a write. Test: replayed against slightly different state, does it still do
  what the user meant? Example: `moveEntry` sends Swift's copy of `temporal`, so a move can revert a
  concurrent time edit; `updateEntryTemporal` sends a copy of `position`, so it can revert a move.
  Side benefits: rejections speak the user's language, the log reads as decisions, and LLM import and
  the HTTP API can express intent without computing internals. Cost: each action shape becomes part
  of the log format, so shapes should be settled rather than proliferate.

## 2. Moves keep the fractional index

- **Agent (first pass, wrong):** name placement as "after sibling X" and have the mutator compute
  the index; reported "that sibling is gone" as a better rejection.
- **Author:** failing is bad. If I put something third and the second item is deleted, I don't want
  the move to fail. A classic reason for a fractional index is that it recovers: it keeps the
  conceptual position. That's an argument *for* carrying the index.
- **Agent:** agreed. The index marks a place in the order, not a reference to a neighbour, so it
  survives deletions and reorders. It is the user's decision in a replay-stable form; the copied
  temporal is the real problem. Same-gap concurrent inserts can mint equal keys on two devices; that
  needs a tiebreaker (e.g. entry id), not a different shape.
- **Author:** no new mechanism is needed to supply the index. After the GvKit refactor, Rust's
  `ChildSlots` query (`gv-ui`) uses the forest to provide the drop slots, kept current by the normal
  subscription. The drop captures the parent and index, which is the intent.
- **Author:** moving to the root must carry a time: a root log entry needs one, so position and
  time change in one atomic step. Setting time first and then moving isn't the user's intent.
- **Author:** moving from the root into a parent keeps the entry's time; the user didn't intend to
  change it.

## 3. Additions to the idea list

- **Author:** goals for code structure are composability, simplicity and elegance. Ideas already in
  play: pre- and postcondition checks, actions that capture intent, reification, property checking.
- **Agent proposed:**
  - Separate reading from deciding ("functional core, imperative shell").
  - Split preconditions (about this action) from invariants (about the state, whatever action ran);
    check invariants once against the post-change state. `DomainError` already distinguishes
    `Rejected` from `InvariantViolation`.
  - "Parse, don't validate" (Alexis King): a successful check returns a typed value that the delta
    builder requires, so changes can't be built without the checks.

## 4. Pushback on separating reading from deciding

- **Author:** there's no generic set of data to load; it depends on the property. Taking data as a
  parameter means the caller knows and provides it, but some inputs, like "sibling entries", aren't
  expressible as a type. And some checks are really queries: a duplicate-name check shouldn't take
  every entry as an argument.
- **Agent:** suggested passing the query's answer (e.g. "is there a conflicting name") into a pure
  check, plus provenance types (only a loader can build `Siblings`), and that the strict two-phase
  split breaks because what a mutator reads depends on earlier answers.
- **Author:** `no_duplicate_names(has_dup: bool)` just passes back its answer; it splits off no logic
  and enables no composition. That isn't an architecture.
- **Agent, revised:** the unit is a check that does its own reading: an async function taking the
  query executor and parameters, returning a rejection or the value it found. Checks compose by
  calling each other. Repeated reads are a mechanism problem: a per-mutator-run cache keyed by the
  query value (queries are already reified values, most deriving `Hash`/`Eq`). Mutators don't write
  until deltas are applied, so such a cache can't go stale within a run. Limit: only identical
  queries are deduplicated.
- **Author:** caching adds significant complexity; not convinced, but it's an option. The shape is
  right: functions that take a query executor and params and check properties give callers the most
  expressivity.

## 5. Research: reference systems

- **TigerBeetle** (`src/state_machine.zig`): each operation has an explicit prefetch phase
  (`prefetch_create_transfers` and callbacks) that enqueues the keys it will need and loads them in a
  batch. Execution (`create_transfer`, `post_or_void_pending_transfer`) is a flat sequence of
  `if (cond) return .result_code;` checks reading through small getters (`get_account`,
  `get_transfer`); in verify builds the getters assert the key was prefetched. Check order is
  deliberate (idempotency "exists" checks before field checks, documented in a comment). After the
  comment "After this point, the transfer must succeed" it uses deferred asserts.
  [source](https://github.com/tigerbeetle/tigerbeetle/blob/main/src/state_machine.zig)
- **FoundationDB**: library code (and the Record Layer) is functions that take a transaction and do
  their own reads. The transaction caches keys and values read and written over its lifetime, so a
  repeated read is served locally (inferred from the client-memory wiki page: disabling the cache
  "will involve making a network request to Storage Server should a key be read twice").
  [source](https://github.com/apple/foundationdb/wiki/Client-memory-considerations)
- **Turso**: runtime rules live in the schema (unique, check, foreign keys), enforced by the engine.
  Its simulator's properties pair preconditions, queries and assertions, used for testing, not to
  guard writes. [source](https://git.joshthomas.dev/mirrors/limbo/src/branch/main/simulator/README.md)

## 6. Research: one or two steps out

- **Ecto** (Elixir): validations are pure checks on the incoming data; constraints rely on the
  database (e.g. `unique_constraint` over a unique index) and are "always safe". A query-based
  uniqueness check is named `unsafe_validate_unique` because of the race between check and insert.
  Validations run before constraints.
  [source](https://hexdocs.pm/ecto/3.5.0-rc.1/Ecto.Changeset.html) (via search; the host was blocked)
- **Datomic entity specs**: a spec lists required attributes and/or entity predicates. Each
  predicate is a function of a database value and an entity id, returning true or false. The
  database value is the state *after* the transaction's changes; the predicate queries outward from
  the id as it likes. A transaction opts in with `:db/ensure`, and fails if any predicate fails.
  Affordable because database values are local and cached. Attribute predicates (`:db.attr/preds`)
  take only the value, for pure field checks.
  [Datomic blog](https://blog.datomic.com/2019/06/tuples-and-database-predicates.html),
  [Datahike entity specs](https://cljdoc.org/d/io.replikativ/datahike/0.7.1624/doc/advanced/entity-specs)
  (both via search snippets and agent knowledge; hosts blocked; confirm exact syntax)
- **DDD aggregates**: draw boundaries so each invariant's data is in one unit loaded whole.
  Set-based validation (unique username) is the known exception, answered with a database index or a
  reservation step.
- **Haxl / DataLoader**: code fetches independently; a layer underneath batches and deduplicates.

## 7. Takeaway and scope

- **Author:** the pattern across real systems is good info: pure field checks, preconditions as
  queries, invariants as postcondition queries. Open: how to make that both composable and
  performant. Maybe simple caching solves it, maybe it's not a big deal, but there should be a road
  to optimization even if every query runs separately for now.
- **Author:** this drifted from RFD 3. Mutator checks, and reassessing which actions should change to
  capture intent, are both about the action system, not the FFI. Broaden a new RFD to actions in
  general.
