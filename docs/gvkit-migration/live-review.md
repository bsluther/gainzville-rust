# LiveStore correctness review: `spike/gvkit-query-hooks` @ f130866

Scope: `client/live.rs` (primary), `client/query_store.rs`, `client/client.rs`, `client/queries.rs`,
`gv-ffi/src/core.rs`, `swift-app/Gainzville/GvKit/{GvClient,GvQuery}.swift`, measured against RFD 0003
"Latest / lean" principles 3 and 4.

The probes ran in a detached worktree at f130866, using `cargo test -p gv-client --lib live::`. All 11
tests passed, 3 runs in a row: 7 existing tests and 4 probes, plus 1 informational timing probe. The probe
code is at the end of this file. The full diff is at
`docs/gvkit-migration/live-review-probes.diff`.

**Key environmental fact.** The app's SQLite runs in **rollback-journal (DELETE) mode**, not WAL.
sqlx 0.8 does not set `journal_mode` (`sqlx-sqlite-0.8.6/src/options/mod.rs:177-181`), no migration sets
it, and the workspace has always been on sqlx 0.8. `makeCore` passes a bare `sqlite://path`. Probe 2
asserts `PRAGMA journal_mode == "delete"` under those defaults. This decides which of findings 1 and 2 is
live today: **#1 is live now, and #2 becomes live as soon as #1 is fixed the obvious way (by turning on
WAL).** Fix them together.

| # | Severity | Status | One line |
|---|----------|--------|----------|
| 1 | High | CONFIRMED | A build's read transaction blocks every writer's commit for the whole build. A build longer than 5 s turns a write into "database is locked", and Swift drops that error with `try?`. |
| 2 | High (once WAL is on) | CONFIRMED (under WAL) | A SQL-backed first value read during a build is followed by a newer-version batch carrying an older value. The hook regresses: new, then old, then new. |
| 3 | Medium | CONFIRMED | `sync()` has no failure path. A failed build makes it wait for the next data change, so it hangs if the failure persists. |
| 4 | Medium | PLAUSIBLE | `subscribe` returns versions ahead of the batches Swift has applied, so one screen can render two versions at once. This breaks principle 3. |
| 5 | Medium-Low | CONFIRMED (window) | A `read` of a forest-backed question right after `run_action` is stale. `createRootEntry` twice puts both roots at the same suggested time. |
| 6 | Medium-Low | PLAUSIBLE | A panic inside a build kills the refresh task for good: every hook freezes and `building` stays true. |
| 7 | Low | PLAUSIBLE | The `subscribe` retry loop is unbounded and can livelock the main thread under sustained writes. |
| 8 | Low | PLAUSIBLE | A Swift subscribe error is sticky: the hook shows its default until its query changes. |
| 9 | Low | PLAUSIBLE | There is a single sink. A second `set_live_listener`/`GvClient` orphans existing hooks forever, and batches built with no sink still advance the stored values. |
| 10 | Low | PLAUSIBLE | Swift applies each batch through its own `Task { @MainActor }`, with no formal FIFO guarantee. Gating keeps each hook correct, but frames can mix versions. |
| 11 | Info (test infra) | CONFIRMED | `pause_build_seeing` never fires in a build that answers zero subscriptions, so a test that awaits `reached` hangs forever. |

Counts: High 2, Medium 2, Medium-Low 2, Low 4, Info 1.

---

## 1. A build's read transaction blocks writers (DELETE journal mode). High. CONFIRMED.

**Scenario**
1. A write W1 commits, and the refresh loop starts build N+1. `pool.begin()` sends a deferred `BEGIN`.
   The `AllEntries` read then takes a SHARED lock, which is held until `tx.rollback()` after `swap_in`.
   The build answers every subscription in that transaction.
2. The user does something else, and Swift calls `core.runAction` → `RUNTIME.block_on(run_action)` **on
   the main thread**. The mutator reads under SHARED and writes under RESERVED. `COMMIT` then needs
   EXCLUSIVE, so it spins in SQLite's busy handler until the build ends.
3. If the build takes longer than sqlx's `busy_timeout` (5 s), the commit fails with
   `(code: 5) database is locked`. Probe 2 holds a build open: the write fails after 5.01 s.

**Impact**
- Every write that overlaps a build blocks the main thread for the rest of that build. Builds grow with
  entry count (a full `AllEntries` plus `Forest::from`) and with the number of subscriptions (each
  answered serially; `FindEntryJoinById` runs several queries). Every visible `EntryView` adds one.
  Builds are triggered by writes, so back-to-back writes pay this in sequence: drag reorders, sets
  duplication, the future LLM import, the dev seeders.
- Swift calls writes as `try? core.runAction(...)` (Core.swift), so a "database is locked" failure
  **silently drops the user's action**.
- It also stalls `subscribe` in `update()` (principle 4: no Rust calls while rendering). A
  background writer (the ticker now, sync later) holding PENDING blocks new readers, so the main
  thread's subscribe read waits behind it and the build.
- Informational timing (probe 2b; debug build, 1500 entries): the mean write took 39 ms with 0
  subscriptions and 78 ms with 150 `FindEntryJoinById` subscriptions. One build took about 120 ms and
  170 ms respectively. This doesn't fully separate build contention from write cost, but the extra
  latency grows with subscription count. Builds also run when nothing is subscribed: every change
  rebuilds the forest.

**Fix**
- Open the pool in WAL with `synchronous = NORMAL`:
  `SqliteConnectOptions::from_str(path)?.journal_mode(Wal).synchronous(Normal)`. With WAL, readers never
  block the writer and the build's snapshot is real MVCC. **This makes finding 2 live, so ship #2's fix
  together with it.**
- Optionally, skip the build (or only rebuild the forest) when `subs` is empty, and let `read` load the
  forest lazily.

## 2. A SQL-backed first value regresses at the next version. High once WAL is on. CONFIRMED under WAL.

**Scenario** (probe 1)
1. Build B for version N+1 opens its read transaction at snapshot S, which contains A but not W.
2. W commits. Under WAL it doesn't block. W's change notification sits in the broadcast receiver.
3. A hook subscribes to a SQL-backed query Q, here `FindEntryById(W)`. `subscribe` reads it **fresh**
   on its own connection, outside B's snapshot, and gets `Some(W)`. `st.version == N` still holds, so it
   inserts `Sub { value: Some(W) }` and returns `{version: N, value: Some(W)}`.
4. B loops, finds Q missing, answers it **from snapshot S**: `None`. `swap_in` sees `None != Some(W)`
   and publishes batch **N+1 carrying `None`**.
5. Swift: `N+1 > N`, so `receive` accepts the update. The hook goes from `Some(W)` to `None`. Batch
   N+2 (W's own build) restores `Some(W)`.

The same thing happens to **forest-backed** queries at startup. If `st.forest` is `None`,
`latest_forest()` loads a fresh forest and stores it as version 0. A build already in flight with an
older snapshot then swaps its older forest in as version 1.

**Impact**
- It isn't permanent: W's notification is buffered, so N+2 always follows. It is a visible flicker,
  though: a new row appears, disappears, then reappears, or a just-added attribute blinks. It happens
  exactly when views mount right after a write, which is the common case (for example, a new
  `EntryView` mounting for a just-created entry).
- It violates the doc comment's guarantee ("can't be behind the returned version") in spirit. The
  value at N+1 is behind the value at N.
- `join_existing` callers in the window inherit the regression.

**Fix** (any one of these)
- (a) Track build epochs. Record `building_since` on `State` when a build begins, and stamp each `Sub`
  with the epoch it was inserted in. In `swap_in`, for a sub inserted during this build with a SQL
  first value, keep the first value and don't emit an update. A differing value implies a commit after
  the build's snapshot, so that commit's notification guarantees a follow-up build to reconcile.
- (b) Simpler: if `st.building`, have `subscribe` wait for `published` to pass the current version
  before answering. This costs one build of main-thread latency, which finding 1's fix keeps small.
- (c) Insert the new sub as "pending" (value from the published version) and let the in-flight build
  deliver its value without a SQL fresh read. This conflicts with "answer synchronously" for SQL
  queries, though.

  In all cases, also stop `latest_forest()` from publishing a forest it loaded itself while
  `building` is true. Return the loaded forest without storing it, and let the build's result win.

## 3. `sync()` hangs when a build fails. Medium. CONFIRMED.

**Scenario** (probe 3)
1. `sync()` computes `target = version + 1` (or `+ 2` while building) and wakes the loop.
2. The build fails. In the probe, `entries` is renamed. In production this is a pool acquire timeout,
   `SQLITE_BUSY` from finding 1, I/O, or disk full. `publish_next_version` correctly resets
   `building = false`, but no version is published.
3. `sync()` waits on `published.wait_for(v >= target)` until some later write produces a good build.
   If the failure persists, it waits forever. The probe sees `sync` still pending after 2 s, with
   `building == false`.
   - Variant: a build in flight (target `+2`) fails, then the permit build succeeds and publishes
     `+1`. `sync` still needs another change.

**Impact**
- `sync` is test-only today. The doc proposes calling it from `run_action` for read-your-writes. If
  that happens, any build failure **freezes the main thread** (`block_on`) on the next write.
- Even on success, `sync` returns at `published`, before the batch reaches the sink and Swift.
  "Read-your-writes" would hold for `read`, not for hooks.
- Arithmetic checked: no early return. `building` is set before `begin()` and cleared in the same
  critical section as the version bump.

**Fix**
- Publish failures too: `watch::Sender<(Version, u64 /*attempts*/)>`, or an `Err` generation. Have
  `sync` return `Result` and fail if a build that started after the call failed.
- Add a bounded timeout at the FFI layer.

## 4. One screen can render two versions. Medium. PLAUSIBLE. Breaks principle 3.

**Scenario**
1. LogView (`dayRoots`) and EntryViews (`entryJoin` each) are mounted at v5.
2. Write → Rust publishes v6. Its batch for `dayRoots` and entry Y's join is queued as a
   `Task { @MainActor }`.
3. Before that task runs, SwiftUI updates a view whose declared query changed, or mounts a new
   `EntryView`. Its `update()` → `liveSubscribe` returns `{version: 6, value@v6}`.
4. The next frame renders the new hook at v6 next to every other hook still at v5. When the batch
   applies, the frame converges.
   - It's worse with finding 2: a SQL first value can be N+1-ish while tagged N.

**Impact**: The mix lasts one or a few frames. Exactly what the RFD says never happens ("a screen never
mixes two moments").

**Fix**
- Track `appliedVersion` in `GvClient`. Have the listener push batches into a lock-protected queue
  rather than one `Task` per batch.
- In `GvClient.subscribe`, if `subscribed.version > appliedVersion`, synchronously drain and apply
  queued batches up to that version before returning.
- To make that airtight, Rust must hand batch v to the sink before any `subscribe` can observe version
  v. Hold a publish mutex from `swap_in` through `sink(batch)`, which is cheap since the sink only
  enqueues, and take the same mutex in `subscribe`/`join_existing` before reading `st.version`.
  Alternatively, store `delivered_version` and report that from `subscribe`.

## 5. `read` after `run_action` is stale for forest-backed questions. Medium-Low. CONFIRMED (window).

**Scenario** (probe 4)
1. `createRootEntry` reads `suggestedRootInsertionTime` (forest-backed, from the *published* forest),
   then runs `CreateEntry`. `run_action` returns at commit, and the next version is built
   asynchronously.
2. A second `createRootEntry` on the same past day, before that build publishes, reads the old forest
   and gets the **same** time. The probe pauses the build to make this deterministic: `t1 == t2`.

The same window affects `wouldCreateCycle` drag validation right after a move.

**Impact**
- Human double taps mostly miss the window: builds take milliseconds in release. Programmatic
  sequences hit it every time: LLM import, scripted creation, `ConvertToSets`-then-read flows.
- `read` mixes backings. A forest-backed question answers from the published version; a SQL-backed one
  answers fresh. Two reads issued back to back can disagree.

**Fix**
- Give `read` a `fresh` mode that builds a forest in a read transaction for forest-backed questions.
  Alternatively, have `read` call `sync()` once finding 3 is fixed.
- Or, for insertion-time and cycle checks, answer inside the mutator: an intent like "create root at
  the suggested time" computed in the write transaction.

## 6. A panic in a build kills the refresh loop permanently. Medium-Low. PLAUSIBLE.

**Scenario**
1. A query's `execute` panics inside `build_and_swap`. Candidates: `Forest::children`'s
   `expect("child entries must have a defined fractional index")`, the `assert!` in
   `suggested_root_day_insertion_time` (read-only today), or any future gv-ui computation.
2. The panic unwinds the `tokio::spawn`ed loop. Tokio swallows it into an unobserved `JoinHandle`.
3. `building` stays `true`, no version is ever published again, every hook freezes, and `sync` hangs.
   Nothing logs beyond the panic message.

**Impact**: Today's data shouldn't hit this; the invariants hold. Future sync-merged data or a new
gv-ui computation could. The failure is silent, total and permanent.

**Fix**
- Wrap each `answer` in `AssertUnwindSafe(fut).catch_unwind()` (futures) and map a panic to
  `LiveResult::Failed`.
- Or supervise the loop: respawn it on panic and reset `building`.

## 7. Unbounded retry in `subscribe`. Low. PLAUSIBLE.

**Scenario**: `subscribe` loops while `st.version != seen_version`. If each answer, holding a SQL read,
takes longer than the gap between published versions, the loop never exits. Sustained writes cause
this: the background ticker today, sync replay or LLM import later. The main thread spins in
`block_on`, re-answering each time.

**Fix**: After one retry, insert anyway and mark the sub "needs answer". The in-flight build already
answers subs it finds missing; extend the missing check to subs flagged stale. Or fall back to fix
2(b), which waits for the in-flight build.

## 8. Swift subscribe failure is sticky. Low. PLAUSIBLE.

**Scenario**: `GvQueryBox.start` catches a `liveSubscribe` error (pool acquire timeout, or `SQLITE_BUSY`
from finding 1), sets `storedError`, and keeps `handle == nil`. `bind` returns early while the
declared query is unchanged, so the hook shows its default forever and never retries.

**Fix**: Leave `hasDeclared` false (or set a `needsRetry` flag) on failure so the next `update()`
retries, perhaps with backoff.

## 9. A single sink can orphan hooks. Low. PLAUSIBLE.

**Scenario**
- `set_batch_sink` replaces the sink. A second `GvClient(core:)` (another scene, a re-created
  environment, or a test harness) silently cuts off every hook bound to the first. Those hooks never
  update again, and Rust's dedupe keeps them stale even after the views re-render.
- If a batch is produced while no sink is set, `swap_in` has already advanced `sub.value`, so those
  subscribers diverge permanently.

`GainzvilleApp.init` gets the order right today.

**Fix**
- Make `set_live_listener` fail or `debug_assert` on a second call, or support several sinks keyed by
  listener.
- Have `subscribe` fail (or refuse) while there's no sink.

## 10. Per-batch `Task { @MainActor }` ordering is not guaranteed. Low. PLAUSIBLE.

**Scenario**: Each `onBatch` creates its own unstructured task. Swift does not guarantee FIFO order
across separate tasks; it holds in practice for equal priority. If v7 applies before v6, per-hook
gating keeps every hook's final value correct (I checked every case: stale updates are skipped, and
unchanged-in-v7 hooks still take v6). Between the two applies, though, hooks sit at different
versions.

**Fix**: Use a single serial channel: an `AsyncStream` consumed by one main-actor task, or the
queue-plus-drain from finding 4.

## 11. Test infra: the pause hook needs at least one subscription. Info. CONFIRMED.

`wait_at_build_pause` is called only after the `for query in missing` loop, inside an iteration that
found something missing. A build with zero subscriptions breaks out on the first iteration, so
`reached` never resolves and the test hangs with no timeout. Three of my first probes hung this way.
Fix: call the hook once right after the `AllEntries` read as well, and wrap `reached` in a timeout in
tests.

---

## Checked and fine

- **No `std::Mutex` held across `.await`**: in `build_and_swap`, `subscribe` (`continue` drops the
  guard), and `latest_forest`. The spawned future is `Send`, so the compiler enforces this.
- **Lock ordering**: `state`, `sink` and `build_pause` are never held together. The sink is cloned
  out of its lock and called with no lock held. `LiveSubscription::drop` takes only `state`, so a
  handle released inside `on_batch` can't deadlock.
- **`block_on` from Swift**: every FFI entry point runs on the main thread or another non-runtime
  thread. `on_batch` and `on_data_changed` only hop to the main actor and never re-enter Rust
  synchronously. Re-entering would panic with "runtime within runtime", so keep it that way.
- **Lagged / coalescing**: a notification sent after a commit is buffered while a build runs, and
  `next_change` drains only notifications that arrived before the next build began. That build's
  snapshot postdates those commits. `select!` dropping `next_change` for the `rebuild` branch is
  cancel-safe. **No write can leave the cache stale indefinitely.** (Finding 2 is a transient
  regression, not lost staleness.)
- **"Missed forever" via dedupe**: Rust's stored `sub.value` always equals what every holder of that
  id last accepted. Values arrive either from `subscribe`/`join_existing` at `st.version`, or from
  batches with a strictly greater version, which the gate always accepts. The one way to diverge is
  finding 9 (no sink).
- **Forest-backed first values** are exactly at the returned version: insert happens only if
  `st.version == seen_version`. The swap and the final "nothing missing" check share one critical
  section, so a sub inserted during a build is always answered by that build.
- **Unsubscribe or re-subscribe during a build**: a dropped key is skipped in `swap_in`. If the same
  query is re-subscribed (new id) before the swap, the build's already-computed value for that query is
  still from its own snapshot, so the diff against the new sub's first value is correct. Ids are
  monotonic `u64` and never reused. The `sub.id != self.id` guard in `Drop` is unreachable but
  harmless.
- **Swift routing**: routes are registered on the main actor right after `liveSubscribe`. Batches apply
  on the main actor, so a batch can't run between subscribe and route registration. Batches already
  queued for a shared id carry versions ≤ the joiner's version and are correctly ignored.
- **Re-target**: the old id's route stays but is ignored (`id == subscriptionId`). Re-targeting to a
  query shared with another hook joins the same id; the duplicate route entry gets its second
  delivery gated. Re-targeting to the same query after the last handle dropped gets a fresh id.
- **`sync` arithmetic**: there is no early return. `building` is set before `begin()`, so
  `building == false` means no snapshot predates the call. `notify_one` can leave a stray permit when
  a change woke the loop instead; that costs one wasted build and nothing else.
- **`LiveResult::Failed` equality**: Failed→Failed with the same message is not redelivered.
  Failed→good (even the old good value) is delivered and clears the Swift error. The first value as
  `Failed` shows the default plus the error.
- **SQLite deadlock between the build and a writer** (DELETE mode): the build only reads and already
  holds SHARED, so it progresses and the writer waits. It's latency, not deadlock (finding 1).

---

## Probe test code (appended inside `mod tests` of `client/live.rs`; uses its helpers)

```rust
    async fn file_pool(wal: bool) -> SqlitePool {
        use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
        let path = std::env::temp_dir().join(format!("gv-live-probe-{}.sqlite", Uuid::new_v4()));
        // Same defaults the app gets from `SqlitePoolOptions::connect("sqlite://...")`.
        let mut opts = SqliteConnectOptions::new().filename(&path).create_if_missing(true);
        if wal {
            opts = opts.journal_mode(SqliteJournalMode::Wal);
        }
        let pool = SqlitePoolOptions::new().max_connections(20).connect_with(opts).await.unwrap();
        sqlx::migrate!("../gv-sql/sqlite/migrations").run(&pool).await.unwrap();
        pool
    }

    fn find_entry(entry_id: Uuid) -> ClientQuery {
        ClientQuery::FindEntryById(gv_core::queries::FindEntryById { entry_id })
    }

    /// PROBE 1 (WAL): finding 2.
    #[tokio::test]
    async fn probe_sql_first_value_regresses_under_wal() {
        let pool = file_pool(true).await;
        let (client, mut rx) = client(pool).await;
        let store = client.live();
        store.sync().await;
        // The pause hook only fires in a build that answers at least one subscription.
        let _keep = store.subscribe(day_roots()).await.unwrap();
        let a = entry(None, at(1), false);
        let w = entry(None, at(2), false);
        let (reached, resume) = store.pause_build_seeing(a.id);
        create(&client, &a).await;
        reached.await.unwrap(); // build N+1 holds a snapshot with A, without W
        tokio::time::timeout(std::time::Duration::from_secs(2), create(&client, &w))
            .await
            .expect("WAL write should not block on the build's read tx");

        let before = store.version();
        let sub = store.subscribe(find_entry(w.id)).await.unwrap();
        assert_eq!(sub.version, before);
        assert!(matches!(&sub.value,
            LiveResult::Response(ClientQueryResponse::FindEntryById(Some(e))) if e.id == w.id));
        while rx.try_recv().is_ok() {}

        resume.send(()).unwrap();
        let b1 = next_batch(&mut rx).await;
        assert_eq!(b1.version, before + 1);
        let u1 = b1.updates.iter().find(|u| u.subscription_id == sub.subscription_id).unwrap();
        // REGRESSION: version N+1 > N, value older than the first value.
        assert_eq!(u1.value, LiveResult::Response(ClientQueryResponse::FindEntryById(None)));
        let b2 = next_batch(&mut rx).await;
        assert_eq!(b2.version, before + 2);
        assert!(b2.updates.iter().any(|u| u.subscription_id == sub.subscription_id
            && matches!(&u.value, LiveResult::Response(ClientQueryResponse::FindEntryById(Some(_))))));
    }

    /// PROBE 2 (default journal mode = DELETE): finding 1.
    #[tokio::test]
    async fn probe_build_blocks_writer_in_default_journal_mode() {
        let pool = file_pool(false).await;
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&pool).await.unwrap();
        assert_eq!(mode, "delete");
        let (client, _rx) = client(pool).await;
        let store = client.live();
        store.sync().await;
        let _keep = store.subscribe(day_roots()).await.unwrap();
        let a = entry(None, at(1), false);
        let (reached, resume) = store.pause_build_seeing(a.id);
        create(&client, &a).await;
        reached.await.unwrap();

        let started = std::time::Instant::now();
        let res = client.run_action(CreateEntry::from(entry(None, at(2), false)).into()).await;
        let elapsed = started.elapsed();
        resume.send(()).unwrap();
        let err = res.expect_err("write should fail while the build holds its read tx");
        // Observed: "write failed after 5.013s: ... (code: 5) database is locked"
        assert!(elapsed >= std::time::Duration::from_millis(4500));
        assert!(err.to_string().contains("locked"));
    }

    /// PROBE 3: finding 3.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn probe_sync_hangs_when_build_fails(pool: SqlitePool) {
        let (client, _rx) = client(pool.clone()).await;
        let store = client.live();
        store.sync().await;
        // Let any stray rebuild (sync's notify_one permit) finish first.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        sqlx::query("ALTER TABLE entries RENAME TO entries_gone").execute(&pool).await.unwrap();
        let r = tokio::time::timeout(std::time::Duration::from_secs(2), store.sync()).await;
        assert!(r.is_err(), "sync should still be waiting");
        assert!(!store.inner.state.lock().unwrap().building);
    }

    /// PROBE 4: finding 5.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn probe_read_after_write_is_stale(pool: SqlitePool) {
        let (client, _rx) = client(pool).await;
        let store = client.live();
        store.sync().await;
        let past_day = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        let q = || ClientQuery::SuggestedRootInsertionTime(SuggestedRootInsertionTime { day_start: past_day });
        let t1 = store.read(q()).await.unwrap();
        let mut root = entry(None, at(0), false);
        root.temporal = Temporal::Start { start: past_day + Duration::hours(12) };
        let _keep = store.subscribe(day_roots()).await.unwrap();
        let (reached, resume) = store.pause_build_seeing(root.id);
        create(&client, &root).await; // run_action has returned: the write is committed
        reached.await.unwrap();
        let t2 = store.read(q()).await.unwrap();
        resume.send(()).unwrap();
        assert_eq!(t1, t2, "second root would be placed on top of the first");
        store.sync().await;
        assert_ne!(store.read(q()).await.unwrap(), t1);
    }
```

Probe 2b (informational timing: 1500 entries, 0 vs 150 `FindEntryJoinById` subscriptions, 30
back-to-back writes) is in the diff file.
