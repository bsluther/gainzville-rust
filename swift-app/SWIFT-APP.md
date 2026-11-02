# Swift App — Patterns & Guidelines

## Architecture

- **Business logic lives in `core`** (Rust, via FFI). The Swift app is a thin client: it asks domain questions through GvKit, states what the user did through GvKit write helpers, and renders.
- **UI state and rendering logic live in Swift.** The boundary between core concerns and UI concerns can be blurry (e.g. display names, temporal formatting) — consider carefully before adding logic to either side.
- **Target platforms: iOS and macOS.** All views should build for both. Use `#if os()` or platform-specific types when behaviour genuinely differs; don't bend design to avoid it.

## Reading and Writing Data (GvKit)

Views talk to Rust only through `Gainzville/GvKit/`. How an answer is produced (in-memory forest or SQL) is Rust's choice, made in one `match` in `client/queries.rs`, and Swift can't see it.

- **Subscribe with a hook.** `@GvQuery(.allActivities) var activities: [Activity]`, or set `_join = GvQuery(.entryJoin(entry.id))` in `init` when the question depends on inputs. The first value is there on the first render, and the hook re-targets when the view is re-created with a different question. A `nil` id makes it inactive. Errors are on `$join.error`.
- **Read once for imperative checks.** `gv.read(.wouldCreateCycle(entryId:under:))`. Questions that depend on the clock (`.suggestedRootInsertionTime`) must be read once, never subscribed: no data change would update them.
- **Write through helpers.** `gv.moveEntry(...)` and the rest of `GvWrites.swift`. They return at commit and log failures. Hooks follow on a later batch, so nothing reads its own write; design for eventual updates. `gv` is `nil` in previews.
- **Use a view model only for real UI logic,** not to fetch data. An `@Observable` model, or a view that needs a query only some of the time, holds a `GvLiveQuery` box directly (see `TextAttribute`). The box must not sit inside an `ObservableObject`, which wouldn't forward its changes.
- **Adding a question:** a struct and `Query` impl (`core/src/queries.rs`, or `gv-ui/src/queries.rs` if forest-backed), an arm in `ClientQuery`, `ClientQueryResponse` and `execute` (`client/queries.rs`), remote types in `gv-ffi/src/types.rs`, and a key in `GvQueries.swift`. Then run `scripts/regen-bindings.sh`.

The database runs in WAL mode, so `-wal` and `-shm` files sit beside `gainzville.sqlite`. A backup or export must use SQLite's backup API or `VACUUM INTO`, never a file copy.

## Code Style

- Prefer idiomatic Swift and modern SwiftUI (Swift 6+, `@Observable`, typed throws, etc.).
- Be willing to break out to `NSViewRepresentable` / `UIViewRepresentable` when SwiftUI can't achieve the design goal — AppKit/UIKit are first-class options, not last resorts.
- Subcomponents scoped to one view stay in that file. Extract to a shared location only when used across views.
- Platform-specific types are named by platform (`FooMacOS`, `FooIOS`), not by container (`FooPopover`, `FooSheet`). The container is external to the component.

## Design System

- Tokens: `GvColor`, `GvFont`, `GvSpacing` in `DesignSystem/`. Semantic aliases (e.g. `gvAttributeField`) live in `GvColor.swift` when they don't warrant a full asset catalog entry.
- `platformPopover` (`GvPresentation.swift`) handles sheet-vs-popover automatically. Use it for all picker-style overlays. If a use case doesn't fit (e.g. toolbar anchoring), drop down to direct `.sheet`/`.popover` — don't contort the abstraction.
- AppKit-backed pickers (`CalendarPickerMacOS`, `TimeFieldMacOS`) use `NSViewRepresentable` with cleared backgrounds to avoid double-box rendering. Follow this pattern for any new AppKit picker.

## Building & Updating the FFI Library

When Rust code changes, agents and developers need to rebuild the Swift-facing artifacts. Two scripts in `scripts/` handle this — always run from the **workspace root**.

| Situation | Script |
|-----------|--------|
| FFI surface changed (`types.rs`, exported types, new `FfiAction` variants, etc.) | `scripts/regen-bindings.sh` |
| Implementation-only Rust change (no `#[uniffi::export]` signature changes) | `scripts/rebuild-xcframework.sh` |

`regen-bindings.sh` regenerates `gv_ffi.swift` + headers, then rebuilds the xcframework. `rebuild-xcframework.sh` skips binding regeneration. **When in doubt, use `regen-bindings.sh`** — a mismatch between the Swift bindings and the compiled library causes a fatal crash at launch.

## Verifying the Swift Build

Run from `swift-app/` — use `-target`, not `-scheme` (scheme + destination hits a platform-resolution bug):

```bash
# iOS
xcodebuild -project Gainzville.xcodeproj -target Gainzville build CONFIGURATION=Debug 2>&1 | grep -E '(error:|BUILD SUCCEEDED|BUILD FAILED)'

# macOS
xcodebuild -project Gainzville.xcodeproj -target 'Gainzville macOS' build CONFIGURATION=Debug 2>&1 | grep -E '(error:|BUILD SUCCEEDED|BUILD FAILED)'
```

SourceKit LSP will show false "Cannot find type" errors for FFI types (`FfiEntry`, `FfiTemporal`, etc.) and design tokens (`GvSpacing`, `Color.gvSurface`). These are noise — trust `xcodebuild` output for real errors.

## Platform Gotchas

- **macOS drops the last `.onDrop` registration**: the final drop registration in the view tree is never consulted — no `validateDrop`, no rejection cursor, total silence — though the view renders normally. Any view hosting entry cards must append a `DropRegistrationSink` (`Features/Log/EntryDragDrop.swift`); `LogView` and `ActivityDetailView` do. Full evidence and root-cause research plan: [`docs/macos-drop-registration-bug.md`](./docs/macos-drop-registration-bug.md).

## Future / Open Questions

- **macOS drop-registration root cause**: the sink above is an empirical workaround; why AppKit skips the last registration is unknown. Research plan and open questions are in [`docs/macos-drop-registration-bug.md`](./docs/macos-drop-registration-bug.md).
- **Unset controls**: no UI yet to clear individual temporal values (start, end, duration).
- **Inline duration field (macOS)**: current stepper popover is a placeholder; long-term goal is an inline `hh:mm:ss` text field with a custom formatter.
- **Sync / offline**: the Swift app is read/write via FFI today; full offline-first sync behaviour is driven by the Rust `client` crate.
- **Entry reorder after temporal edit** (scroll-follow): when a user edits an entry's start/end time, the day's roots update and the entry moves to its new sorted position. The experience is jarring — the entry appears to vanish and the scroll position doesn't track it. Desired behaviour: scroll to the entry's new position and keep it expanded. Sketch of the fix: (1) add `focusedEntryId` to a small `@Observable` model, set where the temporal write happens; (2) wrap `LogView`'s `ScrollView` in a `ScrollViewReader`; (3) on `.onChange` of the day's roots, if `focusedEntryId` is set, call `proxy.scrollTo(id, anchor: .center)` via a short `DispatchQueue.main.asyncAfter` to let layout finish, then clear the id. The keep-expanded behaviour should come for free since `EntryView.isExpanded` is `@State` and SwiftUI preserves it for stable `id:`-keyed views across reorders. Deferred until the day-filter log view exists — when the log is scoped to a day, an entry moving to a different day requires navigating to that day first, which needs the navigation stack in place.
