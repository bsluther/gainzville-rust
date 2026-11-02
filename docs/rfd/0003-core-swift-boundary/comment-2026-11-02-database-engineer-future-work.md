# RFD 0003 comment: database-engineer, cues for future work

- Date: 2026-11-02
- Lens: what established database patterns apply here, given that GV isn't a general database?
- Read: RFD 0003 at 52bce8b; `client/{live,client}.rs`; `core/src/forest.rs`; `gv-sql/sqlite/`; a throwaway release-build probe on a Mac (not on a device; synthetic data; not committed)

## Verdict
None of these changes the architecture, and each can wait. They are recorded as cues, not as a plan.

## Concerns
- **Forest reloads.** At 100k entries a build took 292 ms, of which 265 ms was re-reading `AllEntries`; at 10k, 31 ms. Skip the reload when a mutation has no `Entry` delta, then apply entry deltas with copy-on-write.
- **Query invalidation.** All 34 answers re-ran per write, and 2 changed. Per-query table dependencies would be a cheap start. GRDB's database regions are the fuller reference.
- **Indexes.** SQLite has no secondary indexes, and `Forest` scans a `Vec`. A `parent_id` index took a group delete from 159 ms to 1.6 ms at 100k. My candidates didn't help `RootEntriesInInterval` (the `OR` needs a rewrite) or `DistinctTextValuesForAttribute`.
- **Torn first values.** `subscribe` and `read` run in autocommit, while `FindEntryJoinById` issues two statements. Use a read transaction.
- **Sibling positions.** The mutator accepted two siblings at one position. Consider `UNIQUE (parent_id, frac_index)`, a tie-break for sync, and computing positions inside the write transaction.
- **Durability claim.** `fullfsync` is off, so `client.rs`'s "survives a power cut" isn't guaranteed. On this Mac it took commits from 0.08 ms to 4 ms. Measure on a device.
- **Result order.** Several list queries have no `ORDER BY`, yet changes are detected by `Vec` equality.
- **Failed builds.** They retry only on the next write, and hooks show no error.
- **File metadata.** RFD 2 could use `user_version`, `application_id`, `integrity_check` and `foreign_key_check`.

## Missing
Device measurements and realistic data.

## Recommendation
Pick these up when a symptom or dogfooding calls for them.

## Can expand on
- The probe and its full numbers.
