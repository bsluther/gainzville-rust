# RFD 0003 comment: architecture

- Date: 2026-10-30
- Lens: fit with the existing model, crate boundaries, docs, and decided RFDs.
- Read: RFD 3 as of 4e8e369 (focus: Latest / lean 2026-10-29); spike `spike/gvkit-query-hooks` at
  c11ec80; sources 10-27, 10-29 (notes, prior art), 10-30; `CLAUDE.md`,
  `docs/{actions_and_queries,boundary-transformations,forest-ui-model,sync}.md`; spike
  `client/live.rs`, `gv-ui/src/child_slots.rs`, `gv-ffi/src/{core,types}.rs`, `core/src/queries.rs`,
  `gv-sql/*/query_executor.rs`, `GvKit/*.swift`, `Features/Log/*`.

## Verdict
The lean fits: queries and actions were already the contract, and `gv-ui` as a pure crate is the
right layer. But the spike shows two unnamed seams: "everything is a query" is really two query
registries with different obligations, and the version trails commits, so the backing leaks.

## Concerns
- Re: Open, query macro: the tax is structural. `AnyQuery` serves mutators on both backends
  (`AnyQueryExecutor`), so a UI question like `RootEntriesInInterval` needs a Postgres impl the
  server never calls. The spike already has a second registry (`LiveQuery`, with a `Domain(AnyQuery)`
  passthrough). A macro would hide that split, not fix it.
- Re: Principle 2: semantics vary by backing. Forest answers come from the last published version;
  SQL answers read now and can be a version ahead. Day roots go to SQL inside a build that already
  holds a forest that answers them (`Forest::roots_in`).
- Re: Shape 2: `version` is an in-memory counter that resets at launch. `sync.md` already has
  `run_action` assign a local sequence number.
- Re: Drop slots: `ChildSlot` carries `Entry`, so every card keeps a per-entry SQL join
  subscription, the pattern `forest-ui-model.md` records as having failed.
- Re: Reframe ("raw FFI types stay internal"): domain records are 1:1 uniffi mirrors, and views
  switch on `ChildSlot`/`Entry`. GvKit is an app folder, not a package.

## Missing
Which earlier-lean items survive (`Snapshot`, joins in the snapshot, intents returning a version,
the `generation`-shared model). A plan to retire `QueryStore`. "Version" also collides with RFD 2's
data version.

## Recommendation
Before migrating more views:
1. Split the registries. Domain queries serve mutators and the server. Client questions are
   answered by the forest or SQLite only.
2. Make the version the commit's sequence number, returned from writes. Read-your-writes follows,
   and sync reuses it.
3. Delta-apply the forest and add a join map.

Then use a macro only for the boilerplate that's left.

## Can expand on
- A file-by-file breakdown of the per-query tax for both registries.
- A version-at-commit design and how it fits with sync's sequence numbers.
- A public/internal type split for GvKit.
