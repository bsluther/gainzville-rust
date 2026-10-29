# Source: prior-art research, 2026-10-29

Report from a research agent, kept as written. It read shallow clones of each project at the commits
named below; the clones themselves were temporary and are not kept, so local paths in the text are
stale. Use the GitHub links instead.

---

# Prior art: the "client SDK" boundary between a Rust core and a SwiftUI app

Research for Gainzville (GV). Source was read from shallow clones made on 2026-10-29 under
`/tmp/claude-0/-home-user-gainzville-rust/386402ff-aa6b-54f3-a879-1b8d8e24c4e0/scratchpad/ext/`. Nothing was built or run.
Each section gives the commit it read. Anything not checked against source is marked **(unverified)**.

GV today, for comparison (`gv-ffi/src/core.rs`): `subscribe_query(AnyQuery) -> Arc<FfiQuerySubscription>`, a payload-less
`CoreListener::on_data_changed()`, `read_query(AnyQuery) -> Option<AnyQueryResponse>`, `run_action(Action)`, plus about 10
synchronous `forest_*` reads. Some of those reads are domain helpers that Swift calls in order to build Actions:
`forest_position_between`, `forest_would_create_cycle`, `forest_suggested_root_day_insertion_time`.

---

## 1. Ranked shortlist

1. **matrix-rust-sdk (`matrix-sdk-ui` + `bindings/matrix-sdk-ffi`) with Element X iOS.** This is the closest large-scale
   match. One organisation (Element/Matrix.org) owns both the Rust SDK and the flagship SwiftUI app, the SDK owns a
   local store, and the UI renders list-shaped domain data that Rust builds. Rust *pushes data* to Swift: it sends a
   `Reset` with the initial items, guaranteed to arrive before any diffs, followed by `VectorDiff` batches of typed items.
   Subscriptions are drop-to-cancel `TaskHandle`s. Writes are intent-level (`toggle_reaction`, `edit`, `send`). Rust also
   computes per-item capability flags (`is_editable`, `can_be_replied_to`) and presentation-only items (date dividers,
   read marker), so the Swift side doesn't hold those domain rules. It also contains a counter-example of its own: the
   room list delivers `Room` *handles*, and Swift then makes N async calls per row.
2. **wordpress-rs (`wp_mobile_cache`, `wp_mobile`) with WordPress iOS.** This is closest to GV's *current* design. A
   Rust-owned SQLite cache emits SQLite `update_hook` events across FFI. Swift filters them with
   `collection.isRelevantUpdate(hook:)`, debounces for 50 ms, and re-pulls `loadItems()`. Again one company (Automattic)
   owns both sides. It shows where ping-and-pull ends up after a few years: every view model hand-rolls the same
   publisher, filter, debounce and re-read pipeline, and it makes two separate reads (`listInfo()` + `loadItems()`) that
   can disagree with each other. Its per-item `Fresh/Stale/Fetching/Failed` state enum is a useful pattern for GV's
   offline-first plans.
3. **Point-Free SQLiteData (`@FetchAll`/`@FetchOne`/`@Fetch`) on GRDB `ValueObservation`.** No Rust here, but this is the
   best model for the Swift *query hook* GV wants: a property wrapper that works in SwiftUI views and in `@Observable`
   models, delivers its first value synchronously, re-queries on commit, and replaces its query at runtime with
   `$items.load(newQuery)`. `FetchKeyRequest` bundles several reads into **one transaction** so they stay consistent, and
   that is exactly the guarantee GV's multiple `forest_*` reads lack today.
4. **Crux (redbadger/crux).** A "headless core": Rust owns `Model → ViewModel` and returns effects. The shell is a
   generated `@Observable` Swift `Core` whose `view` is replaced on every `Render`. Writes are intent `Event`s. Because
   `update()` is pure, it can be tested deterministically, which fits GV's simulation testing. The big mismatch is that
   in Crux the *shell* performs I/O, storage included (the weather example keeps KV in Core Data). The whole view model
   also crosses the FFI as bincode bytes on every render.
5. **Delta Chat (`chatmail/core` + `deltachat-ios`).** A Rust core owns SQLite, the domain logic and sync (IMAP), and a
   native Swift app sits on top; this has run for about 6 years. The pattern is ping-and-pull: a blocking event mailbox
   (`dc_get_next_event`) delivers `DC_EVENT_MSGS_CHANGED(chat_id?, msg_id?)` with *scope hints*, and Swift re-fetches an
   ID list and then loads rows lazily by ID. It proves ping-and-pull can last, but the UI is UIKit, and the ID-list-then-
   row reads are not consistent with each other.

Others checked, not shortlisted: PowerSync Swift (Rust sync core exposed as SQLite functions, a sans-IO
`powersync_control` loop, and watch via `EXPLAIN` table extraction), Mozilla application-services with Firefox iOS
(synchronous pull, no reactivity, whole tree returned in one call), Bitwarden `sdk-internal` (ownership inverted: the app
implements storage repositories as foreign traits), Ferrostar (pure state-transition core; Swift holds the state), and
automerge-swift (`objectWillChange` ping plus optional `[Patch]`). Realm Swift is a native C++ core rather than Rust;
its `RealmCollectionChange` and live-vs-frozen objects are covered in section 3.

---

## 2. Shortlisted sources in detail

### 2.1 matrix-rust-sdk + Element X iOS

Commits: matrix-rust-sdk `8ab40bd` (UniFFI **0.32.1**, proc macros wrapped in `matrix_sdk_ffi_macros::export`);
element-x-ios `f6a75aa`.

**Push-with-data, initial value before the stream.**
[`bindings/matrix-sdk-ffi/src/timeline/mod.rs` L338–358](https://github.com/matrix-org/matrix-rust-sdk/blob/8ab40bd30c923813caa5cd0fe6698230e9a8bd2f/bindings/matrix-sdk-ffi/src/timeline/mod.rs#L338-L358)
```rust
pub async fn add_listener(&self, listener: Box<dyn TimelineListener>) -> Arc<TaskHandle> {
    let (timeline_items, timeline_stream) = self.inner.subscribe().await;
    // It's important that the initial items are passed _before_ we forward
    // the stream updates, with a guaranteed ordering. ... See #3535 for details.
    // Note we pass initial items as a reset update, as a way to give the
    // callers a unified way to handle the initial batch ...
    listener.on_update(vec![TimelineDiff::new(VectorDiff::Reset { values: timeline_items })]);
    Arc::new(TaskHandle::new(get_runtime_handle().spawn(async move {
        let mut timeline_stream = pin!(timeline_stream);
        while let Some(diffs) = timeline_stream.next().await {
            listener.on_update(diffs.into_iter().map(TimelineDiff::new).collect());
        }
    })))
}
```
[L1006–1028](https://github.com/matrix-org/matrix-rust-sdk/blob/8ab40bd30c923813caa5cd0fe6698230e9a8bd2f/bindings/matrix-sdk-ffi/src/timeline/mod.rs#L1006-L1028)
```rust
#[matrix_sdk_ffi_macros::export(callback_interface)]
pub trait TimelineListener: SyncOutsideWasm + SendOutsideWasm {
    fn on_update(&self, diff: Vec<TimelineDiff>);
}
#[derive(Clone, uniffi::Enum)]
pub enum TimelineDiff {
    Append { values: Vec<Arc<TimelineItem>> }, Clear, PushFront { value: Arc<TimelineItem> },
    PushBack { value: Arc<TimelineItem> }, PopFront, PopBack,
    Insert { index: u32, value: Arc<TimelineItem> }, Set { index: u32, value: Arc<TimelineItem> },
    Remove { index: u32 }, Truncate { length: u32 }, Reset { values: Vec<Arc<TimelineItem>> },
}
```
`TimelineItem` is an `Arc` around an immutable Rust item (`#[repr(transparent)]`, zero copy). Swift calls `as_event()`
on it to get a flat `uniffi::Record`, and Rust computes the domain rules into that record
([L1220–](https://github.com/matrix-org/matrix-rust-sdk/blob/8ab40bd30c923813caa5cd0fe6698230e9a8bd2f/bindings/matrix-sdk-ffi/src/timeline/mod.rs#L1220)):
```rust
#[derive(Clone, uniffi::Record)]
pub struct EventTimelineItem {
    is_remote: bool, event_or_transaction_id: EventOrTransactionId, sender: String,
    is_own: bool, is_editable: bool, content: TimelineItemContent, reactions: Vec<Reaction>,
    local_send_state: Option<EventSendState>, can_be_replied_to: bool, /* ... */
}
```
Rust also emits presentation-only items: `VirtualTimelineItem::{DateDivider, ReadMarker, TimelineStart}`.

**Subscription lifetime = handle lifetime.**
[`task_handle.rs`](https://github.com/matrix-org/matrix-rust-sdk/blob/8ab40bd30c923813caa5cd0fe6698230e9a8bd2f/bindings/matrix-sdk-ffi/src/task_handle.rs)
```rust
#[derive(uniffi::Object)]
pub struct TaskHandle { handle: JoinHandle<()> }
impl Drop for TaskHandle { fn drop(&mut self) { self.cancel(); } }
```

**Reified query and controller.** This is GV's `AnyQuery` idea plus a mutable handle.
[`room_list.rs` L473–520](https://github.com/matrix-org/matrix-rust-sdk/blob/8ab40bd30c923813caa5cd0fe6698230e9a8bd2f/bindings/matrix-sdk-ffi/src/room_list.rs#L473-L520)
```rust
impl RoomListDynamicEntriesController {
    fn set_filter(&self, kind: RoomListEntriesDynamicFilterKind) -> bool { ... }
    fn add_one_page(&self) { ... }
    fn reset_to_one_page(&self) { ... }
}
#[derive(uniffi::Enum)]
pub enum RoomListEntriesDynamicFilterKind {
    All { filters: Vec<RoomListEntriesDynamicFilterKind> }, Any { ... }, NonLeft, Joined, Favourite,
    // Not { filter: ... } - requires recursive enum support in uniffi https://github.com/mozilla/uniffi-rs/issues/396
    NormalizedMatchRoomName { pattern: String }, /* ... */
}
```

**Intent-level writes.** Swift never builds raw events:
```rust
pub async fn send(self: Arc<Self>, msg: Arc<RoomMessageEventContentWithoutRelation>) -> Result<Arc<SendHandle>, ClientError>
pub async fn toggle_reaction(&self, item_id: EventOrTransactionId, key: String) -> Result<bool, ClientError>
```
Local echo is handled in Rust: the send queue inserts the item, and the diff stream reports it with `local_send_state`.

**Swift side.** There is one generic listener adapter, plus one diff applier per list.
[`ElementX/Sources/Other/SDKListener.swift`](https://github.com/element-hq/element-x-ios/blob/f6a75aabc6a06e2f3dfd4ad1b9666622e16e70af/ElementX/Sources/Other/SDKListener.swift)
```swift
final nonisolated class SDKListener<T>: Sendable {
    private let onUpdateClosure: @Sendable (T) -> Void
    init(_ onUpdateClosure: @escaping @Sendable (T) -> Void) { ... }
}
nonisolated extension SDKListener: RoomListEntriesListener where T == [RoomListEntriesUpdate] {
    func onUpdate(roomEntriesUpdate: [RoomListEntriesUpdate]) { onUpdateClosure(roomEntriesUpdate) }
}
// ... one 3-line conformance per SDK listener protocol
```
[`Services/Timeline/TimelineItemProvider.swift`](https://github.com/element-hq/element-x-ios/blob/f6a75aabc6a06e2f3dfd4ad1b9666622e16e70af/ElementX/Sources/Services/Timeline/TimelineItemProvider.swift)
```swift
// `for await` guarantees FIFO ordering and that only one diff is being applied at a time.
Task { [weak self] in for await diffs in diffStream { await self?.applyDiffs(diffs) } }
Task { roomTimelineObservationToken = await timeline.addListener(listener: SDKListener { diffs in
        diffContinuation.yield(diffs) }) }
...
case .set(let index, let item):
    let itemProxy = TimelineItemProxy(item: item)
    changes.append(.remove(offset: Int(index), element: itemProxy, associatedWith: nil))
    changes.append(.insert(offset: Int(index), element: itemProxy, associatedWith: nil))
```
Results are published through Combine `CurrentValueSubject<[TimelineItemProxy]>`, and view models are `ObservableObject`s.

**Counter-example from the same codebase.** The room list diffs carry `Room` *handles*, so Swift issues per-row async pulls
([`RoomSummaryProvider.swift` L296–316](https://github.com/element-hq/element-x-ios/blob/f6a75aabc6a06e2f3dfd4ad1b9666622e16e70af/ElementX/Sources/Services/Room/RoomSummary/RoomSummaryProvider.swift#L296-L316)):
```swift
let latestEvent = await room.latestEvent()
let roomInfo = try await room.roomInfo()
...
// The room info can be momentarily unavailable while the client tears down (e.g. on logout),
// where the SDK still delivers a diff but `room.roomInfo()` throws. Return a placeholder to
// keep the diff indices aligned with the SDK instead of crashing.
```

**Lessons for GV**
- Make the subscription carry data: typed records in a `Reset` first, then diffs, and deliver the initial value
  *before* the stream starts so ordering is guaranteed. This removes the separate re-read call and the race between the
  ping and the read.
- Put domain rules on the records as flags (`is_editable`, `can_be_replied_to`). For GV that would be things like
  `can_move_to(parent)` and `would_cycle`, or better, intent methods (`move_entry(id, after:)`) where Rust computes the
  `Position`. Then `forest_position_between` and `forest_would_create_cycle` stop being public API.
- Rust-built presentation items (date dividers) are the precedent for Rust emitting GV's day-grouped forest rows.
- A generic `SDKListener<T>` adapter plus a single `AsyncStream` per subscription is a small, reusable bridge.
- Data arriving as handles that each need async follow-up calls breaks diff consistency. Ship records.

**Doesn't transfer**
- Everything is `async` on a global tokio runtime (`get_runtime_handle().spawn`), and listeners are called from
  arbitrary threads. That is harder to simulate deterministically. GV can deliver from its own single write loop.
- Positional `VectorDiff`s suit one flat list. A tree needs keyed diffs or a whole-snapshot replace.
- Element X's Swift layering is heavy: TimelineProxy, ItemProvider, Controller and ItemFactory come to about 3.1k lines
  for the timeline alone. GV has one customer and can make the Rust side presentation-ready instead.
- UniFFI can't express recursive enums (#396). Note this before designing a recursive `AnyQuery` filter.

### 2.2 wordpress-rs + WordPress iOS

Commits: wordpress-rs `d6e4d2d`; WordPress-iOS `d62c740`.

**Rust: the SQLite update hook goes straight across the FFI.**
[`wp_mobile_cache/src/lib.rs` L333–345, L575–590](https://github.com/Automattic/wordpress-rs/blob/d6e4d2d7f4cd0eabb5d921ed1d61920faa2d7f3c/wp_mobile_cache/src/lib.rs#L333-L345)
```rust
pub fn start_listening_for_updates(&self, delegate: std::sync::Arc<dyn DatabaseDelegate>) {
    self.execute(|connection| {
        if let Err(e) = connection.update_hook(Some(
            move |action: Action, db_name: &str, table_name: &str, row_id: i64| { /* -> UpdateHook */ }
pub trait DatabaseDelegate: Send + Sync { fn did_update(&self, update_hook: UpdateHook); }
pub struct UpdateHook { pub action: HookAction, pub db_name: String, pub table: DbTable, /* row_id */ }
```
Collections are `uniffi::Object`s that expose `load_items()`, `refresh()`, `load_next_page()` and a relevance predicate
([`wp_mobile/src/collection/post_metadata_collection.rs` L293–318](https://github.com/Automattic/wordpress-rs/blob/d6e4d2d7f4cd0eabb5d921ed1d61920faa2d7f3c/wp_mobile/src/collection/post_metadata_collection.rs#L293-L318)):
```rust
pub fn is_relevant_update(&self, hook: &UpdateHook) -> bool { self.core.is_relevant_update(hook) }
pub fn is_relevant_data_update(&self, hook: &UpdateHook) -> bool { ... }
pub fn is_relevant_list_info_update(&self, hook: &UpdateHook) -> bool { ... }
```
Per-item sync state is a typed enum
([`collection/mod.rs`](https://github.com/Automattic/wordpress-rs/blob/d6e4d2d7f4cd0eabb5d921ed1d61920faa2d7f3c/wp_mobile/src/collection/mod.rs)):
```rust
pub enum $state_name { Missing, Fetching, FetchingWithData { data }, Fresh { data }, Stale { data },
                       Failed { error: String }, FailedWithData { error: String, data } }
```

**Swift package.** Hooks are rebroadcast through NotificationCenter as a Combine publisher.
[`native/swift/Sources/wordpress-api-cache/WordPressApiCache.swift`](https://github.com/Automattic/wordpress-rs/blob/d6e4d2d7f4cd0eabb5d921ed1d61920faa2d7f3c/native/swift/Sources/wordpress-api-cache/WordPressApiCache.swift)
```swift
public func databaseUpdatesPublisher() -> AnyPublisher<UpdateHook, Never> {
    NotificationCenter.default.publisher(for: UpdateHookBroadcaster.notification, object: broadcaster)
        .prefix(untilOutputFrom: stopUpdatesNotifier)
        .compactMap { UpdateHookNotification(from: $0)?.hook }.eraseToAnyPublisher()
}
```
**App view model**
([`WordPress/Classes/ViewRelated/CustomPostTypes/CustomPostListViewModel.swift` L231–262](https://github.com/wordpress-mobile/WordPress-iOS/blob/d62c740a8641922ae694b133461156d85d750a84/WordPress/Classes/ViewRelated/CustomPostTypes/CustomPostListViewModel.swift#L231-L262)):
```swift
func handleDataChanges() async {
    let batches = await client.cache.databaseUpdatesPublisher()
        .filter { [weak collection] in collection?.isRelevantUpdate(hook: $0) == true }
        .collect(.byTime(DispatchQueue.main, .milliseconds(50)))
        .values
    for await batch in batches {
        guard !isBatchSyncing else { continue }
        let listInfo = collection.listInfo()                 // read #1
        let metadataItems = try await collection.loadItems() // read #2 (separate, can disagree)
        withAnimation { ...; updateItems(from: metadataItems) }
    }
}
```

**Lessons for GV**
- This is what GV's current design looks like once it has grown up. The same pipeline (subscribe, filter, debounce,
  re-read) gets copied into every view model. GV should put relevance checking, coalescing and re-querying in Rust
  (GV already re-runs subscribed queries there) and send the *result* to Swift.
- Two reads after one ping can disagree. Return one record (`{list_info, items}`), like a SQLiteData `FetchKeyRequest`.
- Firing an FFI callback for every SQLite row change is chatty. Coalesce per transaction or per Action in Rust.
- The `Fresh/Stale/Fetching/FailedWithData` per-item state is a good shape for GV's future offline-first and sync status.

**Doesn't transfer**
- It is a server-backed cache (REST is the source of truth). GV's local DB *is* the source of truth.
- It relies on NotificationCenter and Combine. GV targets `@Observable`/AsyncSequence.

### 2.3 SQLiteData + GRDB ValueObservation (the model for a Swift query hook)

Commits: sqlite-data `37c461a`; GRDB.swift `0d8cf95`.

**The protocol behind `@Fetch`: one value fetched in one transaction.**
[`Sources/SQLiteData/FetchKeyRequest.swift`](https://github.com/pointfreeco/sqlite-data/blob/37c461a814707e4b33a7d22921cafbcd43a8b125/Sources/SQLiteData/FetchKeyRequest.swift)
```swift
public protocol FetchKeyRequest<Value>: Hashable, Sendable {
  associatedtype Value
  func fetch(_ db: Database) throws -> Value
}
```
[`Documentation.docc/Articles/Fetching.md` L284–315](https://github.com/pointfreeco/sqlite-data/blob/37c461a814707e4b33a7d22921cafbcd43a8b125/Sources/SQLiteData/Documentation.docc/Articles/Fetching.md#L284-L315)
```swift
struct Reminders: FetchKeyRequest {
  struct Value { var completedReminders: [Reminder] = []; var remindersCount = 0 }
  func fetch(_ db: Database) throws -> Value {
    try Value(completedReminders: Reminder.where(\.isCompleted).fetchAll(db),
              remindersCount: Reminder.fetchCount(db))
  }
}
@Fetch(Reminders()) var reminders = Reminders.Value()
```
**It works in views and in `@Observable` models** (`Observing.md`):
```swift
struct ItemsView: View { @FetchAll var items: [Item]; ... }
@Observable class ItemsModel { @ObservationIgnored @FetchAll var items: [Item] }
```
**The query can be swapped at runtime** (`DynamicQueries.md`): `try await $items.load(Items.where { ... }.order { ... })`.

**Internals: re-fetch on commit with an immediate first value.**
[`Internal/FetchKey.swift`](https://github.com/pointfreeco/sqlite-data/blob/37c461a814707e4b33a7d22921cafbcd43a8b125/Sources/SQLiteData/Internal/FetchKey.swift)
```swift
let observation = ValueObservation.tracking { db in dependencies.yield { Result { try request.fetch(db) } } }
let scheduler: any ValueObservationScheduler = scheduler ?? ImmediateScheduler()
let cancellable = observation.publisher(in: database, scheduling: scheduler) ...
```
GRDB states the guarantees explicitly
([`ValueObservation.md` L83–117](https://github.com/groue/GRDB.swift/blob/0d8cf958b4b66a0473ec6e6986eb9da462171da9/GRDB/Documentation.docc/Extension/ValueObservation.md#L83-L117)):
> "`ValueObservation` may coalesce subsequent changes into a single notification." ... "the `.immediate` scheduler
> notifies all values on the main actor, and notifies the first one immediately when the observation starts ... You
> don't have to implement any empty or loading screen".

GRDB also separates the *tracked region* from the *fetched value* (`tracking(region:fetch:)`), and offers
`DatabaseRegionObservation` for cases that need every transaction.

**Lessons for GV**
- Model the Swift API on this: `@GVQuery(.entriesForDay(d)) var rows` and `@GVQuery(ForestScreen(day:)) var forest`,
  usable in views and in `@Observable` models, with `$rows.load(newQuery)` to change the query.
- Get the first value synchronously when the wrapper is set up. GV's `read_query` is already synchronous, so
  `subscribe` can return `(initial, handle)` the same way matrix does.
- Define one request type per screen that returns one value from one read transaction. That is the fix for multiple
  `forest_*` calls that each see a different DB state.
- Coalescing is acceptable for UI. Keep a separate per-transaction channel only for consumers that need every change.

**Doesn't transfer**
- Here Swift owns SQL and the schema. In GV the query is a Rust `AnyQuery` and the fetch closure runs in Rust.
- GRDB automatically tracks which tables a query reads. GV needs its own per-query dependency tracking in Rust (it
  re-runs every subscribed query today; `ivm` could narrow that later).

### 2.4 Crux (headless core)

Commit: `fbb8849`. Examples now export through `#[boltffi::export]`. `examples/notes/shared/uniffi.toml` also exists;
I did not check whether UniFFI is still a supported path **(unverified)**.

**Core contract**
([`crux_core/src/lib.rs` L259–288](https://github.com/redbadger/crux/blob/fbb8849b5f794c171fc1cf25c9779327cfabd943/crux_core/src/lib.rs#L259-L288)):
```rust
pub trait App {
    type Event: Unpin + Send + 'static; type Model; type ViewModel; type Effect: Effect + Unpin;
    fn update(&self, event: Self::Event, model: &mut Self::Model) -> Command<Self::Effect, Self::Event>;
    fn view(&self, model: &Self::Model) -> Self::ViewModel;
}
```
**FFI: three byte-level functions**
([`examples/counter/shared/src/ffi.rs`](https://github.com/redbadger/crux/blob/fbb8849b5f794c171fc1cf25c9779327cfabd943/examples/counter/shared/src/ffi.rs)):
```rust
#[boltffi::export]
impl CoreFfi {
    pub fn update(&self, data: &[u8]) -> Vec<u8> { ... }          // Event in, effect requests out
    pub fn resolve(&self, id: u32, data: &[u8]) -> Vec<u8> { ... } // effect output in, more effects out
    pub fn view(&self) -> Vec<u8> { ... }                         // whole ViewModel, bincode
}
```
**The generated Swift shell is `@Observable`**
([`type_generation/facet/plugins/core/swift.rs` L171–210](https://github.com/redbadger/crux/blob/fbb8849b5f794c171fc1cf25c9779327cfabd943/crux_core/src/type_generation/facet/plugins/core/swift.rs#L171-L210)):
```swift
@Observable @MainActor
public final class Core {
    public private(set) var view: ViewModel
    @ObservationIgnored private let bridge: any CoreBridge
    ...
}
```
**App** ([`examples/weather/apple/WeatherApp/WeatherApp.swift`](https://github.com/redbadger/crux/blob/fbb8849b5f794c171fc1cf25c9779327cfabd943/examples/weather/apple/WeatherApp/WeatherApp.swift)):
```swift
let core = Core(handler: WeatherHandler())   // handler implements http/kv/location/time effects in Swift
ContentView().environment(core).environment(updater)
switch core.view { case .loading: ...; case let .active(active): ActiveView(model: active) ... }
```
**Deterministic tests**
([`examples/counter-http/shared/src/app.rs` L159–181](https://github.com/redbadger/crux/blob/fbb8849b5f794c171fc1cf25c9779327cfabd943/examples/counter-http/shared/src/app.rs#L159-L181)):
```rust
let mut cmd = app.update(Event::Get, &mut model);
let actual = cmd.resolve_http(|op| { assert_eq!(op, &HttpRequest::get("https://crux-counter.fly.dev/").build());
        HttpResult::Ok(HttpResponse::ok().body(r#"{ "value": 1, ... }"#).build()) }).expect_event();
```

**Lessons for GV**
- Treat writes as intent `Event`s that Swift doesn't interpret ("move entry X after Y", "log set"). The core decides the
  records. This is the cure for domain rules leaking into Swift.
- A view model built in Rust per screen, an enum such as `Loading | Active(...)`, removes view-model plumbing from
  Swift. A generated or tiny hand-written `@Observable` wrapper is all Swift needs.
- Sans-IO effects give GV's simulation testing a precedent: network and clock become effects resolved by the test.

**Doesn't transfer**
- The shell owns storage and I/O. GV's core owns SQLite, and its queries are reactive over that DB, not over an
  in-memory `Model`.
- One global `ViewModel` is re-serialised in full on every render. That is fine for small screens but costly for a
  large forest. GV wants per-query subscriptions.
- Bincode bytes plus separate typegen is a second codegen pipeline. GV already has typed UniFFI records.

### 2.5 Delta Chat (Rust core owning SQLite, native Swift)

Commits: chatmail/core `1a4a988`; deltachat-ios `b351d9e`.

**A ping that carries a scope hint**
([`deltachat-ffi/deltachat.h` L6000–6010](https://github.com/chatmail/core/blob/1a4a988ca483e566aaafd9f9bb92239a709d03fd/deltachat-ffi/deltachat.h#L6000-L6010)):
```c
/** Messages or chats changed. ...
 * @param data1 (int) chat_id if only a single chat is affected by the changes, otherwise 0.
 * @param data2 (int) msg_id if only a single message is affected by the changes, otherwise 0. */
#define DC_EVENT_MSGS_CHANGED 2000
```
Events come from a blocking mailbox, `while ((event = dc_get_next_event(emitter)) != NULL)`, which is the same shape as
Ghostty's wakeup+tick.

**Swift** ([`DcCore/DcCore/DC/events.swift` L89–97](https://github.com/deltachat/deltachat-ios/blob/b351d9e6739bd59caaf02de688aa0e5241b3cf45/DcCore/DcCore/DC/events.swift#L89-L97),
[`deltachat-ios/Chat/ChatViewController.swift` L775–799, L1253–1258](https://github.com/deltachat/deltachat-ios/blob/b351d9e6739bd59caaf02de688aa0e5241b3cf45/deltachat-ios/Chat/ChatViewController.swift#L775-L799)):
```swift
case DC_EVENT_MSGS_CHANGED:
    NotificationCenter.default.post(name: Event.messagesChanged, object: nil,
        userInfo: ["message_id": Int(data2), "chat_id": Int(data1)])
...
if chatId == 0 || chatId == self.chatId { self.refreshMessages(); self.updateTitle(); ... }
private func refreshMessages() {
    messages = dcContext.getChatMsgsAndTimestamps(chatId: chatId, flags: DC_GCM_ADDDAYMARKER).reversed()
    reloadData()   // cells then call dcContext.getMessage(id:) lazily, one FFI read per row
}
```
Note the `DC_GCM_ADDDAYMARKER` flag: the Rust core inserts day markers into the ID list, the same idea as matrix's
`DateDivider`.

**Lessons for GV**
- Payload-less pings can be made cheap by adding *scope hints* (`chat_id`/`msg_id`, or 0 for "anything"). If GV keeps
  pinging for any path, send the affected query or subscription IDs.
- Day grouping belongs in the core.

**Doesn't transfer**
- The UI is UIKit with imperative refreshes, a C ABI with manual `*_unref`, and NotificationCenter fan-out.
- Fetching an ID list and then loading each row means rows can vanish between the two reads; Delta Chat tolerates it.

---

## 3. Cross-cutting patterns

### Patterns worth copying
1. **Subscribe returns the initial value, then pushes data.** Seen in matrix (`Reset` before the stream), GRDB
   `.immediate`, Realm `RealmCollectionChange.initial(...)` then `.update(_, deletions:, insertions:, modifications:)`
   ([`RealmSwift/RealmCollection.swift` L135–154](https://github.com/realm/realm-swift/blob/a40ced79265b644a896e393b0f3a37e7916e7de3/RealmSwift/RealmCollection.swift#L135-L154)),
   and PowerSync `watch` (`merge([()].async, updateNotifications)`). For GV, the subscription would carry an
   `AnyQueryResponse` or a snapshot, not a bare ping.
2. **One read means one consistent value.** SQLiteData `FetchKeyRequest`, Mozilla `bookmarks_get_tree(guid) -> BookmarkItem`
   (a whole recursive tree in one call; see the
   [places.udl L133](https://github.com/mozilla/application-services/blob/3d2eb45d7fbb92eeccb1a6284ef8687d4bdb1ece/components/places/src/places.udl#L133)),
   and matrix's immutable `Arc<TimelineItem>`. For GV this could be a **versioned, immutable `ForestSnapshot` object**:
   a uniffi Object holding the result of one read, with synchronous traversal methods (`roots()`, `children(id)`,
   `ancestors(id)`) that all read the same snapshot. Each change pushes a new snapshot. The ~10 synchronous traversal
   methods stay, and they become consistent.
3. **Lifetime is tied to a handle.** matrix `TaskHandle` cancels in `Drop`, Element X's `StreamContinuationWrapper`
   calls `finish()` in `deinit`, and PowerSync uses `continuation.onTermination = { task.cancel() }`. GV already has
   drop-to-unsubscribe; add the AsyncStream `onTermination` link on the Swift side.
4. **One generic Swift adapter instead of one per view model.** Element X's `SDKListener<T>`, SQLiteData's single
   `FetchKey`, and Crux's generated `Core`. GV needs one `@Observable`/property-wrapper type parameterised by query.
5. **Domain rules computed in Rust.** These arrive as record flags (`is_editable`, `can_be_replied_to`), as intent
   methods (`toggle_reaction`, `edit`, Crux `Event`s), or as presentation items computed in the core (`DateDivider`,
   `DC_GCM_ADDDAYMARKER`).
6. **A reified query plus a controller handle.** matrix `set_filter(kind)` / `add_one_page()` mutate a live
   subscription instead of creating a new one. This maps onto GV's `AnyQuery`, and onto SQLiteData `$items.load(q)` on
   the Swift side.
7. **Sans-IO cores for simulation testing.** Crux `Command` resolution in tests, PowerSync's sync state machine driven
   through `SELECT powersync_control(op, payload)` and returning `[Instruction]`
   ([`PowerSyncControlArguments.swift` L49](https://github.com/powersync-ja/powersync-swift/blob/6c674d542e94aa0770bbc6494c03acd6b77ba67e/Sources/PowerSync/Implementation/sync/PowerSyncControlArguments.swift#L49),
   [`Instruction.swift`](https://github.com/powersync-ja/powersync-swift/blob/6c674d542e94aa0770bbc6494c03acd6b77ba67e/Sources/PowerSync/Implementation/sync/Instruction.swift)),
   and Ferrostar's pure `update_user_location(location, state) -> NavState` with an observer decorator for recording
   and replay
   ([`navigation_controller/mod.rs` L41–75](https://github.com/stadiamaps/ferrostar/blob/439220fa784b0e9bf88b5ef9dc62af715a2f6b01/common/ferrostar/src/navigation_controller/mod.rs#L41-L75)).
   These are the precedent for designing GV's future sync engine so that Swift (or a test) feeds it bytes.
8. **Per-item sync state** (wordpress-rs `Fresh/Stale/FetchingWithData/FailedWithData`, matrix `local_send_state`) as a
   typed field instead of a side channel.

### Anti-patterns observed
- **Ping, then several pulls** (wordpress-rs `listInfo()` + `loadItems()`, Delta Chat IDs then rows, GV's
  `forest_*` today): the reads are not atomic, and every view model gets the same plumbing.
- **Handles that need async follow-up reads inside a diff** (Element X `RoomSummaryProvider`): diff indices race with
  teardown, so the app needs placeholder hacks.
- **Per-row change events across the FFI** (wordpress-rs `update_hook` delegate) with debouncing in Swift (`.collect(.byTime(50ms))`).
- **Stacked Swift wrapper layers.** Firefox iOS has Rust `PlacesConnection`, then Swift `PlacesReadConnection`
  (`queue.sync`), then `RustPlaces` (`Deferred<Maybe<T>>` on reader/writer queues)
  ([`RustPlaces.swift` L142–147](https://github.com/mozilla-mobile/firefox-ios/blob/3316313c0f2401bff42b26fea7014cb5dfbbd0f4/firefox-ios/Storage/Rust/RustPlaces.swift#L142-L147)).
  Element X has about 3k lines of Swift between the SDK and the timeline view. One customer means one thin layer.
- **Live, thread-confined objects** (Realm's original model, which later needed "frozen" objects). Prefer immutable
  values or snapshots crossing the boundary.
- **Inverted ownership** (Bitwarden: the app implements `get/list/set/remove` repositories that the SDK calls,
  [`bitwarden-uniffi/src/platform/repository.rs`](https://github.com/bitwarden/sdk-internal/blob/d7ab9964cd0b10db346d0418dba065969663bdcb/crates/bitwarden-uniffi/src/platform/repository.rs)).
  This is right for a crypto SDK whose host already owns a database. It is wrong for GV, where the core owns the data.

### Notes on other sources
- **PowerSync Swift** (`6c674d5`): the Rust code is a SQLite loadable extension (`powersync-sqlite-core`), so the
  "FFI" is SQL. `watch()` runs `EXPLAIN <sql>` to find source tables, filters `pool.tableUpdates`, re-queries, and
  throttles
  ([`queries/watch.swift`](https://github.com/powersync-ja/powersync-swift/blob/6c674d542e94aa0770bbc6494c03acd6b77ba67e/Sources/PowerSync/Implementation/queries/watch.swift)).
  It ships a `PowerSyncGRDB` target so SQLiteData-style wrappers can sit on top.
- **automerge-swift** (`658cb91`): every mutation calls `sendObjectWillChange()` under an `NSRecursiveLock`, and merges
  return `[Patch]` for anyone who wants deltas. Ping plus optional diff.
- **Ditto, Couchbase Lite Swift**: native cores with live queries; their source was not examined **(unverified)**.

---

## 4. Sources

- matrix-rust-sdk @ `8ab40bd`: https://github.com/matrix-org/matrix-rust-sdk ("bindings/matrix-sdk-ffi/src/{timeline/mod.rs,room_list.rs,task_handle.rs}", "crates/matrix-sdk-ui/src/timeline/mod.rs")
- element-x-ios @ `f6a75aa`: https://github.com/element-hq/element-x-ios ("ElementX/Sources/Other/SDKListener.swift", "Services/Timeline/TimelineItemProvider.swift", "Services/Room/RoomSummary/RoomSummaryProvider.swift")
- wordpress-rs @ `d6e4d2d`: https://github.com/Automattic/wordpress-rs ("wp_mobile_cache/src/lib.rs", "wp_mobile/src/collection/", "native/swift/Sources/wordpress-api-cache/")
- WordPress-iOS @ `d62c740`: https://github.com/wordpress-mobile/WordPress-iOS ("WordPress/Classes/ViewRelated/CustomPostTypes/CustomPostListViewModel.swift")
- sqlite-data @ `37c461a`: https://github.com/pointfreeco/sqlite-data ("Sources/SQLiteData/{FetchKeyRequest.swift,Internal/FetchKey.swift,Documentation.docc/Articles/}")
- GRDB.swift @ `0d8cf95`: https://github.com/groue/GRDB.swift ("GRDB/Documentation.docc/Extension/ValueObservation.md")
- crux @ `fbb8849`: https://github.com/redbadger/crux ("crux_core/src/lib.rs", "crux_core/src/type_generation/facet/plugins/core/swift.rs", "examples/{counter,counter-http,weather}")
- Delta Chat core @ `1a4a988`: https://github.com/chatmail/core ("deltachat-ffi/deltachat.h"); deltachat-ios @ `b351d9e`: https://github.com/deltachat/deltachat-ios
- powersync-swift @ `6c674d5`: https://github.com/powersync-ja/powersync-swift
- application-services @ `3d2eb45`: https://github.com/mozilla/application-services ("components/places/src/places.udl"); firefox-ios @ `3316313`: https://github.com/mozilla-mobile/firefox-ios ("MozillaRustComponents/.../Places/Places.swift", "firefox-ios/Storage/Rust/RustPlaces.swift")
- bitwarden sdk-internal @ `d7ab996`: https://github.com/bitwarden/sdk-internal ("crates/bitwarden-uniffi/src/platform/repository.rs", "crates/bitwarden-state/src/repository.rs")
- ferrostar @ `439220f`: https://github.com/stadiamaps/ferrostar ("common/ferrostar/src/navigation_controller/mod.rs", "apple/Sources/FerrostarCore/FerrostarCore.swift")
- automerge-swift @ `658cb91`: https://github.com/automerge/automerge-swift ("Sources/Automerge/Document.swift")
- realm-swift @ `a40ced7`: https://github.com/realm/realm-swift ("RealmSwift/RealmCollection.swift", "RealmSwift/SwiftUI.swift")
- UniFFI recursive enum limitation: https://github.com/mozilla/uniffi-rs/issues/396 (cited in matrix source; issue itself not re-read)
