# Durability and Dogfooding

Status: direction and options, not a design. Started 2026-10-02.

## Goal

Live in GV: log real training in it and tune it to how I actually train. Today training is
logged in Obsidian. GV is an Xcode dev build whose database gets wiped whenever features change,
and there's no way to evolve the schema or data.

## Principle (lean)

During development, durability comes from snapshot + restore, not sync.

The alternative is treating sync as the durability layer. The downside is that sync bugs then
become data corruption, copied to every replica.

## Snapshot A: full fidelity

- Fully encoded and machine-readable; must round-trip.
- Versioned, with vN → vN+1 upgrade steps for model changes.
- Restore validates (domain validation + invariants) before replacing anything.

## Snapshot B: human readable

- The one that gives the most peace of mind: readable text files even if the code is forgotten
  or broken.
- Does not round-trip. Example: fractional indices. The text only keeps order, so re-importing
  creates new indices. Nested sequences and sets may also render ambiguously.
- Weaker property that is testable: Readable → GV → Readable.
- Format design is set aside for now.

## Builds and the dev loop

- Two apps with separate bundle IDs, so their data is separate:
  - **Dev**: wipeable; dummy data, or a copy of real data restored from Snapshot A.
  - **Real**: holds my data; never wipes.
- The real build is distributed via TestFlight internal testing (paid account, no App Review,
  builds expire after 90 days, data persists across updates).
- Rolling snapshot history stored outside the app container (deleting the app deletes its
  database).
- Optional: a local mutation log as an audit trail, not used for restore.

## Sync (later)

Candidates:

- Convex. See [convex-evaluation.md](./convex-evaluation.md).
- Custom sync on Postgres.
- Custom sync on multitenant SQLite or Turso (the Rust rewrite of SQLite).

## Rejected

- Obsidian as the source of truth, with GV as a read-only copy built by import. Writes would
  never happen in GV, so it doesn't amount to living in GV.

## Open questions

- Snapshot frequency.
- Snapshot location (iCloud Drive app folder vs. Files).
- Readable format design.
