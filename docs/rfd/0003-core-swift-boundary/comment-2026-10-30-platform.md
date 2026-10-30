# RFD 0003 comment: platform

- Date: 2026-10-30
- Lens: iOS/Swift/Xcode/TestFlight constraints and costs; here SwiftUI semantics, Swift concurrency, UniFFI-generated Swift
- Read: RFD 3 at `4e8e369` (Latest / lean 2026-10-29), its four sources, `SWIFT-APP.md`. Spike `origin/spike/gvkit-query-hooks` at `c11ec80`: `GvKit/{GvQuery,GvClient,GvQueries}.swift`, `Features/Log/{EntryView,LogView,EntryDragDrop}.swift`, `Core.swift`, `GainzvilleApp.swift`, generated `gv_ffi.swift`, `gv-ffi/src/core.rs`, `client/live.rs`, `project.pbxproj`. Nothing was compiled. SwiftUI behavior is from general platform knowledge.

## Verdict
DynamicProperty is the right mechanism for views. The spike's `@State` box, `revision` counter and version-gated weak routing are sound, so "modifying state during view update" shouldn't fire. It will probably compile: the target builds in Swift 5 mode with MainActor default isolation, so concurrency gaps show up as warnings, not errors. The real costs are synchronous SQL inside SwiftUI's update pass, and one screen split across two delivery paths.

## Concerns
- Re: spike `GvQuery.swift`: `update()` calls `liveSubscribe`, which runs a blocking (`block_on`) SQL read on the main thread in the middle of a render. The first forest-backed subscribe also loads all of `AllEntries` there (`ensure_forest`). That makes a cold-launch hitch that grows with data and shows up in TestFlight hang reports.
- Re: spike `EntryView.swift`: sets members still come from `forestVM.children` (the ping path), while entries arrive in batches. Two separate Task hops can tear frames on convert-to-sets and on member deletion, which breaks principle 3. `onChange(initial:)` + `load` also gives sets cards an empty first frame.
- Re: Shape 1 ("a view or an `@Observable` model"): `@State`, `@Environment` and `update()` only run inside views. In a model, the wrapper never subscribes.
- Re: spike `EntryDragDrop.swift`: each drop callback calls `gv.read`, a `block_on` call at pointer-move rate.

## Missing
Previews (with no client, views show only default values), a retry after a failed subscribe, and a main-thread time budget for subscribes.

## Recommendation
- Make the box a public `@Observable` type that models can hold, and keep `@GvQuery` as a thin view adapter over it.
- Warm the forest off the main thread at startup, and add signposts around subscribe.
- Declare dependent queries from a child view's init, as `DayRootsList` does, instead of using `load`.
- Move sets members to hooks before running the checklist. Add three checks to it: a cold launch with a large DB, the macOS target, and Instruments Hangs.
- Ship an in-memory preview client.

## Can expand on
- What changes when the target moves to Swift 6 language mode: DynamicProperty and isolated-conformance diagnostics
- How long subscriptions live in lazy stacks, TabView and `ViewThatFits`
- The checklist item by item: re-subscribes during a burst trigger rebuilds, and a deletion lands within one batch
- Computing drag validity once per drag
