# macOS: the last `.onDrop` registration is never consulted

**Status:** worked around, root cause unknown.
**Found:** 2026-07-20, macOS 26.3.1, Xcode 26.3.
**Workaround:** `DropRegistrationSink` (`Features/Log/EntryDragDrop.swift`), appended in
`LogView` and `ActivityDetailView`.

## The behavior

On macOS, the **last `.onDrop(of:delegate:)` registration in the view tree is absent from the
set AppKit hit-tests**. Not "it declines" — it is never asked. `validateDrop`, `dropEntered`,
and `dropUpdated` do not fire for it, at any cursor position, for the whole drag session.

The view itself is completely healthy: its `body` runs, layout is correct, and the `.onDrop`
modifier is applied. Only the dispatch is missing.

iOS is unaffected: the same tree dispatches to every registration normally.

### Symptom to recognise

- Drag ghost/preview appears and tracks the cursor for the entire drag.
- No drop indicator **and** no forbidden/rejection cursor — total silence.
- Ancestor drop targets higher in the tree still answer normally, so a trace shows one
  ancestor `validateDrop` at drag start and then nothing.

That silence is the tell. A registration that is consulted and declines still produces a
rejection cursor; this produces nothing at all.

## How it presented

Entry drag-and-drop was completely dead in the **log** on macOS, while the identical
`EntryView`/`DropTarget` code worked in the **library** template editor, and both worked on iOS.

The asymmetry was positional, not structural:

- The log's bottom entry card was the last drop registration in the tree. With a single root
  entry — the common case — that was the only card, so the log looked entirely broken.
- The library's template card is always followed by more content (the Sequence checkbox row,
  the Recent/Categories/Sub-Categories sections), so some other registration was always last
  and absorbed the loss.

Trailing `Color.clear` does **not** count as a registration and does not help: it has no hit
area for the drag system, which is the same reason `DropTarget` uses
`Color.white.opacity(0.001)` as its base layer. `LogView` already had a 600pt `Color.clear`
headroom below the cards and the bug occurred anyway.

## The workaround

`DropRegistrationSink` — a 1pt `Color.white.opacity(0.001)` view whose delegate returns `false`
from `validateDrop` and `performDrop`. Appended as the final element of the scroll content, it
becomes the last registration and is the one that gets dropped, leaving every real target live.

Any view hosting entry cards needs one. Currently: `LogView`, `ActivityDetailView`.

## Evidence (established experimentally)

Confirmed by instrumenting every delegate with prints and driving real drags:

| Observation | Established by |
|---|---|
| Drag source and `DragState` are fine | `onDrag` fires, `hasDragged=true`, single shared `DragState`, ghost tracks |
| Slots render and apply `.onDrop` | `DropTarget.body` prints 4 of 4 in the failing card, interleaved correctly |
| Payload arrives at the destination | `info.hasItemsConforming(to: [.data])` is `true` |
| Not the container | The failing log card is equally dead when hosted inside `ActivityDetailView` |
| Not the card's content | 5 byte-identical copies of the same card: only the last fails |
| Not `entryContext` chrome | Same entry with template chrome fails identically when last |
| Not scroll offset | Scrolling the failing card to the top does not revive it |
| Not the trailing spacer's hit area | `Color.clear` vs `Color.white.opacity(0.001)` headroom: no difference |
| It is ordinal | With 5 copies, #5 fails; add a 6th and #5 works while #6 fails |
| A sacrificial registration fixes it | Appending `DropRegistrationSink` revives the last card |

### Ruled out as causes

Nested-delegate dispatch order, `DayRootDropDelegate` (removing it entirely changed nothing),
the autocomplete `.overlayPreferenceValue` overlay, per-render closure identity in
`EntryDropDelegate`, sibling root collisions, `public.plain-text` payload hijacking by
`NSTextField` (switching to `public.data` changed nothing), `positionBetween` returning nil,
and `AttributesSection` / `TemporalAttribute` content.

## For the root-cause round

Open questions, in the order most likely to pay off:

1. **What does "last" mean exactly?** Last in the view tree's registration order, last in
   AppKit's `registeredDraggedTypes` on the hosting view, or last in some internal SwiftUI
   list? Determine whether it is the last *registered* or the last *in geometry/z-order* —
   these coincided in every case tested so far and were never separated.
2. **Read AppKit state directly.** Nothing in this investigation inspected the drop registry;
   it was all inferred from prints. Attach the Xcode View Debugger to the running app, or dump
   `NSHostingView` descendants and their `registeredDraggedTypes`, and compare a working card
   against the last one. This is the single biggest gap.
3. **Is it exactly one, or a boundary condition?** Only one victim was ever observed. Test
   whether N sinks protect N cards, and whether the count of registrations matters (a cap, an
   off-by-one in a list traversal).
4. **Does it depend on the container?** All observations are inside a `ScrollView`. Test a
   plain `VStack` with no scroll view, a `List`, and a `LazyVStack`.
5. **Is it version-specific?** Recorded on macOS 26.3.1 / Xcode 26.3 only. Check earlier and
   later SDKs; if it is a regression, an Apple Feedback report is warranted.
6. **Does `.dropDestination(for:)` share the fault?** The modern API may dispatch differently.
   If it is immune, migrating is a cleaner fix than the sink — but note the header comment in
   `EntryDragDrop.swift` claims `.draggable`/`.dropDestination` do not interoperate with
   `.onDrag` on iOS, so any migration must move both ends together and be tested on both
   platforms.

### Method notes (this took ~15 build cycles; most were wasted)

- **Isolate to one expanded card per test.** With several cards expanded simultaneously,
  results were irreproducible and produced two confidently-wrong conclusions.
- **Repeat each drag three times** before believing a result.
- **Prefer additive controls.** Six consecutive subtractive probes ("remove X from the log,
  see if it revives") produced six nulls. The result came from *adding* a card to the library
  and rendering identical copies — controls that can produce a positive.
- **Don't truncate UUIDs in traces.** `.prefix(8)` renders every std_lib id as `00000000`,
  which silently invalidated several comparisons.
- **Instrument `body`, not just delegates.** `dropTargets=4 of 4` from `buildSlots` proved only
  that slot *values* were constructed; `DropTarget.body` was needed to prove they rendered.
