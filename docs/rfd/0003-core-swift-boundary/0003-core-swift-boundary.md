# RFD 0003: The core–Swift boundary

- State: discussion
- Linear: none yet

## Latest / lean (2026-10-29)

This records the shape that came out of a follow-up discussion
([notes](./source-2026-10-29-discussion-notes.md), including a read of the Convex and Ghostty source)
and a survey of open-source prior art ([research](./source-2026-10-29-prior-art-research.md)). It
hasn't been compared against the research in depth yet. It's written down so the shape doesn't get
lost while that happens. Where it differs from the [earlier lean](#earlier-lean-2026-10-27), this
section wins.

### Reframe

- **Keep the query seam.** Most systems expose queries for one of two reasons. Generality is one,
  and it doesn't apply to GV. A clean declarative contract is the other, and it does. So GV keeps
  queries and changes what sits around them.
- **The boundary is the FFI plus a Swift SDK, GvKit, owned by the core side.** This is the clearest
  takeaway so far, and it holds whatever else is decided. GvKit is a Swift package that holds the
  generated bindings, the delivery adapter, the query hook and the intent calls. The app imports only
  GvKit, and the raw FFI types stay internal to it. It changes when the boundary changes, not when
  screens change.

### Shape

1. **Everything is a query, used two ways.**
   - *Subscribe*, declared in a view or an `@Observable` model:
     `@GvQuery(.children(of: parent.id)) var children`.
   - *Read once*, for imperative checks such as drag-time validation:
     `gv.read(.wouldCreateCycle(entry, under: target))`.

   The forest traversal methods are queries like any other: `children(of:)`, `parent`, and "forest
   for this day".
2. **Delivery is pushed and batched, modeled on Convex's `watch_all`.** `subscribe` returns the first
   value. After each change, Rust makes one FFI callback carrying a version and the results of every
   subscription that changed. GvKit applies the whole batch in one main-thread step and routes each
   result to its hook. Views never see a torn state, never call into Rust while rendering, and get
   nothing when their result didn't change.

   The model is `convex-rs`'s `watch_all()`: a stream of `QueryResults`, a copy-on-write map from
   subscriber to latest result, where "each item in the stream contains a consistent view of the
   results of all the queries in the query set." GV keeps that idea of one consistent map per version,
   with each hook picking out its own entry. It differs in sending only the changed entries across the
   FFI.
3. **Rust picks the backing for each kind of query, and Swift can't see the choice.** Forest queries
   are answered from an in-memory forest; others, such as autocomplete history, from SQL. The snapshot
   handle is internal machinery. This leaves every optimization open, without committing to one:
   - naively re-running every query on every change;
   - custom IVM;
   - keeping only part of the forest in memory and loading more by day. A thorough version merges
     overlapping "forest for an interval" queries into one forest and knows when a traversal walks off
     the loaded edge.
4. **Writes are intents.** Unchanged from the earlier lean.
5. **Pure, screen-shaped computations move to Rust, in a UI-shaped layer separate from the core.**
   Structure goes in Rust and formatting stays in Swift. Rust decides *where* things go and what is
   allowed. Swift decides *how* it looks, including locale-dependent text such as "7:30 – 8:15". See
   the worked example below.

### Screen-shaped computations in Rust: drop slots

**Inspiration: Matrix's date dividers.** In matrix-rust-sdk, Rust decides where day dividers go in a
timeline.

- **The logic.** `DateDividerAdjuster` (`crates/matrix-sdk-ui/src/timeline/date_dividers.rs`, about
  700 lines) inserts or removes divider items whenever messages are added, including older history
  loaded at the top.
- **Invariants.** It checks its own invariants (never two dividers in a row, never one at the end),
  and tests panic if they break.
- **What Swift sees.** Dividers reach Swift as ordinary list items:

```rust
pub enum VirtualTimelineItem {
    DateDivider(MilliSecondsSinceUnixEpoch),  // a day, in local time
    ReadMarker,
    TimelineStart,
}
```

Element X just maps the case to a view and formats the date:

```swift
case .dateDivider(let timestamp):
    let date = Date(timeIntervalSince1970: TimeInterval(timestamp / 1000))
    return SeparatorRoomTimelineItem(id: .virtual(uniqueID: uniqueID), timestamp: date)
```

**Organization.** Matrix keeps this out of its core. `matrix-sdk` is the client (protocol, storage,
sync) and has no UI concepts. `matrix-sdk-ui` holds the screen-shaped but platform-independent logic
(timeline, date dividers, read markers, the room list service). `matrix-sdk-ffi` exposes both. GV
would add a parallel layer:

- a `gv-ui` crate, or a module in `gv-client`, of pure functions over the forest, with no storage and
  no platform code;
- its results returned through ordinary queries;
- `gv-core` staying domain-only, so drop slots never leak into the domain model.

The logic stays pure Rust, so it is property- and simulation-testable, and a future Android or
desktop frontend could reuse it.

**The GV case: drop slots between children.** Today `ChildrenSection` (`EntryView.swift:291`) builds
"drop target, child, drop target, child, drop target" itself:

- it calls `positionBetween` for every gap to get a fractional position;
- it invents slot ids so SwiftUI keeps the right identity after a drop;
- it does both on every render.

That's structural logic about fractional indices, living in a view. In Rust, a query would return the
slots already laid out:

```rust
enum ChildSlot {
    Drop { id: SlotId, position: Position },  // gap between two siblings
    Entry(EntryJoin),
}
// query: .childSlots(of: parent)
```

```swift
ForEach(slots) { slot in
    switch slot {
    case .drop(let id, let position): DropTarget(position: position)
    case .entry(let join): EntryView(join: join)
    }
}
```

That gives three things:

- positions computed once per version, not once per gap per render;
- invariants tested in Rust (slots alternate, and each position sits strictly between its
  neighbors);
- slot identity supplied by Rust, so Swift never handles fractional indices.

**What stays in Swift.** Whether a slot is valid *for the item being dragged* depends on UI state:
the dragged item, and whether the drop would cycle. That stays a read-once query at drag time
(`gv.read(.wouldCreateCycle(...))`). The slot list itself is plain data, the same way Matrix's
dividers don't depend on what the user is doing.

**Other candidates for the same pattern:** day grouping, set numbering, and affordance flags on
records (Matrix's `is_editable` and `can_be_replied_to`; for GV, "can add child" and "can clear this
temporal field").

### Reference point: Convex's Rust client, not its Swift SDK

Convex's layers relate to consistency differently, and the difference matters for GV:

- **The Rust client (`get-convex/convex-rs`) is a good reference.** Subscriptions drop with their
  token, and `watch_all()` delivers one consistent view across all subscribed queries. Its stream
  also skips lagged broadcast messages on purpose ("Ok to be lagged ... since Convex only guarantees a
  newer value than the previous value"). That's exactly the fix for GV's refresh loop, which exits on
  `Lagged`.
- **The mobile FFI and Swift SDK (`convex-mobile`, `convex-swift`) drop that guarantee.** Each
  subscription gets its own Rust task and FFI callback, and Swift applies each one separately, so
  queries aren't consistent with each other on Swift. Convex's React client does keep them consistent.

GV's shape is closer to `convex-rs` than to `convex-swift`. GV still differs in some places:

- **`subscribe` returns the first value synchronously.** The data is local; Convex's subscribe is a
  network call.
- **Read-once uses the latest local version.** Convex's one-off query is a server round trip.
- **Rust chooses the backing per kind of query.** Convex has no equivalent, because every query is a
  server function.
- **No optimistic-update layer yet.** Local mutators make a local write the truth. That changes once
  sync exists.

### Reference point: SQLiteData for the hook

Point-Free's SQLiteData (`@FetchAll`, `@FetchOne`, `@Fetch`, built on GRDB) is good inspiration and
a good design space for `@GvQuery`. It isn't settled as the shape, but it raises the right questions.
What it does:

- **Where it's used.** The wrapper is declared in a SwiftUI view or in an `@Observable` model.
- **Lifecycle.** It is a property wrapper and a SwiftUI `DynamicProperty`: it subscribes when SwiftUI
  sets the view up and cancels when the view goes away.
- **First value.** It arrives immediately, so there is no empty first frame.
- **Swapping queries.** The query can change at runtime (`$items.load(newQuery)`).
- **Sharing.** Identical queries share one subscription, like GV's refcount.
- **Loading and errors are side channels** (`$items.isLoading`, `$items.loadError`). The value is
  always present: the default first, then the last known value. In effect it's a hook-shaped result
  type. Views render what they have, and only the ones that care check loading or error. That suits
  lazy data (other users' data, activity library media) better than forcing every view to switch on
  an enum. A per-record status enum, like WordPress's fresh/stale/failed, is still the right tool for
  sync status.

Where GV's shape goes further: each SQLiteData observation runs in its own transaction, so hooks are
consistent with each other only when reads are bundled into one `FetchKeyRequest`. GV's versioned
batch makes every hook consistent. `FetchKeyRequest`-style grouping may still be handy, but GV
doesn't need it for correctness.

**Dependency.** SQLiteData is built on Point-Free's Sharing library. GvKit could implement Sharing's
key type and get the wrapper, loading state and sharing for free. **Lean: avoid the Point-Free
dependency** unless writing our own turns out to be a heavy lift or the dependency brings a big
architectural win. Copying the design looks like a few hundred lines.

### Settled for now

- **Read-once sees the latest version.** It could disagree with what the hooks are showing. That's
  accepted for now, because there's a fix if needed (force every subscription to refresh to the
  latest version), and intents are validated in Rust anyway.
- **Performance work is deferred.** That covers an indexed forest, `Arc` snapshots and finer-grained
  invalidation. The boundary only has to not rule them out.

### Open

- **A macro for query plumbing.** Adding one query touches about six Rust files plus binding
  regeneration. If everything is a query, that cost is the real tax, and a macro is the likely
  answer.
- **A loading state in the hook.** Design it now for data that loads lazily: other users' data (for
  example coaching), activity library content such as workout videos, and later virtualization.
- **Compare against the research in depth.**
  - Matrix: typed diffs, capability flags on records, and display items built by the core.
  - SQLiteData: `@FetchAll` ergonomics and `FetchKeyRequest` for one consistent multi-read.
  - Crux: intents and deterministic testing.
- **The hook's form.** A property wrapper, an `@Observable` model, or both. AsyncSequence over
  Combine.

## Earlier lean (2026-10-27)

This summarizes the [voice note](./source-2026-10-27-transcript.md) and a read of everything from
`gv-core` outward: `gv-ffi`, `gv-client` (`client/`), the Swift app's `Core.swift` and its callers,
and the inspiration projects.

### Question

Is "subscribe to a query" the right place for the FFI boundary? Or should the core expose
something at the domain level, a store for the user that already knows the app needs the forest,
the activities and the attributes? Is the logic piling up in Swift view models a sign that the
boundary is in the wrong place?

### Three separate choices

"Where is the boundary?" bundles three choices that can be made independently:

1. **Declaration: how a view states what it reads.** Today each view gets its data through a
   hand-written view model. The alternative is to declare the query in the view, the way a Convex
   component calls `useQuery`.
2. **Backing: what answers those reads.** Today it's a cache of SQL results, one entry per
   subscribed query, each re-run on every write. The alternative is one versioned in-memory model
   in Rust that answers every read from the same version.
3. **Writes: what a view sends.** Today Swift sends record-level `Action`s that it assembles
   itself. The alternative is intent-level commands that Rust turns into `Action`s.

The voice note's two perspectives map onto these. "Keep subscription-to-query and rethink what
Swift builds on it" is mostly about choice 1. "Give me the store for this user" is mostly about
choices 2 and 3.

### Lean

- **Declaration: views declare their reads; no hand-written view model per query.** Data the core
  always holds in memory (forest, entry joins, activities, attributes) is read from the current
  snapshot. Reads that take parameters and aren't always loaded (autocomplete history, later the
  remote library) go through a `@GvQuery` property wrapper. The wrapper bundles subscribing,
  reading, refreshing and unsubscribing into one declaration.
- **Backing: one versioned in-memory model in Rust.** Every read comes from a single version, so
  reads are coherent by construction, and a write doesn't trigger one SQL query per subscription.
  The model is the one `generation` already has, so deterministic simulation covers what the UI
  reads.
- **Writes: intent commands in Rust that emit `Action`s.** Rust fills in the id, actor, position
  and template flag and returns typed errors, so Swift no longer copies the domain rules.
- **UI state stays in Swift**, and Swift view models remain where there is real UI logic to hold.
- **Why.** This app is the only consumer of subscriptions. The core already knows what the app
  needs. And everything assembled in Swift is outside the Rust tests and deterministic simulation.
- **Not a rewrite.** The three choices can land one at a time, and the declaration change works on
  today's backing.

### What the boundary is today

The design docs ([design.md](../../swift-architecture/design.md),
[gv-ios-architecture.md](../../swift-architecture/gv-ios-architecture.md)) put a store in Rust.
What got built is lower-level:

| | Designed | Built |
|---|---|---|
| Read model | In-memory AppState: Forest + EntryPool, one coherent snapshot | `QueryStore`: a `HashMap<AnyQuery, AnyQueryResponse>`. Every write re-runs every cached query against SQLite |
| Consistency | Every read from one point in time | Each query re-runs on its own pooled connection, with no shared read transaction. Results enter the cache one key at a time |
| Forest | Maintained in memory | Rebuilt from a clone of the `AllEntries` vector on every `forest_*` call |
| Entry data | Pre-joined and read alongside the forest | One `FindEntryJoinById` subscription per mounted `EntryView` |
| Swift observation | `@Observable` with one refresh path | `ObservableObject` + Combine: three app-wide view models plus a `DataChange` fan-out |
| Actor | Embedded in `GainzvilleCore` | Passed to the constructor but unused. Swift hard-codes `SYSTEM_ACTOR_ID` in 25 places |
| Moves | `move_entry(id, after, before)`, with Rust computing the index | Swift calls `forest_position_between`, then sends `MoveEntry` with the resulting `Position` |

[forest-ui-model.md](../../forest-ui-model.md) records that the Dioxus app tried one subscription
per entry, hit problems, and planned one Forest plus one join map instead. The Swift app rebuilt
the per-entry pattern. That lesson didn't make it across the move to Swift.

**How a view gets data today.** Subscription *lifetimes* work as intended. Each subscription
returns a token, and dropping the token decrements a refcount and evicts the cache entry. The view
models hold tokens correctly:

- The app-wide view models keep theirs for the life of the app.
- `EntryViewModel` is a `@StateObject`, so its token drops when its view leaves the hierarchy.
- `TextSuggestionsModel` subscribes on focus and releases on blur.

What's missing is packaging. Every consumer repeats five steps by hand:

1. Subscribe and hold the token.
2. Read once.
3. Attach a sink to `DataChange`.
4. Re-read on each tick, passing the same query value again.
5. Unwrap the matching case of `AnyQueryResponse`.

The failures so far all come from this protocol, not from the lifetimes:

- **Missing sink.** `AttributesViewModel` holds its token but never attached the sink, so the Rust
  cache stays fresh while the UI goes stale.
- **Silent misses.** `readQuery` returns `nil` when the query isn't subscribed.
- **Hidden dependency.** The `forest_*` reads depend on `ForestViewModel` holding the `AllEntries`
  token. This is behind the cold-start race in `EditAttributesView.swift:50`.
- **Lifetime as a cost lever.** `TextSuggestionsModel` drops its subscription on blur because each
  live subscription costs one SQL query on every write.

So the boundary is lower-level than designed, and Swift is building the store that the design put
in Rust. That accounts for most of the "amalgam" in the voice note.

### What's in Swift, by kind

1. **UI state: belongs in Swift.** Shadow edit state, debounce, focus, drag state, expansion and
   display formatting. The Numeric, Mass, Length, Text, Select and Temporal editors each repeat
   the same shadow-edit machinery (`syncEditState`, `scheduleDebounce`, `flushNow`,
   `PendingWrite`). That is worth consolidating, but as a Swift-internal refactor. It isn't a
   boundary problem.
2. **Store plumbing: should move out of app code.** This is the five-step read protocol above,
   plus a few workarounds:
   - the refresh fan-out order (`GainzvilleApp.swift:39`);
   - `_ = roots` reads that exist only to fake a SwiftUI dependency (`Core.swift:122`, `:133`,
     `:138`, `:143`);
   - `pendingExpandedEntryIds`, which covers the gap between a write and the refresh that shows
     it (`Core.swift:348`);
   - lazy template-root resolution (`EditAttributesView.swift:50`).

   Each one makes up for a primitive the boundary doesn't offer: a packaged read, a coherent
   snapshot, read-your-writes, or a single change path.
3. **Domain rules: duplicated because the boundary can't express them.**
   - *Action construction.* Swift builds full `Entry` records, mints UUIDs, copies `isTemplate`
     from the parent (`Core.swift:291`), and rewrites the temporal when an entry moves to root
     (`Core.swift:169`).
   - *Pre-validation.* Swift checks the temporal 2-of-3 rule and the rule that a root log entry
     keeps a start or end (`TemporalAttribute.swift:111-152`). It also clamps and rounds numerics
     to 2 decimal places "because core rejects … a rejected write would fail silently"
     (`NumericAttribute.swift:284`).
   - *Derived values.* Swift infers the duration from start and end
     (`TemporalAttribute.swift:253`) and builds the temporal summaries.

   There are two causes. Actions are record-level, so Swift has to assemble the record. And errors
   are swallowed: 27 `try?` calls into the core, 18 of them `runAction`. So Swift validates up
   front rather than reacting to the core's answer.

### Problems found (whatever the decision)

1. **The cache stops refreshing for good after a burst of writes.** The `QueryStore` refresh task
   loops on `while let Ok(()) = rx.recv().await` over a tokio broadcast channel with capacity 16
   (`client/query_store.rs:50`, `client/client.rs:121`). When writes outpace refreshes, `recv`
   returns `Err(Lagged)`, the loop exits, and no subscribed query refreshes again until the app
   restarts.
   - *Confirmed* with a throwaway test against file-backed SQLite on a multi-thread runtime. With
     1 subscription, 200 quick writes leave the cache permanently stale: a later write never
     appears. With 20 subscriptions, 40 writes are enough.
   - The listener loop in `gv-ffi/src/core.rs:84` has the same shape.
   - Paths that will hit it: LLM import of a training log (a project goal), sync catch-up, and the
     dev seeding helpers.
   - *Fix:* treat `Lagged` as "something changed", coalesce, and keep looping. That is the meaning
     the channel needs anyway.
2. **Reads are consistent only once the app goes quiet.** Every query does re-run on each change,
   but nothing guarantees they all describe the same moment.
   - In `refresh_subscribed_queries` (`client/query_store.rs:167`), each query takes its own
     connection from the pool (`:64`), with no shared read transaction. If a second write commits
     partway through a refresh, the earlier queries see the state before it and the later ones the
     state after.
   - Results enter the cache one key at a time, with the lock released between keys. Swift reads
     synchronously whenever SwiftUI renders, not only after the notification, so a render can see
     a half-updated cache.
   - A new subscription runs its first query whenever it is created, not at the cache's current
     state.
   - A refresh error skips the remaining keys, but `on_cache_ready` still fires.

   With no other writes in flight, the cache is consistent once the notification fires. Under
   concurrent writes it is only eventually consistent.
   - *Fix within today's backing:* run the whole refresh inside one read transaction on one
     connection (SQLite gives a read transaction a consistent snapshot). Build a new map and swap
     it in all at once, tagged with a version number.
3. **No read-your-writes.** `run_action` returns after the commit but before the refresh
   (`client/client.rs:233`), so Swift's next read is stale until the callback arrives.
   `pendingExpandedEntryIds` and the scroll-follow open question in
   [SWIFT-APP.md](../../../swift-app/SWIFT-APP.md) are both workarounds for this.
4. **Cost grows with total history, not with what's on screen.**
   - Each `forest_*` call clones the whole `AllEntries` vector out of the mutex and scans it
     linearly (`gv-ffi/src/core.rs:320`, `core/src/forest.rs`).
   - `ChildrenSection` calls `positionBetween` for every gap plus `children()` on every render
     (`EntryView.swift:291`). These calls run in `body`, so SwiftUI can't skip them.
   - Every change re-runs every per-entry join query in SQL, one after another.
   - Each newly mounted `EntryView` subscribes with a synchronous SQLite query on the main thread
     (`subscribe_query` → `block_on`).

   This is fine for a dev database. Years of dense data is where it will show.
5. **Invalidation is coarse.** 11 view files take `forestVM` as an `@EnvironmentObject`. That
   includes every attribute editor, which uses it only to dispatch writes. Any `roots` publish
   invalidates all of them, and then `DataChange` republishes every `EntryViewModel`. This is the
   "change notification soup" that Ghostty's macOS app refactored away from (PR #7523).
6. **UUID case mismatch.** IDs cross the boundary as `String`. Swift mints uppercase
   (`UUID().uuidString`) and the core emits lowercase. `convertToSets` lowercases to compensate
   (`Core.swift:352`). The three other places Swift mints IDs don't, so a future equality check
   against a Swift-minted ID will miss.

### Choice 1: declaration

**View models (today).** Each read is a class implementing the five-step protocol. Most of these
classes contain no UI logic. `EntryViewModel`, `TextSuggestionsModel` and
`AttributeDetailViewModel` are each a hand-written copy of one generic pattern.

**Queries declared in the view.** In Convex, a component calls
`useQuery(api.entries.forDay, { day })` and has no view model. Convex isn't a general query
language either: `api.entries.forDay` is a named function the app developer wrote. So GV's
`AnyQuery` is already the same kind of thing, a named, domain-language, parameterized query.
Nothing stops GV from declaring queries in views. A SwiftUI `DynamicProperty` (the mechanism behind
SwiftData's `@Query`) can hold the token, re-read on change, re-target when its parameters change,
and release on disappear. That gives you `@GvQuery(...) var join` in the view.

Three things currently stand in the way of view-shaped queries:

- **Adding a query is expensive.** One query (`DistinctTextValuesForAttribute`) touches six Rust
  files: `queries.rs`, `query_executor.rs`, both SQL executors, `query_store.rs` and `types.rs`.
  Then the bindings need regenerating. In Convex it's one function. So GV views use the existing
  entity-shaped queries and assemble the rest themselves.
- **Forest reads aren't queries.** `children`, `positionBetween` and `templateRoot` are side
  methods that read the `AllEntries` cache entry, so a view can't declare them.
- **Unwrapping responses is untyped.** Matching on `AnyQueryResponse` makes every caller pick the
  right case by hand. The wrapper needs a typed result per query, either a Swift helper per query
  or typed read functions exported from the core.

**Lean:** declare reads in views. Snapshot reads (choice 2) cover the always-loaded data with no
subscription at all, and `@GvQuery` covers the rest. Because the wrapper hides the backing,
changing choice 2 later doesn't touch views.

### Choice 2: backing

**Cache of SQL results (today).** It is simple and reuses the SQL executors. Its costs are
problems 2 to 4 above: no coherent snapshot, no read-your-writes, and one SQL query per
subscription on every write. Fixing consistency in place takes one read transaction plus an
atomic swap (problem 2). Fixing cost needs fewer, coarser queries.

**Versioned in-memory model.** Rust holds the user's data in memory and builds an immutable
snapshot per version: an indexed forest plus a join map. Reads are coherent by construction and
never touch SQLite. Writes apply each `Mutation`'s deltas to the model after the SQLite commit. The
model already exists: `generation::Model` has entity hash maps, an `AnyDeltaExecutor` impl and
`apply_mutation`. Moving it out of the test crate (open question 2 in design.md, `gv-model`) makes
the client store and the simulation model the same code.

The voice note's argument decides this one. The server wants to issue queries, not subscribe to
them, so subscriptions exist only for this app, and keeping the backing generic preserves no
generality. The core knows the client needs the whole forest, all activities and all attributes,
and [forest-ui-model.md](../../forest-ui-model.md) already assumes one user's full forest fits in
memory.

Convex shows the contrast. Its client shows new results only once every active query has
advanced to the same database timestamp. GV gets that guarantee for free from a snapshot, and only
with extra work from the per-query cache.

**Lean:** the in-memory model, reached in steps.

### Choice 3: writes

**Record-level `Action`s (today).** Swift assembles the full record, which is how domain rules
leaked into Swift (category 3 above).

**Intent commands.** Rust turns "add a child here" into a `CreateEntry` with id, actor, position
and `is_template` filled in. `Action` stays the canonical write record, so the mutation log, sync
and DST keep working on Actions. Each intent returns typed errors and the new version (plus any
created ids), so Swift can wait for that snapshot. That gives read-your-writes without
`pendingExpandedEntryIds`, and any pre-validation left in Swift is UX polish backed by affordances
from the core.

**Lean:** intents, starting where Swift holds domain logic today.

### Prior art

- **Ghostty.** The boundary is a domain object, not a data query.
  - Swift holds a `ghostty_surface_t` per terminal view and forwards input (keys, mouse, resize).
    The core owns all terminal state.
  - Core-to-Swift traffic goes through one typed action callback: a tagged union of actions such
    as new tab and set title. It replaced a set of one-off callbacks ("unified action dispatch").
  - Swift keeps its own state only for layout (`SplitTree`), as an immutable value that gets
    replaced wholesale.

  *Lesson:* the core owns state and operations at the granularity the app thinks in, and views
  render values.
- **Convex.** Components declare reads with `useQuery` against app-written, domain-named query
  functions that are cheap to add and return view-shaped data. The client guarantees a consistent
  view across all active queries. The library bundles subscribing, reading, re-rendering and
  unsubscribing into one call. Mutations are named functions that take intent-level arguments.
  Read-set invalidation is already noted in [convex-evaluation.md](../../convex-evaluation.md).
- **matrix-rust-sdk / Element X.** Not on your inspiration list, but
  [gv-ios-architecture.md](../../swift-architecture/gv-ios-architecture.md) names it the closest
  production analog. Its `matrix-sdk-ui` crate puts view-shaped state in Rust (`Timeline`,
  `RoomListService`) and pushes `VectorDiff` updates to a Swift listener. Element X's Swift layer
  wraps these in thin proxies. That goes further than GV needs (view models in Rust), but it shows
  the store-in-Rust shape working at scale over UniFFI.
- **TigerBeetle.** Every language client sits on one well-tested `tb_client` written in Zig. The
  stated reason is to avoid bugs and divergent behavior across reimplementations. *Lesson:* logic
  in the binding layer is outside the test harness and gets duplicated per platform. For GV, store
  logic in Swift is outside DST and would be rewritten for any other frontend.
- **Turso.** One Rust core with thin bindings for many languages. A secondary source (not checked
  against the code) says the offline sync engine is written once in Rust with its I/O injected, so
  each binding just drives it. *Lesson for sync:* the store Swift reads should be the same Rust
  object that sync writes into.

The common thread: the core exports stateful domain objects and commands. The host declares what
it reads, forwards intent, and renders values. None of these projects makes app code hand-wire a
query cache.

### Proposed shape

In Rust, the logic lives in `gv-client`. `gv-ffi` stays a declaration layer.

```rust
// One handle per signed-in user.
impl GainzvilleCore {
    fn snapshot(&self) -> Arc<Snapshot>;   // immutable, versioned
    // One method per user intent. Each resolves against the current model and emits Action(s).
    fn create_child_entry(&self, parent: Uuid, spec: NewEntry) -> Result<Created, FfiError>;
    fn move_entry(&self, id: Uuid, to: MoveTarget) -> Result<Version, FfiError>; // under a parent between siblings, or to root at a time
    fn set_temporal(&self, id: Uuid, edit: TemporalEdit) -> Result<Version, FfiError>;
    // ...
    fn subscribe_query(&self, q: AnyQuery) -> Result<Arc<FfiQuerySubscription>, FfiError>; // parameterized reads, behind @GvQuery
}

impl Snapshot {   // uniffi Object; every method reads the same version
    fn version(&self) -> u64;
    fn roots_in(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<Entry>;
    fn children(&self, parent: Uuid) -> Vec<Entry>;
    fn entry(&self, id: Uuid) -> Option<EntryJoin>;         // replaces per-entry subscriptions
    fn drop_slots(&self, parent: Uuid) -> Vec<DropSlot>;    // positions computed once
    fn template_root(&self, activity: Uuid) -> Option<Entry>;
    fn activities(&self) -> Vec<Activity>;
    fn attributes(&self) -> Vec<Attribute>;
    fn temporal_affordances(&self, id: Uuid) -> TemporalAffordances; // what can be set or cleared
}

trait CoreListener { fn on_data_changed(&self, version: u64); }
```

In Swift:

```swift
// Always-loaded data: read from the current snapshot.
@Environment(Store.self) private var store
let join = store.snapshot.entry(id: entry.id)

// Parameterized reads: declared in the view; the wrapper owns the token and refresh.
@GvQuery private var history: [String]
init(attributeId: String) {
    _history = GvQuery(.distinctTextValues(attributeId: attributeId))
}
```

- **One store.** A single `@Observable @MainActor` store holds the current `Snapshot` and swaps it
  on `on_data_changed`. `ForestViewModel`'s ~20 action wrappers become thin calls, or an extension
  on the core. `EntryViewModel`, `DataChange` and `CoreEnv` go away.
- **Views take values.** Views receive values (`EntryJoin`, child lists) as inputs instead of
  calling Rust from `body`, so SwiftUI's diffing can skip unchanged subtrees. More precise
  invalidation (changed ids per version, or matrix-style diffs) can come later behind the same
  API.
- **Remaining view models hold UI logic.** UI state stays in Swift: shadow edits, debounce, focus,
  drag, expansion and display formatting. Consolidating the six copies of the shadow-edit
  machinery is a separate job.

### Virtualization

Agreed that it isn't a priority. The shape above leaves it open without letting it drive the API.
Snapshot reads are methods, not a shipped vector, so `roots_in(day)` could later be served from
day-bucketed lazy loads ([forest-ui-model.md](../../forest-ui-model.md)'s `EntrySource` and "lazy
loading local data") with no change on the Swift side. A day that isn't loaded could also become a
`@GvQuery`. What would close that door is shipping the whole forest to Swift as one value, so avoid
that.

### Path

Fix now, whatever the decision:

1. The `Lagged` loop bug, in both loops, with a regression test.
2. Make the refresh coherent: one read transaction, a whole-map swap, and a version number.
3. Return core errors to Swift instead of `try?`. At minimum, log them.
4. Use the actor the core was constructed with, and stop passing `SYSTEM_ACTOR_ID` from Swift.

Then, in steps, each shippable on its own:

1. **Declaration.** Add `@GvQuery` over today's subscribe/read, and move the per-query view models
   (`EntryViewModel`, `TextSuggestionsModel`, `AttributeDetailViewModel`, the read side of
   `EditAttributesViewModel`, `AttributesViewModel`) onto it. This fixes the missing-sink class of
   bug and needs no Rust change.
2. **Backing.** Build a versioned `Snapshot` once per change, with the forest indexed once. Forest
   reads come from it.
3. **Joins in the snapshot.** Entry reads move from per-entry subscriptions to `snapshot.entry`.
4. **Intents**, starting where Swift holds domain logic: creating root and child entries, moving to
   root, converting to sets (returning the new id), and temporal edits with affordances.
5. **An `@Observable` store**, with views taking values.
6. **A delta-applied in-memory model** shared with `generation`.

### Open questions

- **One read mechanism or two?** Should the forest reads also become queries, so every read goes
  through `@GvQuery`? Or is "snapshot for always-loaded data, `@GvQuery` for the rest" the right
  split?
- **Cheaper queries.** Can adding a query get cheaper, for example queries answered from the
  in-memory model that need no SQL executor? That decides how view-shaped the queries can be.
- **Typed reads.** Should typed results come from Swift helpers per query, or from typed read
  functions exported from the core?
- **Other frontends.** Will there be other UI consumers (Android, desktop, web)? The more there
  are, the more moving logic into Rust pays off. With Swift alone, the DST argument still holds.
- **Intent granularity.** One method per intent, or an `Intent` enum that mirrors `Action`? An
  enum is easier to log and replay. Methods read better in Swift.
- **Snapshot memory.** Should the snapshot hold every value for every entry? forest-ui-model.md
  says one user's data fits in memory. Confirm with a size estimate for years of dense logging.
- **Sync.** Does the store become the sync replica, with sync writing into the same model? Lean
  yes. That's the Turso lesson.
- **Rust-side presentation.** Is a presentation layer in Rust (matrix-style timeline diffs) ever
  warranted? Lean no, unless profiling says otherwise.

## Sources

- [Voice note transcript, 2026-10-27](./source-2026-10-27-transcript.md)
- Code: `gv-ffi/src/core.rs`, `client/client.rs`, `client/query_store.rs`, `core/src/forest.rs`,
  `generation/model.rs`, `swift-app/Gainzville/Core.swift`, `GainzvilleApp.swift`,
  `Features/Log/EntryView.swift`, `EditAttributesView.swift`, `Features/Log/Attributes/*`
- Docs: [design.md](../../swift-architecture/design.md),
  [gv-ios-architecture.md](../../swift-architecture/gv-ios-architecture.md),
  [codebase-assessment.md](../../swift-architecture/codebase-assessment.md),
  [ghostty-zig-swift-architecture-research.md](../../swift-architecture/ghostty-zig-swift-architecture-research.md),
  [forest-ui-model.md](../../forest-ui-model.md), [convex-evaluation.md](../../convex-evaluation.md),
  [boundary-transformations.md](../../boundary-transformations.md)
- Ghostty unified action dispatch (mirror of the upstream commit):
  https://git.uoc.run.place/Applied-Software/ghostty/commit/4ae20212bfe707ec3bedd9555321d51cfed1e3d4
- Convex React client, consistency across queries: https://docs.convex.dev/client/react
- TigerBeetle, "Writing High-Performance Clients for TigerBeetle":
  https://tigerbeetle.com/blog/2023-02-21-writing-high-performance-clients-for-tigerbeetle/
- Turso sync engine (secondary write-up, not verified against source):
  https://instagit.com/tursodatabase/turso/architecture-turso-sync-engine-turso-cloud ;
  crate: https://docs.rs/turso_sync_engine
