# RFD 0002 comment: testability

- Date: 2026-10-10
- Lens: can it be property-tested or covered by deterministic simulation?
- Read: RFD as of b203b8d; transcripts 1–3; `docs/properties.md`; `generation/`;
  `integration-tests/tests/*`; `client/tests`; `gv-sql/tests`; `gv-sql/sqlite/migrations`; no
  `.github/` (no CI exists yet)

## Verdict
The CI property is very testable, and most of the parts already exist. But fixtures have to be
frozen at release time. Current code can't generate old-version data later.

## Concerns
- Re: Open questions (fixture source): `Action::arbitrary` only emits head-shaped data. Each
  release must write its own fixture (seeded sim → `vN.sqlite`, checked in). You can't
  backfill one.
- Re: Requirements 5: "reads" needs a definition you can test. `SnapshotAll` must parse every
  row (the first property in `properties.md`), and whole-DB invariants must hold. Today the
  invariants are mostly prose. The only checker is `Model::all_activities_have_one_template_root`.
- Re: Current state: the simulation (`postgres_tests.rs`) and the determinism test run on
  Postgres only. The archive is SQLite. `SqliteClient::from_pool` already accepts `SimIo`, so
  porting is cheap.
- Re: Version mechanism: sqlx checksums old migrations, so editing one in place breaks startup
  on real data. Nothing tests that migrations are append-only.

## Missing
- Whether a migrated DB stays *usable*, not just readable.
- A CI host: `gv-sql`'s postgres feature needs a live DB to compile.
- Hand-written edge-case fixtures alongside the generated ones.

## Recommendation
Now: add a SQLite sim with a fixed seed and a `check_invariants(&Snapshot)`. At each tag, freeze
`vN.sqlite` plus a JSON dump of its snapshot. The CI test would migrate every fixture to head,
parse it, check the invariants, and diff the result against the dump (allowing for deliberate
transforms). Then `load_snapshot` and run about 200 simulated actions with model-vs-DB checks.
Later: data transforms inside columns, and backward round-trips. Read "read any version" as
"migrate to head". Testing the original release means checking out old tags in CI, which costs
more than it's worth.

## Can expand on
- Fixture layout, size, and determinism of regeneration.
- Turning the `properties.md` list into the invariant checker.
- Property tests for column-level transforms (attribute value shapes).
- Making CI work without a live Postgres.
