# RFD 0002: Dogfooding

- State: discussion
- Linear: [GV-76](https://linear.app/gainzville/issue/GV-76)

## Latest / lean (2026-10-10)

Summary of three voice notes:
[transcript 1](./source-2026-10-09-transcript-1.md),
[transcript 2](./source-2026-10-10-transcript-2.md),
[transcript 3](./source-2026-10-10-transcript-3.md).

### Objective

Make GV my system of record for training, replacing Obsidian: actually using the data, not just
entering it. That needs two things:

- **Durability plus recoverability.** Trust that the data is safe and that I can always get it
  back. Durable but unreadable is worthless.
- **Painless upgrades.** Moving to a new release keeps my data with no effort, so daily use and
  ongoing development can coexist.

Ergonomics (userland features) also matter but are out of scope here.

### Shift since 2026-10-02

The [background](#background-2026-10-02) proposed two snapshots: A (full-fidelity, encoded) and
B (human-readable). The notes move toward **the SQLite file itself as the full-fidelity
snapshot**:

- It round-trips by definition. There's no new translation boundary where things can go wrong.
- A human-readable form can always be generated later from the data plus a program that reads
  it. Losing that program is a to-do, not data loss.
- A human-readable export (Markdown/YAML in iCloud) still appeals as the ultimate fallback,
  since it mirrors what I have in Obsidian today, but it's now secondary.

### Requirements

1. **Versioned data.** The database records its data version, and the code knows every version.
   Given any historical file, I know which release can read it. Today a three-month-old file
   would mean hunting for the commit that wrote it.
2. **Transforms between versions.** Always forward to the latest, in a principled way. Backward
   would be nice but may be lossy.
3. **Automatic migration on upgrade.** A new release migrates my existing data on its own.
4. **Backups to iCloud.** Copy-only: never overwrite, prune by hand. Weekly or daily. Manual is
   acceptable to start (losing a week beats losing a year), but the lean is to automate.
5. **Releases.** The build on my phone is always a tagged release that satisfies these
   properties, never whatever dev build happened to be installed. CI on main or on release tags
   enforces them: every release reads and migrates every prior data version.

### Breakdown into work

From transcript 3:

- Release process: tags plus a CI gate.
- Data versioning: any version is identifiable and readable.
- Version transforms: tested for every release against every prior version.
- Backup: get the SQLite file from the app into iCloud, manual first, then automated.
- Later, optional: human-readable export.

### Options and scope

Axes still open, with the notes' lean where there is one:

- **Archive format:** SQLite file only (lean) / SQLite plus human-readable export / a custom
  encoded snapshot (background's Snapshot A).
- **Version mechanism:** sqlx schema migrations alone / an explicit data version plus transforms
  that also cover data inside columns (e.g. attribute value shapes) and invariants.
- **Direction:** forward-only (lean, minimum) / bidirectional.
- **Backup trigger:** manual export / scheduled / on launch or on change.
- **Scope of the first milestone:** smallest is tagged TestFlight releases + migrations tested in
  CI + a manual copy to iCloud. Fuller adds automated backups, validated restore, and the
  human-readable export.

### Current state

- The SQLite client runs embedded, forward-only sqlx migrations at startup
  (`gv-sql/sqlite/migrations`, five so far), and sqlx records applied ones in
  `_sqlx_migrations`. So schema versioning partly exists already.
- Not covered: changes to data inside columns, domain invariants, and any test that an old
  database migrates. The dev build's database is wiped when features change.

### Open questions

- Is the SQLite file enough as the long-term archive, or is a format independent of SQLite and
  the schema still wanted?
- What does "read any version" mean: open and migrate to head, or also open with the release
  that wrote it?
- Where do CI's per-version fixture databases come from (generated data, not personal data) and
  where do they live?
- Is the human-readable export in or out of the first milestone?
- Carried over: backup frequency and location; dev vs real build separation.

## Background (2026-10-02)

Originally `docs/durability-and-dogfooding.md`, moved here unchanged apart from heading levels and
link paths.

### Goal

Live in GV: log real training in it and tune it to how I actually train. Today training is
logged in Obsidian. GV is an Xcode dev build whose database gets wiped whenever features change,
and there's no way to evolve the schema or data.

### Principle (lean)

During development, durability comes from snapshot + restore, not sync.

The alternative is treating sync as the durability layer. The downside is that sync bugs then
become data corruption, copied to every replica.

### Snapshot A: full fidelity

- Fully encoded and machine-readable; must round-trip.
- Versioned, with vN → vN+1 upgrade steps for model changes.
- Restore validates (domain validation + invariants) before replacing anything.

### Snapshot B: human readable

- The one that gives the most peace of mind: readable text files even if the code is forgotten
  or broken.
- Does not round-trip. Example: fractional indices. The text only keeps order, so re-importing
  creates new indices. Nested sequences and sets may also render ambiguously.
- Weaker property that is testable: Readable → GV → Readable.
- Format design is set aside for now.

### Builds and the dev loop

- Two apps with separate bundle IDs, so their data is separate:
  - **Dev**: wipeable; dummy data, or a copy of real data restored from Snapshot A.
  - **Real**: holds my data; never wipes.
- The real build is distributed via TestFlight internal testing (paid account, no App Review,
  builds expire after 90 days, data persists across updates).
- Rolling snapshot history stored outside the app container (deleting the app deletes its
  database).
- Optional: a local mutation log as an audit trail, not used for restore.

### Sync (later)

Candidates:

- Convex. See [convex-evaluation.md](../../convex-evaluation.md).
- Custom sync on Postgres.
- Custom sync on multitenant SQLite or Turso (the Rust rewrite of SQLite).

### Rejected

- Obsidian as the source of truth, with GV as a read-only copy built by import. Writes would
  never happen in GV, so it doesn't amount to living in GV.

### Open questions

- Snapshot frequency.
- Snapshot location (iCloud Drive app folder vs. Files).
- Readable format design.
