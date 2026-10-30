# Source: discussion notes, 2026-10-28 to 2026-10-29

Notes from a follow-up conversation about this RFD, including a read of the Convex and Ghostty
source. These are notes rather than a transcript, so the reasoning is paraphrased.

## Reframes from the discussion

- **design.md isn't the reference.** The drift from design.md's in-memory store isn't automatically
  wrong. The real question is why so many systems expose a query seam, and whether that reason applies
  to GV. There are two reasons:
  - *Generality.* Databases, `useQuery` hooks and query-based sync engines don't know the app, so
    they can't do better. This mostly doesn't apply to GV, whose core knows its one frontend.
  - *A clean contract.* A query is declarative, has an obvious subscribe-and-refresh story, and
    keeps the seam narrow and stable. This does apply.
- **Forest traversal methods are queries too.** `parent(entry)` could run in SQL or in memory. The
  question is how to run many small queries consistently and cheaply, not whether they are queries.
- **Two kinds of simplicity.** The current FFI is simple to *implement* and pushes the cost onto the
  *consumer*. The inbox pattern was deliberate (avoids UniFFI async, batches bursts, latest-wins),
  but nothing on the Swift side packages it, so each view model re-implements it. Writes also throw
  away information: `run_action` returns the `Mutation` in Rust, the FFI returns `()`, and Swift
  swallows errors with `try?`.
- **The boundary is FFI plus a Swift SDK.** It doesn't have to be just the FFI line. A slim FFI plus
  a thin Swift package that the core side owns (Convex's `convex-swift`, Ghostty's Swift wrapper).
  Named **GvKit** for now. This should exist whatever else is decided.
- **Performance is an optimization, not the problem.** Forest copying (each `forest_*` call clones
  the whole `AllEntries` vector because the cache sits behind a mutex) is an implementation detail.
  It matters only in that the boundary must leave room to optimize later.

## Push vs ping, clarified

- **Ping (today).** One doorbell. Every reader walks into the shared room (the Rust cache) and picks
  up their item, one call at a time. Each read clones in Rust and copies across the FFI, even when
  nothing changed for that reader. Swift re-asks with the query value as a key and unwraps
  `AnyQueryResponse`.
- **Push.** Each subscription has a PO box (a Swift listener). Rust does the delivery: it decides when
  something changed, computes each subscription's result and puts it in the box. Swift only owns the
  box. GV's `QueryStore` already re-runs every subscription; it would deliver instead of storing.
- **Tearing.** Separate per-subscription callbacks each hop to the main thread, and SwiftUI can
  render between them. So deliver one batch per change across the FFI, tagged with a version, and
  apply it in one main-thread step. That gives a "room on the Swift side" that can't change
  mid-render, local reads, free routing, and only changed results crossing.
- **Snapshot handle vs pushed data.** Both freeze the room. A handle leaves it in Rust, and reads are
  method calls. Pushed data copies it into Swift, and reads are free. The handle suits the big,
  heavily traversed forest; pushed data suits small results.

## Convex (`get-convex/convex-mobile` + `get-convex/convex-swift`, read 2026-10-28)

- UniFFI 0.28 with a `.udl` file. One object, about 450 lines of Rust.
- The FFI is stringly typed: function name plus JSON-string args in, JSON strings out. It is async
  (Swift awaits Rust futures). Subscriptions push the full value through a per-subscription
  callback.

```
interface MobileConvexClient {
    [Async, Throws=ClientError] string query(string name, record<string, string> args);
    [Async, Throws=ClientError] SubscriptionHandle subscribe(string name, record<string, string> args, QuerySubscriber subscriber);
    [Async, Throws=ClientError] string mutation(string name, record<string, string> args);
};
[Trait, WithForeign] interface QuerySubscriber { void on_update(string value); void on_error(string message, string? value); };
```

- The Swift wrapper (~150 lines plus auth) turns each subscription into a typed Combine
  `AnyPublisher<T>` (JSON decoded via `Decodable`) and cancels the Rust subscription when downstream
  cancels. Usage is a plain `for await` loop over `.values`. There's no SwiftUI property wrapper.
- **Lessons:** library-managed lifetimes, so subscription lifetime equals listener lifetime with
  nothing wired by hand. A thin typed Swift layer over a generic FFI.
- **Not applicable:** stringly typing. GV's typed UniFFI records are better for GV, at the cost of
  regenerating bindings whenever the model changes.
- **Caution:** each subscription has its own Rust task and callback, so the Swift SDK has no
  cross-query consistency, unlike Convex's React client.
- **Low-level by design:** Convex can't pick a pattern for apps it hasn't seen. GV's SDK has one
  customer and can be opinionated, so it should ship the hook.

## Ghostty (`ghostty-org/ghostty`, read 2026-10-29)

- A hand-written C header with about 92 functions around opaque handles (app, surface, config).
  Swift sends commands on handles (`ghostty_surface_key`, `ghostty_surface_set_size`,
  `ghostty_surface_binding_action`).
- The core talks back through one runtime config struct: `wakeup_cb` plus `action_cb`. On wakeup,
  Swift schedules `ghostty_app_tick` on the main thread. The tick drains the core's mailbox by
  calling `action_cb(app, target, action)` with a tagged union (about 85 action kinds: new tab, set
  title, ...) aimed at the app or a specific surface. Swift handles it in one big switch and writes
  into `@Published` properties.
- The `Surface` Swift class frees its handle in `deinit`. It's marked "work-in-progress" by Mitchell
  as part of a data-model refactor, and the app still uses `ObservableObject`/`@Published`.
- **Lessons:** the inbox isn't the problem; the bare ping is. Ghostty's drained messages are typed,
  carry data and name a target. Commands are intents on handles.
- **Not applicable:** Swift barely reads core data, because the core renders the terminal itself. GV
  renders domain data, so it needs a read layer Ghostty has no equivalent of.

## Convex's Rust client (`get-convex/convex-rs` @ `d05acaa`, read 2026-10-29)

- `ConvexClient::watch_all()` returns a `QuerySetSubscription`, a `Stream<QueryResults>`. Its docs:
  "Each item in the stream contains a consistent view of the results of all the queries in the query
  set." `QueryResults` is a copy-on-write map (`OrdMap<QueryId, FunctionResult>` plus
  `OrdSet<SubscriberId>`). Queries join the set via `subscribe` and leave when their
  `QuerySubscription` token drops.
- The stream treats a lagged broadcast as fine:

  ```rust
  // Ok to be lagged (skip intermediate values) - since Convex
  // only guarantees a newer value than the previous value.
  task::Poll::Ready(Some(Err(BroadcastStreamRecvError::Lagged(_amt)))) => continue,
  ```

- `convex-mobile` doesn't use `watch_all`. It subscribes per query and forwards each stream to its own
  FFI callback, so the cross-query consistency the Rust client offers is lost on the Swift side.
- The base client also has `OptimisticQueryResults` for patching results while a mutation is in
  flight.

## Matrix date dividers and crate layering (`matrix-rust-sdk` @ `8ab40bd`, `element-x-ios` @ `f6a75aa`, read 2026-10-30)

- `crates/matrix-sdk-ui/src/timeline/virtual_item.rs` defines
  `VirtualTimelineItem::{DateDivider(MilliSecondsSinceUnixEpoch), ReadMarker, TimelineStart}`.
- `crates/matrix-sdk-ui/src/timeline/date_dividers.rs`: `DateDividerAdjuster::run` "ensures that date
  separators are properly inserted/removed when needs be" after each batch of item changes, then runs
  `check_invariants`. On violation it logs an error and panics. `DateDividerMode` is `Daily` or
  `Monthly`, and dates are computed in local time.
- The Element X Swift side
  (`ElementX/Sources/Services/Timeline/TimelineController/TimelineController.swift`) maps
  `.dateDivider(timestamp)` to a `SeparatorRoomTimelineItem` and formats the date itself.
- Crate layering: `matrix-sdk` (client: protocol, storage, sync), `matrix-sdk-ui`
  (platform-independent, screen-shaped logic: timeline, dividers, room list service),
  `bindings/matrix-sdk-ffi` (exposes both over UniFFI).
- From the discussion: the GV analog is drop slots in `ChildrenSection`. Drop slots are UI-ish rather
  than a core concern, so they belong in a UI-shaped Rust layer, not `gv-core`.
