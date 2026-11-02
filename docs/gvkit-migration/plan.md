# GvKit migration: inventory and plan

> **Done (2026-11-01).** Kept as the record of the migration. Where it ended up differing: S0's
> LiveStore fixes (see `live-review.md`); S4 uses plain hooks rather than an `@Observable` model,
> because all three questions come from init inputs. The optional client-set prune in S6 was done.

- Date: 2026-11-01
- Base: `spike/gvkit-query-hooks` at `f130866`. All
  `file:line` refs are against that commit. Swift paths are relative to `swift-app/Gainzville/`.
- Scope: move every Swift consumer of core data onto `@GvQuery` / the public box / `gv.read`, then
  delete the ping-and-pull path. Writes stay record-level `run_action` (intents are a later phase).
  Forest generalization, macros and the join map are out of scope.

## Summary

- **30 consumers**: 6 view models + 24 views (3 already on hooks: `DayRootsList`, `ChildrenSection`,
  `DropTarget`'s cycle read; `EntryView` is half-migrated).
- **2 new `ClientQuery` members**: `SetsMembers` (forest, gv-ui) and `ActivityTemplateJoin` (SQL,
  client-only). Everything else maps to members that already exist. A third need (append position for
  "+ entry") is answered by a new Swift key over the existing `ChildSlots`, with no Rust change.
- **Decision (a), read-your-writes:** don't call `sync()` on the write path. No current site depends on
  RYW, and today's path doesn't give it either. Keep a designed-but-unbuilt `settle()` as a fallback.
  Version-at-commit arrives with the sync work.
- **Decision (b), box:** one `@MainActor @Observable` class, `GvLiveQuery<Value>` (today's
  `GvQueryBox`, made public). Its init is cheap and it subscribes on first read. `@GvQuery` stores it in
  `@State` and binds it in `update()`; `$hook` projects the box. Models that hold boxes must be
  `@Observable`, not `ObservableObject`.
- **7 steps** (S0–S6). Only S0 and S6 touch the FFI, so S2, S3 and S4 run in parallel after S0/S1.

---

## 1. Inventory

Legend for the query column:
- **existing**: already a `ClientQuery` member (`client/queries.rs:34`).
- **NEW-forest** / **NEW-SQL**: a new member, with its backing named.
- **key-only**: a new Swift `GvKey` over an existing member.

### 1a. View models and infra (`Core.swift`, `GainzvilleApp.swift`, GvKit)

| Consumer | file:line | Reads → ClientQuery | Writes | Classification |
|---|---|---|---|---|
| `AppListener` | Core.swift:21 | `on_data_changed` ping (gv-ffi core.rs:47,106-114) | – | **Delete** (S6) |
| `CoreEnv` | Core.swift:33 | hands `core` to per-view VMs | – | **Delete** (S6) |
| `DataChange` | Core.swift:44 | Combine fan-out of the ping | – | **Delete** (S6) |
| `ActivitiesViewModel` | Core.swift:52 (subscribe :57-61, read :63-67) | `AllActivities` → **existing** | `createActivity` :69-98 (`CreateActivity`) | **Hook** (`.allActivities`); write → GvKit helper. Delete VM (S2). |
| `ForestViewModel.roots` | Core.swift:105,109-117 | `subscribe_forest` + `forest_roots`; only consumer is `templateRoot`'s dependency hack (:130) | – | **Delete** (S5) |
| `ForestViewModel.children(of:)` | Core.swift:123 | `forest_children` → **NEW-forest `SetsMembers`** (only caller is sets, EntryView.swift:58). No domain question exists (`FindDescendants` is transitive). | – | **Hook** (S5) |
| `ForestViewModel.templateRoot` | Core.swift:129-132 | `forest_activity_template_root` → `FindActivityTemplateRoot` **existing** (SQL) | – | **Hook** (S5) |
| `ForestViewModel.suggestedRootInsertionTime` | Core.swift:179-183 | `SuggestedRootInsertionTime` **existing** (forest, read-once) | – | `gv.read` at the call site (S1) |
| `ForestViewModel.createChildEntry` | Core.swift:254-288 | `forest_position_after_children` (:256) → **key-only** `.appendPosition(in:)`, decoding the last `.drop` of `ChildSlots` (gv-ui child_slots.rs:76-80 == forest.rs:197) | `CreateEntryFromActivity` / `CreateEntry` | Write helper (S1); position read → key (S1) |
| `ForestViewModel.createRootEntry` | Core.swift:222-252 | `SuggestedRootInsertionTime` read-once (:224) **existing** | `CreateEntryFromActivity` / `CreateEntry` | Write helper (S1) |
| `ForestViewModel` other writes | Core.swift:134 `moveEntry`, :144 `updateEntryTemporal`, :156 `moveEntryToRoot`, :190 `updateAttributeValue`, :208 `clearAttributeValue`, :213 `removeAttribute`, :290 `updateEntryCompletion`, :299 `deleteEntry`, :310 `setIsSequence`, :322 `setDisplayAsSets`, :335 `convertToSets`, :350 `duplicateEntry` | – | 12 record-level actions | **Move to GvKit write helpers** (S1). `moveEntryToRoot`'s temporal math (:159-168) is pure; it can optionally become a Rust free fn, like `mass_value_converted_to` (types.rs:281). |
| `ForestViewModel.pendingExpandedEntryIds` | Core.swift:340,359-363 | – | – | **Stays UI state** (real UI logic: one-shot expansion across an identity swap). Moves to a tiny `@Observable EntryExpansion` in env (S5). |
| `AttributesViewModel` | Core.swift:373-387 | `AllAttributes` → **existing** | – | **Hook** (`.allAttributes`); delete (S2). Also fixes the stale-list TODO at :366-371. |
| `EditAttributesViewModel` | Features/Log/EditAttributesView.swift:10 (start :27-46, refresh :60-80) | `FindAttributesByOwner` **existing**; `FindEntryJoinById(entry)` **existing**; template root via `forest_activity_template_root` (:52-58) then `FindEntryJoinById(root)` → **NEW-SQL `ActivityTemplateJoin`** (composed from `FindActivityTemplateRoot` + `FindEntryJoinById`, no domain-set change) | `AttachValue` / `DeleteAttributeValue` :95-103 | **Stays a view model** (real UI logic: frozen display order, now in view `@State` :123-124,198ff; two-column attach/detach). Becomes `@Observable` and holds 3 `GvLiveQuery` boxes (S4). |
| `TextSuggestionsModel` | Features/Log/Attributes/TextAttribute.swift:266-301 | `DistinctTextValuesForAttribute` **existing** (SQL), live only while focused | – | **Box held by the view**: `@State var suggestions: GvLiveQuery<[String]>?`, set on focus and nil on blur. Delete the model (S5). |
| `AttributeDetailViewModel` | Features/Library/Attributes/AttributeDetailView.swift:8 (start :17-26, refresh :28-34) | `FindAttributeById` **existing** | `UpdateAttribute` :36-40 | **Hook** + write helper; delete VM (S3) |
| App wiring | GainzvilleApp.swift:12-22, 25-53, 61-71, 77-86 | builds every VM, `CoreEnv`, `DataChange`, listener | – | Shrinks in each step; S6 leaves `GvClient` + UI-state objects. |
| `GainzvilleCore.read` ext | GvKit/GvClient.swift:115-125 | `live_read` for code without a `GvClient` (only `ForestViewModel`) | – | **Delete** (S5) |

### 1b. Views

| View | file:line | Reads (today → target) | Writes (today) | Classification |
|---|---|---|---|---|
| `LogView` | Features/Log/LogView.swift:5,23,25,45; preview :243 | `forestVM.suggestedRootInsertionTime` → `gv.read(.suggestedRootInsertionTime(on:))` | `moveEntryToRoot` :25, `createRootEntry` :45 | Hook/read (done for roots); writes → helpers |
| `DayRootsList` | LogView.swift:69-78 | `@GvQuery(.dayRoots)` **done** (`RootEntriesInInterval`, SQL) | – | Done |
| `EntryView` | Features/Log/EntryView.swift:37-59,130,136-138 | `join` hook **done** (:39,53); `selectedMemberJoin` via `load` (:44,54,136-138), which shows an empty first frame; `setsMembers` = `forestVM.children` (:57-59) → **`SetsMembers`**; `consumePendingExpanded` (:130) | – | Hook. Sets restructure (S5). |
| `EntryHeader` | EntryView.swift:169,202 | props | `updateEntryCompletion` :202 | Write only |
| `ChildrenSection` | EntryView.swift:255-276 | `@GvQuery(.childSlots)` **done** | – | Done |
| `SetsBody` | EntryView.swift:284-341 | props | – | No change (gets members from EntryView) |
| `SetsControl` | EntryView.swift:347-407 | props | `duplicateEntry` :396, `deleteEntry` :405 | Write only |
| `EntryFooter` | EntryView.swift:411-456 | (`createChildEntry` reads the append position inside the VM) → `@GvQuery(.appendPosition(in: entry.id))`, which shares the Rust subscription with `ChildrenSection` | `createChildEntry` :442, `updateEntryCompletion` :450 | Hook + writes |
| `EntryMenuContent` | EntryView.swift:485-593 | props | `duplicateEntry` :507, `setDisplayAsSets` :511, `convertToSets` :515, `deleteEntry` :560,:565 | Write only. `EditAttributesView` push at :527 must pass `gv` (S4). |
| `DropTarget` | Features/Log/EntryDragDrop.swift:90-126 | `gv.read(.wouldCreateCycle)` **done** (:104) | `moveEntry` :126 | Write only |
| `TemporalAttribute` | Features/Log/Attributes/TemporalAttribute.swift:32,197,207 | props | `updateEntryTemporal` ×2 | Write only |
| `DurationAttribute` | TemporalAttribute.swift:228,245 | props | `updateEntryTemporal` | Write only |
| `NumericAttribute` | Features/Log/Attributes/NumericAttribute.swift:20,54,172,234,237 | props (`pair` from join) | remove/update/clear ×4 | Write only |
| `SelectAttribute` | Features/Log/Attributes/SelectAttribute.swift:15,107,113,177,192 | props | ×4 | Write only |
| `MultiselectAttribute` | Features/Log/Attributes/MultiselectAttribute.swift:17,101,106,124,127 | props | ×4 | Write only |
| `MassAttribute` | Features/Log/Attributes/MassAttribute.swift:19,69,205,260,309,312 | props | ×5 | Write only |
| `LengthAttribute` | Features/Log/Attributes/LengthAttribute.swift:17,67,203,258,307,310 | props | ×5 | Write only |
| `TextAttribute` | Features/Log/Attributes/TextAttribute.swift:25-27,42,103-107,236,239 | suggestions via `TextSuggestionsModel` + `CoreEnv` + `DataChange` → box | remove/clear/update ×3 | Mixed: box + writes (S5) |
| `CreateEntrySheet` | Features/Log/CreateEntrySheet.swift:12,19-21 | `activitiesVM.activities` → `@GvQuery(.allActivities)` | – | Hook (S2) |
| `EditAttributesView` | Features/Log/EditAttributesView.swift:115-117,164-167 | `CoreEnv` + `DataChange` + VM | via VM | View-model screen (S4) |
| `LibraryView` | Features/Library/LibraryView.swift:11-12,28,30; preview :74-75 | both list VMs → `@GvQuery(.allActivities)`, `@GvQuery(.allAttributes)` | – | Hook (S2) |
| `ActivitiesListView` | Features/Library/Activities/ActivitiesListView.swift:5,48 | props | `createActivity` :48 | Write only (S2) |
| `ActivityDetailView` | Features/Library/Activities/ActivityDetailView.swift:6,27,40 | `forestVM.templateRoot` → `@GvQuery(.activityTemplateRoot(activity.id))`. Optional: `.activity(id)`, so the name and description update live (today they come from the nav value). | `setIsSequence` :40 | Hook + write (S5: it embeds `EntryView`) |
| `AttributeDetailView` | Features/Library/Attributes/AttributeDetailView.swift:46-48,86 | VM → `@GvQuery(.attribute(id))` | via VM `apply` | Hook (S3) |
| `AttributesListView` | Features/Library/Attributes/AttributesListView.swift | props only | – | No change |

Not core consumers: `LogDayStore`, `DragState`, `AttributeFocusModel`, `AutocompleteCoordinator`,
`RootDropTimePickerSheet`, `AttributeActionBar`, navigation, settings.

Write call sites: 42 `forestVM.*`, plus 1 `activitiesVM.createActivity` and 2 VM-internal
`runAction` calls (EditAttributes :102, AttributeDetail :38).

---

## 2. New Rust and GvKit work

### 2a. New `ClientQuery` members (both land in S0)

| Member | Backing | Defined in | Answer | Notes |
|---|---|---|---|---|
| `SetsMembers { sequence_id }` | forest | `gv-ui/src/queries.rs` (beside `ChildSlots` :21) | `Vec<Entry>` | `forest.children(id)` if the parent is `display_as_sets`, else `[]`. Screen-shaped, so it can later carry set numbering and an affordance like `can_remove = len > 1` (RFD "other candidates"). Test: membership and order match `child_slots` entries. |
| `ActivityTemplateJoin { activity_id }` | SQL | `core/src/queries.rs` (client-only, like `RootEntriesInInterval` :121) + `QueryExecutor` impl in gv-sql SQLite only | `Option<EntryJoin>` | Composes `FindActivityTemplateRoot` → `FindEntryJoinById` in one transaction. Removes the cold-start race hack at EditAttributesView.swift:48-58. Not added to `DomainQuery`. |

Each one needs: a struct and `Query` impl; one arm in `ClientQuery`, `ClientQueryResponse` and
`execute` (client/queries.rs:34,58,89); a uniffi remote record plus the enum variants (gv-ffi
types.rs:~557, 632, 652); and a Swift key.

**Key-only additions** in `GvKit/GvQueries.swift`, no Rust:
- `.allActivities`, `.allAttributes`, `.attribute(id)`, `.attributesOwned(by:)`,
  `.distinctTextValues(attributeId)`, `.activityTemplateRoot(activityId)`, `.activityTemplateJoin(activityId)`, `.setsMembers(of:)`.
- `.appendPosition(in:)`: query `.childSlots(parentId)`, decoded to the last `.drop`'s `position`.
  Rust dedupes identical queries (live.rs:279), so the footer and `ChildrenSection` share one
  subscription and always agree.

**Optional prune (S6):** after the migration Swift doesn't use `FindEntryById`, `FindAncestors`,
`FindDescendants`, `FindActivityById` or `FindAttributePairsForEntry`. Drop them from the client set to
keep Swift's surface honest. `FindEntryById` is used by a client.rs:446 test, which can switch to
`FindEntryJoinById`.

### 2b. GvKit (S0)

- **Public box** `GvLiveQuery<Value>`: rename of `GvQueryBox` (GvKit/GvQuery.swift:87). See decision (b).
- **`@GvQuery`** becomes a thin adapter: `@State var box`, `update()` → `box.bind`. `projectedValue`
  returns the box, which replaces `GvQueryState` (GvQuery.swift:70).
- **Write helpers** in `GvKit/GvWrites.swift`: `extension GvClient` with the 12 `ForestViewModel`
  writes plus `createActivity`, `updateAttribute`, `attachValue`, `detachValue`, all over one
  `func run(_ action: Action)`. That function calls `core.runAction` and logs failures (today
  they're swallowed with `try?`). Domain names now mean the intent phase changes bodies, not call sites.
- **Signposts** around `core.liveSubscribe` in `GvClient.subscribe` (GvClient.swift:75), so Hangs and
  Points of Interest show per-query subscribe cost.

### 2c. Rust infra (S0)

- **Forest warm-up off the main thread:** add `LiveStore::warm()`, which calls `latest_forest`
  (live.rs:309). Spawn it from `GainzvilleCore::new` (gv-ffi core.rs:96) after `init`, so the first
  forest-backed `update()` doesn't load `AllEntries` on the main thread. There's no FFI signature
  change.
- **Delete the dead FFI methods now.** Nothing in Swift calls `forest_roots_in` (core.rs:233),
  `forest_ancestors` (:256), `forest_would_create_cycle` (:271) or `forest_position_between` (:281).

### 2d. Retire at the end (S6)

- **gv-ffi core.rs:**
  - `CoreListener` (:47) and the `listener` param and wiring of `new` (:99,106-114);
  - `FfiQuerySubscription` (:78);
  - `subscribe_query` (:137), `read_query` (:150), `subscribe_forest` (:215);
  - `forest_roots` (:226), `forest_children` (:240), `forest_activity_template_root` (:250),
    `forest_position_after_children` (:265), `forest_snapshot` (:382);
  - test `NoopListener` (:400).
- **gv-ffi types.rs:**
  - `DomainQuery` (:580) and `DomainQueryResponse` (:605);
  - the 9 remote records that only `DomainQuery` uses: `IsEmailRegistered`, `FindUserById`,
    `FindUserByUsername`, `AllActorIds`, `AllEntries`, `EntriesRootedInTimeInterval`,
    `FindValueByKey`, `FindValuesForEntry`, `FindValuesForEntries`. Keep `Email` and `Username`
    custom types, which `User` uses (:108-112).
- **gv-client:**
  - delete `query_store.rs`, after moving `next_change` (:153) into `live.rs` and `run_query` (:64)
    onto `SqliteClient` directly;
  - `client.rs`: drop the `query_store` and `cache_ready_transmitter` fields and their setup
    (:33-35,124-132), plus `run_domain_query` (:256), `subscribe_query` (:260),
    `read_cached_query` (:264) and `subscribe_cache_ready` (:283);
  - tests at :323 and :404 (QueryStore-specific) are deleted;
  - live.rs:16-17 doc ("beside QueryStore") is updated.
- **gv-core:** `DomainQuery` stays. Mutators and the server use it through `DomainQueryExecutor`;
  it only leaves the FFI.

---

## 3. Decisions to settle first

### (a) Read-your-writes, with intents deferred

**Today nothing has RYW.** `run_action` returns at commit (core.rs:121). QueryStore refreshes
asynchronously and then pings, and `forest_*` reads that same trailing cache. The new path trails the
same way: `LiveStore` versions trail commits (live.rs:207-212). So migrating keeps parity. The
question is whether to add RYW now.

**Every write-then-expect site:**

| Site | Expects | Mechanism today | Needs RYW? |
|---|---|---|---|
| `convertToSets` Core.swift:335-346 → `EntryView.onAppear` consume :130 | New sets card mounts expanded | Pending marker, consumed whenever the card appears | No, eventual by design. Keep the marker (`EntryExpansion`). |
| `SetsControl.addSet` EntryView.swift:394-399 | New copy becomes selected | `selectedMemberId = nil` tracks the last member | No. Old last shows for a frame (same as today). |
| `SetsControl.removeSelected` :401-406 | Neighbour selected | Selection moved before the write | No |
| `createChildEntry` via `EntryFooter` :442 | Append position fresh | Reads trailing forest cache | Only for a re-add before the batch lands, which would duplicate a frac index (`child_slots` then omits that gap, child_slots.rs:58-62). The sheet round-trip makes that practically impossible. No. |
| `createRootEntry` LogView.swift:45 / Core.swift:224 | Suggested time after the last root | Read-once of the published forest | Two creates on a past day within one rebuild get the same time. Cosmetic. No. |
| `DropTarget.performDrop` EntryDragDrop.swift:126 | Next drag sees new slots | Slots trail by one rebuild | Only for a second drop within ms. No. |
| `moveEntryToRoot` LogView.swift:25 | – | No read | No |
| `EditAttributesViewModel.toggle` :95-103 | Second tap sees the first | Reads `attachedIds` | A double-tap inside one rebuild sends the same action twice, which is idempotent in core. No. |
| `ActivityDetailView` sequence checkbox :40 | Second tap sees the first | Reads `root.isSequence` | Same as above. No. |
| Attribute editors (Numeric/Select/Multiselect/Mass/Length/Text/Temporal/Duration) | Field shows the committed value | Local edit state; re-sync on `onChange(of: pair.actual)` when unfocused | No |
| `createActivity` ActivitiesListView.swift:48, `updateAttribute` AttributeDetailView.swift:38 | List/detail update | Eventual | No |

**Options:**

1. **`sync()` inside FFI `run_action`.**
   - Each write blocks the main thread on a full rebuild: `AllEntries`, a new forest, and every
     subscription re-answered, including N `FindEntryJoinById` SQL reads. That cost grows with history.
   - It still doesn't give hook RYW within the same handler, because the batch hops to main via
     `Task` (GvClient.swift:65-69) after `run_action` returns. Only read-once gets fresh.
   - Rejected.
2. **Opt-in `await gv.settle()`.** `LiveStore::sync()` runs off main, then GvKit awaits an applied
   version ≥ the synced one.
   - It needs two things:
     - `swap_in` (live.rs:408) publishes before the sink runs, and empty batches aren't sent
       (:337). So Rust must send a version-only batch whenever a `sync` waiter exists, or always.
     - `GvClient` must track `appliedVersion` and resume waiters in `apply`.
   - About 60 lines in Rust and Swift. Gives real hook RYW where awaited, with no main-thread blocking.
3. **Version-at-commit** (architecture comment rec 2). `run_action` returns the commit's sequence
   number and versions *are* sequence numbers, so `settle(until: seq)` is exact and forces no extra
   rebuild. It's the right end state and aligns with sync.md's local sequence numbers, but it belongs
   with the sync engine.

**Recommendation:**
- Keep commit-return semantics and don't call `sync()` on the write path. No site above needs it,
  because the app already designs around eventual updates (markers, "nil tracks last").
- Don't build `settle()` in this migration. Specify it (option 2) as the contingency if a manual check
  shows a visible glitch, and give it the same signature version-at-commit will have, so call sites
  survive.
- Record in the RFD that RYW waits for version-at-commit.

### (b) The public box API

**Constraints:**
- `DynamicProperty`, `@State` and `@Environment` only work in views (platform comment).
- A model gets no environment in `init`.
- `@State(initialValue:)` is evaluated on every view init and discarded after the first, so the box's
  init must not subscribe.
- Updates must arrive in the same `apply` step as hooks (principle 3), which rules out per-box
  AsyncSequence and Combine streams.

**Shape (recommended):**

```swift
@MainActor @Observable
final class GvLiveQuery<Value>: GvSubscriber {
    /// Cheap: no subscription until first read of `value`/`error` (or `bind`).
    init(_ key: GvKey<Value>, client: GvClient?)
    var value: Value { get }            // default until first answer, then last good value
    var error: (any Error)? { get }     // side channel, as today
    func load(_ key: GvKey<Value>)      // re-target (query depends on state)
    // reserved: var isLoading: Bool    // RFD "loading state", not built now
    internal func bind(declared: GvKey<Value>, client: GvClient?)   // used by @GvQuery.update()
}

@propertyWrapper struct GvQuery<Value>: DynamicProperty {
    @State private var box: GvLiveQuery<Value>     // created unbound, bound in update()
    var wrappedValue: Value { box.value }
    var projectedValue: GvLiveQuery<Value> { box }  // $x.error, $x.load(...)
}
```

**Rules:**
- Models that hold boxes are `@Observable`. An `ObservableObject` VM doesn't forward a nested
  `@Observable` box's changes, so the view wouldn't re-render. The view stores the model in `@State`
  and builds it in `init` from a `gv` passed down by the parent. Because the box init is lazy, the
  discarded `@State` initial values never subscribe.
- `deinit` releases the handle through ARC. Routes are pruned lazily, as today (GvClient.swift:104).
- Keep `revision`-driven observation (GvQuery.swift:88-104). Lazy subscribe on first read happens
  inside `body`, so it must write storage without bumping `revision`, just like `bind`.
- GvKit is an app folder, so "public" means "documented GvKit API". Making it a Swift package is a
  separate decision.

**Alternatives rejected:**
- `AsyncSequence` per query: each `for await` hop is separate, which breaks batch consistency.
- Combine: same problem, plus the old pattern.
- A `GvClient.shared` singleton: hurts previews and tests.

---

## 4. Migration order

Each step ships on its own (app builds and runs, no screen on two pipelines), and ends with the
listed deletions. "Screen" means everything rendered under one navigation destination or sheet.
`EntryView` is shared by the Log and the activity-template editor, so those move together in S5.

| Step | Scope | Rust/FFI | Swift changes | Deleted at end | Done when |
|---|---|---|---|---|---|
| **S0 Foundations** | No screen behavior change | `SetsMembers`, `ActivityTemplateJoin` (+ tests); `LiveStore::warm()` spawned from `GainzvilleCore::new`; delete 4 dead `forest_*` fns; **regen bindings** | `GvLiveQuery` public box + `@GvQuery` adapter; all new keys (§2a); `GvWrites.swift`; signposts | `GvQueryState`; `forest_roots_in`, `forest_ancestors`, `forest_would_create_cycle`, `forest_position_between` | Rust tests green; both targets build; app unchanged |
| **S1 Writes** | All `forestVM.*` writes (42 sites, §1b) → `gv.*` helpers. The two reads that only feed writes move too, both onto data already on the new path: `EntryFooter` takes `.appendPosition(in:)` (same subscription as `ChildrenSection`), and `LogView` uses `gv.read(.suggestedRootInsertionTime)`. After S1, `forestVM` is read only for `children`, `consumePendingExpanded` (EntryView) and `templateRoot` (ActivityDetailView). | – | Mechanical edits in 12 files | The 12 write methods on `ForestViewModel` (Core.swift:134-356 minus `convertToSets`'s marker insert, which moves next to the helper call in `EntryMenuContent`) | `grep -n "forestVM\.\(move\|update\|create\|delete\|set\|convert\|duplicate\|remove\|clear\)"` is empty |
| **S2 Library lists + Create sheet** | `LibraryView`, `ActivitiesListView`, `CreateEntrySheet` | – | Hooks `.allActivities` / `.allAttributes`; `gv.createActivity` | `ActivitiesViewModel`, `AttributesViewModel` (Core.swift:49-99, 366-387); GainzvilleApp.swift:16-17,32-36,48-49,65-66,80-81; preview env LibraryView.swift:74-75 | grep for both VM names is empty |
| **S3 Attribute detail** | `AttributeDetailView` | – | `@GvQuery(.attribute(id))`, `gv.updateAttribute` | `AttributeDetailViewModel` (AttributeDetailView.swift:4-41), its `CoreEnv`/`DataChange` env objects | – |
| **S4 Edit attributes** | `EditAttributesView` (pushed from the entry menu) | – | `@Observable EditAttributesModel` holding 3 boxes (`.attributesOwned(by:)`, `.entryJoin(entry.id)`, `.activityTemplateJoin(activityId)`); frozen order stays; `.onReceive(vm.$attributes)` → `.onChange(of: model.attributes.value, initial: true)`; `EntryMenuContent` passes `gv` | Old VM body (EditAttributesView.swift:9-104), the `forest_activity_template_root` dependency, `CoreEnv`/`DataChange` env objects | Must land **before S5**. It reads the forest cache that only `ForestViewModel`'s `subscribeForest` keeps alive. |
| **S5 Entry tree** (Log + template editor) | `EntryView` + subviews, `ActivityDetailView`, `TextAttribute` | – | `@GvQuery(entry.displayAsSets ? .setsMembers(of:) : .inactive)` declared in `init`. Sets card's selected-member content goes in a child view that declares `.entryJoin(member.id)` in `init` (replaces `load`, fixes the empty first frame; the header moves with it). `EntryExpansion` env model for the convert-to-sets marker. `ActivityDetailView` uses `.activityTemplateRoot`. `TextAttribute` holds `GvLiveQuery<[String]>?` on focus. | `ForestViewModel` entirely, including its `subscribeForest` (drops QueryStore's per-write `AllEntries` reload); `TextSuggestionsModel`; `GainzvilleCore.read` ext (GvClient.swift:115-125); `forestVM` wiring GainzvilleApp.swift:18,38-39,50,67,82; LogView preview :243 | grep for `forestVM`, `ForestViewModel`, `coreEnv` and `dataChange` is empty, except the definitions in Core.swift and GainzvilleApp |
| **S6 Retire legacy** | Plumbing only | All of §2d; optional client-set prune; **regen bindings** | Delete `AppListener`, `CoreEnv`, `DataChange` (Core.swift:18-47); `makeCore` drops `listener` (:389,407-411); GainzvilleApp listener wiring (:25,41,43-53,63-64,78-79) | `QueryStore`, `DataChange`, `CoreEnv`, `AppListener`, `CoreListener`, `forest_*`, `subscribe_forest`, `subscribe_query`, `read_query`, `on_data_changed`, `FfiQuerySubscription`, `DomainQuery`/`DomainQueryResponse` in the FFI | `grep -rn "subscribeQuery\|readQuery\|forest[A-Z]\|onDataChanged\|DomainQuery" swift-app/Gainzville --include=*.swift` hits only `gv_ffi.swift` before regen, and nothing after. Update docs: `docs/actions_and_queries.md`, `docs/swift-architecture/{design,gv-ios-architecture}.md`, `docs/sets-design.md`, `swift-app/SWIFT-APP.md`, RFD 3 "Spike" → done. |

---

## 5. Risks and verification

**Every step:**
1. `docker compose up -d` first: the workspace build compiles `gv-server` against live Postgres
   (CLAUDE.md).
2. Run `cargo test` from the workspace root, not `--package`.
3. Rebuild the Swift artifacts:
   - after an FFI change (S0, S6): `scripts/regen-bindings.sh`;
   - otherwise only after a Rust change: `scripts/rebuild-xcframework.sh`.
4. From `swift-app/`, build both targets with xcodebuild: `-target Gainzville` and
   `-target 'Gainzville macOS'` (SWIFT-APP.md:39-42).

| Step | Extra Rust checks | Manual checks (Mac, iOS sim + macOS) |
|---|---|---|
| S0 | gv-ui test: `SetsMembers` == entry slots of `child_slots` for sets parents, `[]` otherwise; live.rs test: `ActivityTemplateJoin` follows an `AttachValue` on the template root; gv-ffi round-trip (core.rs:436) still passes | Cold launch with a large DB (seed `devCreateArbitraryEntries(500)`); Instruments **Hangs** + signposts: first render has no `AllEntries` load inside `update()` |
| S1 | – | Spike checklist for writes: drag reorder (incl. refused self/cycle drops), day-root drop + time sheet, completion toggle, each attribute editor commit/clear/remove, temporal debounce, duplicate, convert to sets / break out, delete expanded entry, template sequence toggle |
| S2 | – | Library lists render. Create an activity: it appears in the list and in the Create Entry sheet. Background ticker (`startBackgroundTicker`) updates the list live. |
| S3 | – | Edit each config default and see it reflected live; nav title follows the name |
| S4 | – | Open Edit Attributes on: a scratch entry; an activity entry (both columns); a cold launch straight into it (the old race). Order stays frozen while toggling. New attribute appends. |
| S5 | – | Full spike checklist (RFD "Verified on a Mac"); sets: select / add / remove with no tearing between card and pills (platform concern), convert-to-sets opens expanded, first frame of a sets card shows its title; template editor in Library; text autocomplete on focus, gone on blur; burst of 20+ seeded entries; no "modifying state during view update" warnings; Hangs trace on scroll through a full day |
| S6 | `cargo test` after deleting QueryStore tests; the `Lagged` regression stays covered by client.rs:434 | Smoke of everything above; launch with `-wipeDB` (seeding path without a listener) |

**Known risks:**

- **Main-thread SQL on subscribe.**
  - `GvQuery.update()` → `liveSubscribe` → `block_on` (core.rs:174) runs SQL inside SwiftUI's update
    pass. That's one `FindEntryJoinById` per card on day switch, `DistinctTextValuesForAttribute` on
    focus, and the library lists.
  - Mitigation: signposts (S0) and a Hangs budget check in S0/S5.
  - Escalation, out of scope: async first value with a `isLoading` side channel.
- **Cold forest load.**
  - The first forest-backed subscribe loads `AllEntries` (live.rs:309-324).
  - Mitigation: `warm()` at core init (S0). If the UI beats the warm-up, both load, and
    `get_or_insert` keeps one (live.rs:322). That's harmless.
- **Double work until S5.** QueryStore re-runs `AllEntries` (held by `subscribeForest`) on every write,
  alongside LiveStore's rebuild (skeptic). It goes away in S5, so don't linger between S1 and S5.
- **Per-version cost.** Each write re-answers every mounted `FindEntryJoinById` in SQL (architecture
  concern; `ChildSlot` carries `Entry`, not the join). That's accepted. Whether to answer
  joins from memory at all (a full in-memory model) is an open question.
- **`ObservableObject` holding `@Observable` boxes** won't re-render. S4 must convert the VM (§3b).
- **Swift 5 language mode.** Main-actor assumptions (`MainActor.assumeIsolated` in GvQuery.swift:62)
  are only checked at runtime, so watch the console in debug runs.
- **Previews.** `gvClient` is nil, so hooks show defaults and write helpers no-op. That's acceptable;
  an in-memory preview client is a follow-up.
- **Generated-binding conflicts.** `gv_ffi.swift` changes only in S0 and S6. Any parallel branch
  rebases and regenerates instead of hand-merging.
- **Clock-dependent `SuggestedRootInsertionTime`** must stay read-once. Never make it a hook key.
- **`.inactive` keys and `@State` selection.** Re-keying the sets child view on `member.id` (as
  EntryView.swift:334 does in `SetsBody`) resets editor state, which is intended. Verify focus doesn't jump on
  "+" set.

---

## 6. Sizing and parallelism

| Step | Size | Rough diff | Notes |
|---|---|---|---|
| S0 | M | Rust ~250 (2 questions, tests, warm, deletions), Swift ~200 (box refactor, keys, writes) | One agent; blocks everything |
| S1 | S | ~45 call-site edits in 12 files | Mechanical; one agent |
| S2 | S | ~60 lines removed, ~20 added | Parallel with S3, S4 |
| S3 | S | ~40 removed, ~10 added | Parallel with S2, S4 (can fold into S2) |
| S4 | M | Model rewrite ~100 lines | Parallel with S2, S3; must merge before S5 |
| S5 | L | EntryView restructure + 4 files; ~250 lines changed | One agent; the riskiest step, so do it with the Mac checklist |
| S6 | M | Rust −500 lines, FFI −300, Swift −80, docs | One agent, last |

**Critical path:** S0 → S1 → S5 → S6.
- S2, S3 and S4 run in parallel with S1. They touch no files S1 touches, except that S2 owns
  `ActivitiesListView`'s `createActivity` (leave it out of S1).
- All of S1–S5 touch `GainzvilleApp.swift`, but only to delete wiring lines, which are trivial
  merges. Land S2–S4 in any order, then S5.
- **Wall-clock:** about 4 agent-sessions if S2, S3 and S4 run alongside S1.

## Follow-ups (not in this migration)

`settle()` (§3a option 2) if needed, then version-at-commit with sync; hook loading state; in-memory
preview client; intent-level writes, which replace `GvWrites.swift` bodies; an open question,
whether to hold all data in memory (full model, join map); GvKit as a Swift package with internal FFI types.
