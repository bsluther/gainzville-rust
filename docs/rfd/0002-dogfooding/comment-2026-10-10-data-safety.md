# RFD 0002 comment: data-safety

- Date: 2026-10-10
- Lens: what can lose or corrupt data, and how would the author notice and recover?
- Read: RFD 0002 at b203b8d; transcripts 1–3; `docs/attributes-design.md`; `gv-sql/sqlite/migrations`, `gv-sql/rows.rs`, `client/client.rs`, `gv-ffi/src/core.rs`, `swift-app/Gainzville/Core.swift`

## Verdict
The SQLite-file lean is sound, but the riskiest moment is the one the RFD automates: an upgrade migrating the only copy. And today, corruption would look like an empty screen.

## Concerns
- Re: Requirements 4 (Backups): copying `gainzville.sqlite` while the pool is open can capture a torn file or miss `-wal` contents. Use `VACUUM INTO` (consistent, single file, live-safe).
- Re: Requirements 3 (Automatic migration): nothing snapshots before `run_migrations()` in `SqliteClient::init`. A clean-running but semantically wrong migration becomes the truth, and later copy-only backups preserve it.
- Re: Options, version mechanism: attribute values are JSON TEXT (`plan`/`actual`). A serde rename with no migration makes `to_value` fail, and `collect::<Result<…>>` fails the whole query. `read_query` returns `Option` and Swift uses `try?`, so the result is an empty list, indistinguishable from no data. `#[serde(default)]` drift is fully silent.
- Re: Background, Builds: until the bundle IDs are split, an Xcode dev build on the phone can apply migrations that no release knows about. The next release then refuses to start (sqlx reports a missing applied migration), or sees semantics it doesn't expect.

## Missing
- A restore path. "Restore validates before replacing" from the background dropped out of the lean.
- Detection: integrity and invariant checks, and row counts compared against the previous backup.

## Recommendation
For the first milestone: run `VACUUM INTO` before every migration and on a schedule, with the data version and timestamp in each filename. After migrating, decode every row and check forest invariants, and fail loudly rather than showing an empty view. CI fixtures should cover every attribute variant and pass the same full-decode check. Practice one restore before trusting the setup.

## Can expand on
- Edge cases in iCloud ubiquity containers: eviction, the entitlement, and files left behind when the app is deleted.
- Hazards of SQLite table-rebuild migrations inside sqlx transactions with foreign keys on.
- `seed_std_lib` re-creation interacting with migrations.
- Fixture generation via the `generation` crate.
