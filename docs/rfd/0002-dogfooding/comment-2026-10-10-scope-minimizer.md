# RFD 0002 comment: scope-minimizer

- Date: 2026-10-10
- Lens: the smallest version that delivers the goal. What can be cut, deferred, or done by hand?
- Read: RFD 0002 at `b203b8d`; transcripts 1-3; `rfd.md`; `client/client.rs` (`run_migrations`);
  `gv-sql/sqlite/migrations` (5 files); `gv-sql/tests`; `swift-app/Gainzville/Core.swift`
  (`makeCore`); `project.pbxproj` bundle IDs; no `.github/`, no git tags.

## Verdict
Most of the five requirements are already in place or are a checklist item. The first milestone is
about a weekend of work and adds no new versioning machinery.

## Concerns
- Re: Requirements 1-2: `_sqlx_migrations` already records the data version. All five migrations
  are additive (`ADD COLUMN`, `CREATE TABLE`). A separate version-plus-transform framework solves a
  problem you don't have yet. Write the first in-column rewrite as a migration when one is needed.
- Re: Open questions, "read any version": define it as "migrates to head" and drop the other meanings.
  Fixtures start at your first tag. No real data predates it, so no backfill is needed.
- Re: Requirement 5: there is no CI at all, and workspace `cargo test` needs live Postgres. A CI
  gate is its own project. Run the fixture test locally before you tag.

## Missing
- WAL: copying `gainzville.sqlite` alone can silently drop recent rows. Use `VACUUM INTO`.
- A restore path: how does a backup get back into the app?
- Dev and real builds share one bundle ID (`com.gainzville.Gainzville`), so a dev install can wipe real data.

## Recommendation
Milestone 1:
1. Give Debug a `.dev` bundle ID suffix.
2. Add an "Export DB" button: `VACUUM INTO` a dated file, then `.fileExporter` to iCloud Drive.
   That's copy-only by construction.
3. Enable `UIFileSharingEnabled`, so you restore by dropping a file into Documents.
4. At each tag, commit a seeded fixture DB and add one test that migrates every fixture and runs
   every query.
5. Ship tagged builds via Xcode Archive to TestFlight, following a three-line checklist.

Defer automated backup, the human-readable export, validated restore, bidirectional transforms,
and CI.

## Can expand on
- Fixture generation from `devSeedStdLib`/`generation`.
- Postgres-free test target for CI later.
- Automating the export via `BGTaskScheduler`.
- What counts as the trigger for a real data-version layer.
