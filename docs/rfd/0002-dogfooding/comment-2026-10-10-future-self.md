# RFD 0002 comment: future-self

- Date: 2026-10-10
- Lens: you in a year, having forgotten the code. Is it reversible, legible, and recoverable?
- Read: RFD 0002 as of b203b8d; all three transcripts; `docs/model.md`, `docs/attributes-design.md`;
  `gv-sql/sqlite/migrations/*`, `gv-sql/columns.rs`, `core/src/models/attribute.rs`,
  `client/client.rs`, `swift-app/Gainzville/Core.swift`

## Verdict
The SQLite-only lean is the right archive, but today the file isn't self-describing. Its meaning
lives in code you'll have forgotten. A few cheap additions make "losing the program is a to-do"
actually true.

## Concerns
- Re: Archive format: `sqlite3` opens the file, but IDs are 16-byte BLOBs and values are serde
  JSON tagged with Rust enum names (`{"Mass":{"Exact":{"unit":"Kilogram",...}}}`). Order is
  `frac_index`, templates share `entries` with logs, and start time may need `end - duration`.
  You'd have to relearn all of that from the code.
- Re: Requirement 1: `_sqlx_migrations` records schema only, not which release wrote the file.
  The Mass `Vec` → single-value change (attributes-design "History") left no trace in the schema.
- Re: Requirement 3: `SqliteClient::init` migrates in place, forward-only, then `seed_std_lib`
  re-creates deleted std items. Once a bad upgrade has run, you can't undo it.
- Re: Requirement 4: the DB runs in WAL mode (the `-wipeDB` code deletes `-wal`/`-shm`). Copying
  only the `.sqlite` file can miss recent writes.

## Missing
- A runbook for "I found a backup file, now what?"
- A way to tell a dev DB from a real one. Both are `gainzville.sqlite` with the same hardcoded
  actor ID.

## Recommendation
- Add a `gv_meta` table with the data version, release tag, and commit.
- Ship a SQL view that flattens entries, activities, and values into a chronological log, with
  a one-page "reading a GV database" doc. Test the view in CI against the fixture DBs.
- Back up with `VACUUM INTO` and name files by date and data version.
- Write that same copy just before any migration runs.
- With these in place, the human-readable export can stay out of the first milestone.

## Can expand on
- A draft schema for the flattening view and the `gv_meta` table
- Which in-column changes need data-version bumps (serde renames, select option strings)
- Making `seed_std_lib` version-aware
- A sketch of the restore runbook
