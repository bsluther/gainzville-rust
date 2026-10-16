# RFD 0002 comment: skeptic

- Date: 2026-10-10
- Lens: argues against the current lean, or against doing this now at all.
- Read: RFD 2 as of b203b8d; transcripts 1–3; RFD 1; `docs/sync.md`, `docs/convex-evaluation.md`;
  `gv-sql/sqlite/migrations`, `client/client.rs`, `core/src/std_lib.rs`,
  `swift-app/Gainzville/Core.swift`.

## Verdict
The lean commits to a permanent, CI-tested migration chain while the model is still moving. Every
schema change made during that churn becomes a transform you maintain forever.

## Concerns
- Re: Shift since 2026-10-02: "the SQLite file round-trips by definition" holds only for the
  release that wrote it. Reading it later needs the very transform chain this RFD hasn't built
  yet. A SQLite file nothing can migrate is "durable but unreadable", which the Objective calls
  worthless.
- Re: Requirements 2 and 5: RFD 1 (plan/actual) is in discussion and may move `plan`/`actual` off
  `attribute_values` onto entries. Sync (`sync.md` mutation log, Convex Approach B's pending log
  and cursor) adds tables and likely ownership changes. Dogfooding first turns each of those into
  a migration with real data at stake, plus a fixture per version in CI.
- Re: Requirements 4: the DB runs in WAL mode (`Core.swift` deletes `-wal`/`-shm` sidecars). A
  plain file copy to iCloud can miss committed rows or be torn. It needs `VACUUM INTO` or the
  backup API.
- Re: Current state: sqlx versions the schema but not `seed_std_lib`, which re-creates items the
  user deleted and ignores definition changes.

## Missing
Why now. Nothing says what dogfooding before RFD 1 and the sync choice buys over dogfooding
after them.

## Recommendation
Don't build versioning or the CI matrix yet. Keep Obsidian as the record. If you want daily use
now, make the human-readable export the durability layer, and recover through the markdown →
Actions import that's already a project goal. That survives any schema change without
transforms. Start the migration chain once RFD 1 is decided.

## Can expand on
- How sqlx's checksum and missing-version checks make TestFlight rollbacks fail at launch.
- Export → import as a property test, compared with per-version fixtures.
