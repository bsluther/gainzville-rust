# Handoff: migrate the app onto GvKit

You're picking up `spike/gvkit-query-hooks` to do the **full migration** of the Swift app onto the
GvKit live-query architecture. Read all of this, then the two documents it points to, before changing
anything.

## Context

**What this branch is.** It started as a spike of RFD 0003, "The core–Swift boundary". The slice it
built has been reviewed, built on a Mac and runtime-checked by the owner, and the owner has decided to
adopt it and migrate the whole app now. The RFD lives on another branch:

```
git fetch origin rfd-0003-core-swift-boundary
git show origin/rfd-0003-core-swift-boundary:docs/rfd/0003-core-swift-boundary/0003-core-swift-boundary.md
```

In the RFD, read "Latest / lean (2026-10-29)": first principles, what's different, the shape, the
queries section, and the spike section. The comments `comment-2026-10-30-*.md` in the same directory
are useful too.

**The architecture in short.**

- Swift asks domain questions through **GvKit** (`swift-app/Gainzville/GvKit/`), with a `@GvQuery`
  hook and a read-once `read`. It writes through helpers.
- Rust picks the backing for each question (forest or SQLite) in **one `match`**,
  `ClientQuery::execute` in `client/queries.rs`. Swift can't see that choice.
- `LiveStore` (`client/live.rs`) delivers one versioned batch of changed answers per data change.
- Queries are grouped into **flat sets by who can answer them**. `DomainQuery` is for mutators and
  the server; `ClientQuery` is the only set Swift sees. A question can appear in several sets; **no
  set wraps another**.

**Decisions already made** (don't reopen them; raise a concern if one blocks you):

- **Do the migration now.** It's a foundation for building the UI well.
- **Intent-level writes are deferred** to a later phase. Writes stay record-level `run_action` calls,
  wrapped in GvKit write helpers, so the intent phase changes helper bodies rather than call sites.
- **Read-your-writes isn't added now.** Today's path doesn't have it either, and every
  write-then-expect site is eventual by design. See plan §3a.
- **Generalizing the forest into a full in-memory model is deferred**, and so are the join map and
  version-at-commit.
- **Keep the `Query` trait and the hand-written flat sets. No macros.** The owner reviews this code by
  reading it.
- **Hooks are for fetching data.** View models stay only where there's real UI logic, and they hold
  queries through a **public observable box**; see plan §3b. They must be `@Observable`, not
  `ObservableObject`.

## The two documents

- **`docs/gvkit-migration/plan.md`: the plan.** It has the inventory of all 30 Swift consumers, the new
  Rust and GvKit work, both decisions, seven steps S0–S6 with what each deletes, verification per
  step, and sizing. `file:line` refs are against `f130866`, so expect small drift.
- **`docs/gvkit-migration/live-review.md`: a correctness review of `LiveStore`,** with probe tests in
  `live-review-probes.diff`. Its fixes are part of S0; see the next section.

## S0 also includes hardening `LiveStore`

Do this **before** migrating any screen, because every screen will depend on it. Bring the review's
probe tests into the suite as regression tests where they apply.

**Must fix:**

1. **Finding 1 (High): a build's read transaction blocks writers.** SQLite runs in rollback-journal
   mode, because sqlx doesn't set `journal_mode`. Open the pool in WAL with `synchronous = NORMAL`
   (`SqliteConnectOptions::journal_mode(Wal)`) where `SqliteClient::init` builds the pool.
2. **Finding 2 (High once WAL is on): a SQL-backed first value can regress at the next version.**
   **Ship this together with #1.** The review offers three fixes. Prefer (a), build epochs, because
   (b) adds main-thread latency on subscribe. Also stop `latest_forest()` from publishing a forest it
   loaded itself while a build is in flight.
3. **Finding 6 (Medium-Low): a panic in a build kills the refresh loop for good.** Catch unwinds per
   answer and map them to `LiveResult::Failed`, or supervise and restart the loop.

**Should fix, in the same S0 change to `GvClient`:**

4. **Findings 4 and 10: one screen can render two versions.**
   - Deliver batches through a single serial queue rather than one `Task` per batch.
   - Track the applied version in `GvClient`.
   - Have `subscribe` drain queued batches up to the version it returns.
   - Make Rust hand batch v to the sink before any `subscribe` can report version v.

   This is what makes principle 3 ("a screen never mixes two moments") hold.

**Fix if cheap:**

- **Finding 3:** give `sync()` a failure path.
- **Finding 11:** the test hook should fire even when there are no subscriptions.

**Don't fix; record them as known:** findings 5, 7, 8 and 9.

**Owner note.** Switching to WAL changes the on-disk layout: `-wal` and `-shm` sidecar files appear.
Any backup or export that copies the SQLite file (RFD 0002) must use the backup API or
`VACUUM INTO`, not a raw copy. Mention this in your S0 report.

## How to work

- **Ask the owner at the start** whether to pause for a manual app check after every step, or only
  after S0, S5 and S6. Default to pausing after S0, S5 and S6.
- **Order:** S0 first, then S1. S2, S3 and S4 can follow in any order, and S4 must land before S5.
  S6 comes last. If you use parallel subagents, follow the plan's parallelism notes, and regenerate
  bindings rather than hand-merging `gv_ffi.swift`.
- **One commit per step,** or a few logical ones. Each step must build and run on its own, with no
  screen left on two pipelines.
- **Every step:**
  1. Have the Postgres container running (`docker compose up -d`).
  2. Run `cargo test` from the workspace root.
  3. Rebuild the Swift artifacts: `scripts/regen-bindings.sh` if the FFI changed, otherwise
     `scripts/rebuild-xcframework.sh` after any Rust change.
  4. Run `xcodebuild` for both targets from `swift-app/`, as `swift-app/SWIFT-APP.md` describes.
  5. Don't launch the app yourself. List the step's manual checks from plan §5 for the owner.
- **Keep the `BSL:` comments** (the owner's review notes) unless the step deletes that code.
- **Commits:** end each message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
  Push to `spike/gvkit-query-hooks`. Don't open a PR.
- **At the end:** delete this handoff file, and update the docs listed in S6. That includes the RFD's
  spike section, which lives on the `rfd-0003-core-swift-boundary` branch, so ask the owner how they
  want that done.

## Report back after each paused step

- What changed and what was deleted.
- Verification results, with exact commands.
- The manual checks for the owner.
- Anything that diverged from the plan, and why.
