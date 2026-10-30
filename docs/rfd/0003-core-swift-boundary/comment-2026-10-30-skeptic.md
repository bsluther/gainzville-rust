# RFD 0003 comment: skeptic

- Date: 2026-10-30
- Lens: argues against the current lean, or against doing this now at all.
- Read: RFD 3 at 4e8e369 (lean 2026-10-29); spike `spike/gvkit-query-hooks` at c11ec80; transcript
  2026-10-27, discussion notes, prior-art research, voice note 2026-10-30; RFD 2; `CLAUDE.md`;
  `client/live.rs`, `client/query_store.rs`, `gv-ui/src/child_slots.rs`, `gv-ffi/src/core.rs`,
  `GvKit/*.swift`, `EntryView`/`LogView`/`EntryDragDrop.swift`, `Core.swift`.

## Verdict
The spike shows the lean works, and what it costs: ~2,400 hand-written lines to retire ~180 lines
of Swift, with read-your-writes still open and the Swift not yet compiled. A much smaller change
gets most of the value. Don't build `LiveStore` before dogfooding (RFD 2).

## Concerns
- Re: Shape 2–3: each version rebuilds the whole forest from `AllEntries` and re-runs every mounted
  `FindEntryJoinById` in SQL. `QueryStore` still makes its own pass, so per-write work doubles during
  migration and still grows with history (problem 4).
- Re: First principles 3: `EntryView` reads its join from the new pipeline and `setsMembers` from
  `forestVM.children` on the old one. One view can mix two versions.
- Re: Principle 4: `live_subscribe` runs a `block_on` SQL read on the main thread inside
  `update()`, the same per-EntryView cost as today.
- Re: Hypothesis (hooks): the first slice already needed a `$x.load` escape hatch and a
  `DayRootsList` split just to supply inputs at init.
- Re: Open (macro): each query touches ~7 files plus bindings. Hooks before the macro pay that cost
  for every screen migrated.

## Missing
Why now, given RFD 2. None of this makes data durable, and sync will reshape the boundary again.

## Recommendation
Keep the cheap wins:

- the `Lagged` fix (0b5c270);
- a coherent `QueryStore` refresh (one transaction, swap, version);
- `child_slots` exposed as one forest method;
- a small generic Swift wrapper over today's subscribe/read/`DataChange` (Path step 1, no Rust
  change), which closes the missing-sink class.

Park `LiveStore` until after the first dogfooding release and the sync choice.

## Can expand on
- Line-by-line cost accounting of the spike.
- What the small wrapper fixes and what it doesn't.
- Whether calling `sync()` on the write path is affordable.
