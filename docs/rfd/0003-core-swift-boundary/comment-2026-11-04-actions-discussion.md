# RFD 0003 comment: actions discussion

- Date: 2026-11-04
- Lens: bespoke. A summary of a discussion with the author about "intent-level writes" and how the
  mutation log handles versions.
- Read: RFD 0003 at 9bda979; `GvKit/GvWrites.swift`; `core/src/{actions,mutators,instantiation,io}.rs`;
  `docs/sync.md`; `docs/actions_and_queries.md`; the database-engineer comments of 2026-11-02.

Longer than the usual comment, because it records a whole discussion. Several topics will move to
their own RFDs when this one closes.

## Verdict
Drop the separate idea of "intents." Actions are already meant to state what the user meant
(`actions_and_queries.md`: "Actions reify write intent"). The real finding is that some actions are
too low-level, and `GvWrites.swift` makes up for it. Fix the actions.

## Concerns
- Re: "Writes are intents": the test isn't whether Swift holds logic. It's whether a helper reads
  state and bakes the result into the action. The mutator runs in the write transaction and reruns on
  rebase (`sync.md`), so anything Swift computes from its snapshot is outside both.
  - `moveEntry` sends Swift's copy of `temporal`, and `updateEntryTemporal` sends Swift's copy of
    `position`. Each can undo a concurrent change to the other field.
  - `moveEntryToRoot` decides in Swift whether to keep the duration. Replayed after a concurrent
    duration edit, it restores the old duration.
  - `createChildEntry` passes `isTemplate` from the parent, and the mutator rejects a mismatch. It
    should be derived. The same holds for `CreateEntryFromActivity.is_template`.
- Re: ids: new ids belong in the action. `CreateEntryFromActivity` and `DuplicateEntry` mint them
  in the mutator (`mint_id_map`), so a replay mints different ids, and a later queued action that
  refers to them points at nothing. `ConvertToSets` already does it right. Who mints doesn't matter.
- Re: other senders: LLM import and the HTTP API send actions too. An intent layer in `gv-client`
  wouldn't help them.

## Missing
- The mutation log isn't stored yet, so history isn't kept. The format questions below need deciding
  before it is.

## Recommendation
- **Actions.**
  - One `MoveEntry` whose target is either under a parent or to the root at a start time. Editing
    time becomes its own action.
  - Under a parent, the placement is first, after a given sibling, or at the end. The mutator turns
    that into a fractional index. If the sibling is gone, place the entry at the end. This also
    puts position computation inside the write transaction, as the database engineer suggested.
  - Ids go into the create actions.
- **What's left for Rust-side helpers:** default start times, filling in the actor, and returning
  created ids and a version. These are thin conveniences. The "intent enum" in the open questions is
  `Action`.
- **Log and versions** (for RFD 2):
  - Store both the action and its deltas. Stamp each entry with a payload format version, which
    readers branch on, and the app version, which is context only.
  - Never rewrite the log. Migrate the database right away, and record each migration as a log entry,
    so replay applies the old deltas, runs the migration, and continues.
  - Keep migrations SQL-only. Decode old payloads as JSON, so no old Rust types accumulate.
- **Old clients:** force an update on a format change. Pending local actions survive the update and
  replay through the new mutators. Their deltas are regenerated.
- **Priority:** after dogfooding, except settle the action shapes before the log stores real data.

## Can expand on
- Prior art: event sourcing commands and events, upcasting, Cambria lenses, Datomic.
- The connection between operational transformation, upcasting and rebase.
- Failures and typed errors back to Swift (not discussed yet).
