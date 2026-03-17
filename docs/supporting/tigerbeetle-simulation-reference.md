# TigerBeetle Simulation Reference

Reference for TigerBeetle's simulation-based testing approach and how it applies to GV.
TB repo expected at `~/dev/zig/tigerbeetle` (commit used for line citations: `fe90d6a`).

---

## 1. Overview: TB's Philosophy

TigerBeetle's simulation philosophy can be summarized as **"assertions as tripwires +
simulation as the path-exerciser"**.

- **Dense `assert()` in production code**: not test-only guards, but permanent tripwires that
  fire the moment an invariant is violated anywhere in any code path — including paths only
  reachable under fault injection or concurrent replica behavior.
- **Simulation exercises the paths**: the simulator's job is to drive the system into every
  interesting corner (faults, restarts, clock skew, message loss) so the tripwires have
  a chance to fire. The assertions do the actual checking.
- **Reference model for convergence**: a parallel in-memory `StateChecker` tracks what the
  cluster *should* have committed and verifies each replica against it on every tick.

### Contrast with Turso

| Dimension | TigerBeetle | Turso |
|-----------|-------------|-------|
| Primary check | Dense `assert()` in prod code | `Shadow` trait — in-memory model |
| Fault injection | PacketSimulator + ClusterFaultAtlas | Scheduler intercepts + error injection |
| Convergence check | StateChecker (commit log comparison) | Property verification after each op |
| Swarm testing | `random_enum_weights()` per seed | Not present |
| Coverage tracking | `marks.zig` log-based tracker | Not present |

### Why GV can use both

TB and Turso are complementary, not competing:

- **Turso-style Shadow / model checking** is directly applicable to GV *today*: maintain an
  in-memory `ModelChecker` that receives the same mutations as the DB, then verify domain
  invariants (forest structure, FractionalIndex uniqueness, attribute type consistency) after
  each action.
- **TB-style assertions** are also applicable today: plant tripwires in `mutators.rs` that
  verify pre/post conditions at write time; they fire in integration tests and sim runs.
- **TB-style fault injection and time simulation** become relevant when GV implements
  offline-first sync — testing rebase ordering, HLC correctness, and mid-sync disconnects
  requires the infrastructure TB built for distributed faults.

---

## 2. PRNG & Deterministic Reproducibility

**Files**: `src/stdx/prng.zig`, `src/testing/fuzz.zig`

### `PRNG` struct — `src/stdx/prng.zig`

TigerBeetle wraps Xoshiro256++ in a struct with ergonomic helpers:

```zig
pub const PRNG = struct {
    random: std.rand.Random,
    xoshiro: std.rand.Xoshiro256,

    pub fn init(seed: u64) PRNG { ... }
    pub fn chance(prng: *PRNG, r: Ratio) bool { ... }  // line 404
    pub fn range(prng: *PRNG, T: type, min: T, max: T) T { ... }
    pub fn shuffle(prng: *PRNG, T: type, buf: []T) void { ... }
};
```

**`Ratio` struct** (lines 31–83): a `p/q` rational used for probabilistic decisions.

```zig
pub const Ratio = struct {
    p: u64,  // numerator
    q: u64,  // denominator
    pub fn init(p: u64, q: u64) Ratio { ... }
};
```

`chance(prng, Ratio.init(1, 100))` fires ~1% of the time. This makes fault rates
human-readable in configuration structs rather than raw float literals.

**Single seed → all randomness**: every PRNG in the simulation is derived from a single
`u64` seed. Replaying the seed exactly reproduces the entire run — faults, client request
timing, and message delivery order.

### `parse_seed()` — `src/testing/fuzz.zig` lines 90–108

```zig
pub fn parse_seed(bytes: []const u8) u64 {
    // Accepts a git commit hash (hex string) as seed.
    // First 8 bytes of the hash are interpreted as a little-endian u64.
    // Falls back to parsing as a decimal integer.
    ...
}
```

This means you can run `./simulator --seed $(git rev-parse HEAD)` and the exact run is
reproducible from the commit hash. CI can record the seed in the failure output; a developer
can reproduce locally by passing the same seed.

**GV application**: `generation/src/lib.rs` already uses a seeded `ChaCha8Rng`. Adopt the
`parse_seed` pattern:
- Accept the seed as a CLI arg or env var in the sim binary
- Accept a git commit hash (hex) and derive `u64` from first 8 bytes
- Print the seed at the start of every run so CI failures are reproducible

---

## 3. Swarm Testing

**Files**: `src/testing/fuzz.zig`, `src/state_machine_fuzz.zig`, `src/stdx/prng.zig`

Swarm testing deliberately biases the distribution of generated operations *per run* so that
each run explores a different narrow slice of the space rather than every run averaging out
to the same distribution.

### `random_enum_weights()` — `src/testing/fuzz.zig` lines 34–55

```zig
pub fn random_enum_weights(prng: *PRNG, Enum: type) [@typeInfo(Enum).Enum.fields.len]u64 {
    // Assigns random weights to each variant.
    // With some probability, sets a weight to 0 (disables the variant entirely for this run).
    // Remaining weights are drawn from a wide range (e.g., 1..1000).
}
```

Effect: one run might generate 90% `CreateEntry` and 0% `DeleteEntry`; another might
generate 50% `MoveEntry`. Bugs that only appear when a particular action dominates are
found faster than with uniform weighting.

### `int_edge_biased()` — `src/state_machine_fuzz.zig` lines 17–37

```zig
fn int_edge_biased(prng: *PRNG, comptime T: type) T {
    // With 50% probability, returns a value near a power-of-2 boundary.
    // Otherwise returns a uniform random value.
    // Targets off-by-one errors at bit boundaries.
}
```

Analogous: for GV attribute values (e.g., rep counts, weights), bias toward 0, 1, boundary
values, and very large values rather than random mid-range values.

### `Combination` struct — `src/stdx/prng.zig` lines 540–574

```zig
pub fn Combination(n: comptime_int, k: comptime_int) type {
    // Uniformly samples k-element subsets of {0, ..., n-1} without repetition.
    // Used to select which replicas to fault in a given tick.
}
```

For GV sync testing: uniformly select which subset of pending operations to drop/delay.

**GV application**: add a `SimulationMode` enum to the sim binary:

```rust
pub enum SimulationMode {
    /// All action variants equally weighted.
    Uniform,
    /// Per-run random weights drawn from seed; some variants may be disabled entirely.
    Swarm,
}
```

On each sim run in `Swarm` mode, derive per-variant weights from the seed before generating
any actions. Record the weights in the run header so failures are reproducible.

---

## 4. Assertions & Coverage Marks

### TB's assertion philosophy

TigerBeetle places `assert()` (which panics in all builds, including release) throughout
production code — not as defensive checks, but as invariant tripwires. The philosophy:

> "An assertion that never fires in testing is not doing its job. The simulator's role is
> to exercise enough paths that every assertion fires at least once under some scenario."

This means assertions are co-designed with the simulator: when you write an assert, you
also think about what simulation scenario would trigger it.

### `marks.zig` — lines 44–127

A log-based coverage tracker used in tests to verify that specific code paths were reached.

```zig
// Usage pattern:
var mark = marks.check("label");         // before the code block
// ... code that should be reached ...
mark.expect_hit();     // panics if the block was never entered during the test
mark.expect_not_hit(); // panics if the block was entered (negative assertion)
```

Implementation: `marks.check("label")` emits a log line with a known format string. The
test framework scans the log buffer after the run. If a mark's format string appears in the
log, the mark is "hit". `expect_hit()` fails if the log scan finds no match.

Characteristics:
- **Not antithesis-style** (no bytecode instrumentation): purely log-based
- **Test-only**: uses a global (single-threaded) registry; not safe for production
- **String-matching**: the mark name must match the exact log format string used in the
  production code path

### Proposed GV equivalent

Add `core/src/assert.rs` with three macros:

```rust
/// Invariant that must always hold. Panics in test/sim; logs error in production.
/// Use for: pre/postconditions in mutators, structural invariants.
macro_rules! assert_always {
    ($cond:expr, $msg:literal) => {
        if !$cond {
            #[cfg(test)]
            panic!("assert_always failed: {}", $msg);
            #[cfg(not(test))]
            tracing::error!("assert_always failed: {}", $msg);
        }
    };
}

/// Dead code path that should never be reached. Same behavior as assert_always!(false).
macro_rules! assert_unreachable {
    ($msg:literal) => { assert_always!(false, $msg) };
}

/// Coverage mark: records that this path was reached during a sim run.
/// Sim harness verifies all registered labels were hit at least once.
macro_rules! assert_sometimes {
    ($label:literal) => {
        #[cfg(test)]
        SimulationCoverage::record($label);
    };
}
```

### Specific tripwire candidates in `core/src/mutators.rs`

| Mutator | Tripwire |
|---------|----------|
| `create_entry` | If `position` has a `parent_id`, assert that parent entry exists in DB |
| `move_entry` | Assert target parent has `is_sequence = true` |
| `create_value` | Assert attribute type tag matches value variant (e.g., `AttributeType::Reps` → `Value::Integer`) |
| `FractionalIndex` construction | Assert terminator byte (0x00) is present |
| Any update mutator | Assert `old` state read before write matches what was fetched from DB |

---

## 5. State Checker / Reference Model

**File**: `src/testing/cluster/state_checker.zig`

### TB's `StateChecker` (lines 27–331)

Maintains the canonical commit history and verifies every replica against it on each tick.

```zig
pub const StateChecker = struct {
    commits: std.ArrayList(Commit),  // canonical log of all committed ops

    pub fn check_state(checker: *StateChecker, replica: *const Replica) !void {
        // line 166: called on every replica tick
        // Verifies:
        //   - commit op monotonicity (line 217): replica.commit_min never decreases
        //   - parent checksum chain (lines 243-248): each commit's parent_checksum
        //     matches the previous commit's checksum
        //   - client request inflight check (lines 250-281): no client has two
        //     in-flight requests with the same client ID
        //   - view number monotonicity (line 184): replica.view never decreases
    }
};
```

Convergence detection: `replica_test.zig` line 2283 queries
`state_checker.commits.items.len` to detect that new commits are being made. The `run()`
loop (line 2266) runs up to 4100 ticks and resets its progress counter whenever commits
advance.

### Turso analog

Turso's `Shadow` trait: an in-memory model that applies the same mutations as the real DB.
After each operation, queries are issued against both the real DB and the shadow model; the
results must match. The shadow never reads from the DB — it is the authoritative model.

### GV application: `ModelChecker`

For GV, a `ModelChecker` would shadow DB writes and verify domain invariants after each
action. Unlike TB's commit-log approach, GV's invariants are structural (forest correctness)
rather than consensus-based.

**Invariants to verify**:

| Invariant | Check |
|-----------|-------|
| Forest structure | Every `entry.parent_id` references an existing entry in the same actor's forest |
| FractionalIndex uniqueness | Within same parent, all children have distinct FractionalIndex values |
| FractionalIndex ordering | Children's FractionalIndex values are sorted ascending by their sequence order |
| Set homogeneity | Entries with `display_as_sets = true` have children all sharing the same `activity_id` |
| Attribute type consistency | For each `Value` row, the `AttributeConfig` variant matches the value's type tag |
| Aggregate consistency | Entry count per actor matches `COUNT(*)` from DB (catches phantom inserts/deletes) |

**Implementation approach**:

```rust
pub struct ModelChecker {
    entries: HashMap<Uuid, Entry>,
    activities: HashMap<Uuid, Activity>,
    // ...
}

impl ModelChecker {
    /// Apply a mutation to the in-memory model.
    pub fn apply(&mut self, mutation: &Mutation) { ... }

    /// Verify all invariants. Called after each action in sim/integration tests.
    pub fn check_invariants(&self) -> Result<(), Vec<InvariantViolation>> { ... }
}
```

---

## 6. Simulated Time

**File**: `src/testing/time.zig`

### `TimeSim` struct (lines 12–98)

Each replica gets its own `TimeSim` instance, injectable via a `Time` vtable (line 39).

```zig
pub const OffsetType = enum {       // lines 5-10
    linear,    // steady clock drift (constant rate offset)
    periodic,  // sinusoidal NTP-like correction
    step,      // single instantaneous jump
    non_ideal, // periodic + random jitter
};

pub const TimeSim = struct {
    offset_type: OffsetType,
    offset_ms: i64,  // current offset from "true" time

    pub fn monotonic(ts: *TimeSim) u64 { ... }  // always increases
    pub fn realtime(ts: *TimeSim) i64  { ... }  // applies offset; can go backward
};
```

`monotonic()` and `realtime()` are separate because monotonic clocks (used for timeouts)
must never go backward, but wall-clock time (used for timestamps visible to clients) can
jump under NTP corrections.

### GV applicability — now

Not needed for current single-node GV. All timestamps are generated locally with no
distributed clock skew to simulate.

### GV applicability — for sync

When GV implements offline-first sync, clients will generate timestamps while disconnected.
Those timestamps will have drifted relative to each other and to the server. `TimeSim`-like
injection would enable testing:

- **Entries created with clocks that drifted apart**: do rebase ordering rules handle
  timestamps that are close but out of order?
- **HLC (Hybrid Logical Clock) correctness under clock jumps**: does the HLC advance
  correctly when a client reconnects after a large step forward or backward?
- **Rebase of concurrent offline writes**: if two clients each make 10 writes while
  disconnected and reconnect simultaneously, does the merge produce a consistent forest?

**Recommendation**: add a `Clock` trait to `core/` now, before sync is implemented.

```rust
pub trait Clock: Send + Sync {
    fn now_utc(&self) -> DateTime<Utc>;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now_utc(&self) -> DateTime<Utc> { Utc::now() }
}
```

Mutators that generate timestamps accept `&dyn Clock`. Tests and simulation inject a
`FakeClock`. Adding this later would require changing every mutator signature.

---

## 7. Fault Injection

### Network — `src/testing/packet_simulator.zig`

**`PacketSimulatorOptions`** (lines 13–50):

```zig
pub const PacketSimulatorOptions = struct {
    node_count: u8,
    client_count: u8,
    seed: u64,
    one_way_delay_mean_ms: u64,    // latency distribution mean
    one_way_delay_min_ms: u64,
    packet_loss_probability: Ratio,   // e.g. Ratio.init(1, 100) = 1%
    packet_replay_probability: Ratio, // probability a delivered packet is also re-delivered
    partition_probability: Ratio,     // probability a partition starts each tick
    unpartition_probability: Ratio,   // probability an existing partition ends
    partition_symmetry: PartitionSymmetry,
};
```

**`PartitionMode` enum** (lines 64–78):

```zig
pub const PartitionMode = enum {
    none,               // no partitions
    uniform_size,       // random subset of nodes isolated
    uniform_partition,  // random split into two groups
    isolate_single,     // one node isolated from all others
};
```

Asymmetric partitions (`PartitionSymmetry.asymmetric`): node A can send to B, but B cannot
send to A. Critical for testing split-brain edge cases.

Packet replay: a delivered packet may be re-delivered one or more times. Tests that
operations are idempotent and that duplicate detection works correctly.

**GV sync applicability**:
- Simulated HTTP connection loss between offline client and server
- Dropped or replayed sync payloads (tests idempotency of the sync endpoint)
- Client-side outbox queue drain testing (does the client retry correctly after loss?)

### Storage — `src/testing/storage.zig`, `ClusterFaultAtlas` (lines 989–1193)

Quorum-aware fault distribution: the atlas ensures that exactly `quorums.replication - 1`
replicas have some form of storage corruption in any given zone, but at least one replica
always has valid data. This prevents the simulation from trivially making the cluster
unrecoverable while still exercising fault-recovery paths.

Per-zone fault tracking:
- WAL headers (write-ahead log header corruption)
- WAL prepares (individual op record corruption)
- Client replies (cached reply corruption)
- Grid blocks (data block corruption)

**GV sync applicability**:
- SQLite WAL corruption on the client device
- Offline write queue (outbox) corruption before sync
- Partial write to the client DB during a sync transaction

### Replica lifecycle — `src/testing/cluster.zig`

```zig
fn replica_crash(cluster: *Cluster, replica_index: u8)   // line 684
fn replica_restart(cluster: *Cluster, replica_index: u8) // line 658
fn replica_pause(cluster: *Cluster, replica_index: u8)   // line 645
fn replica_unpause(cluster: *Cluster, replica_index: u8) // line 651
fn replica_reformat(cluster: *Cluster, replica_index: u8)// line 860 — full data loss
```

- **crash + restart**: storage is closed and re-opened; replica re-initializes from
  persisted state. Tests recovery from app crash mid-operation.
- **pause + unpause**: simulates VM migration or process suspension. The replica
  accumulates a message backlog while paused.
- **reformat**: complete data loss. Replica must re-sync from scratch.

**GV sync applicability**:

| TB scenario | GV analog |
|-------------|-----------|
| `replica_reformat` | Client device wipe — SQLite DB deleted, client must re-sync from server |
| `replica_crash` + restart | App killed mid-sync — local DB in partial state |
| `replica_pause` + unpause | Device offline for extended period, then reconnects |

---

## 8. Cluster Tick Loop & Convergence

**File**: `src/testing/cluster.zig`

### `cluster.tick()` — line 536

```zig
pub fn tick(cluster: *Cluster) void {
    // Inner loop (lines 540-564): step network + storage until no more progress
    while (network.step() or storage.step()) {}

    // Outer tick:
    network.tick();                        // advance packet delays by 1ms
    for (clients) |*client| client.tick(); // drive client state machines
    for (replicas) |*replica| {
        replica.tick();                    // drive replica state machines
        if (prng.chance(Ratio.init(1, 100))) {
            journal_checker.check(replica); // 1% stochastic journal check (line 605)
        }
    }
}
```

The interleaved model (network + storage steps until quiescent, then one tick of all
actors) avoids arbitrary ordering artifacts while keeping the simulation deterministic.

### Convergence detection — `replica_test.zig` line 2283

```zig
const commits_before = state_checker.commits.items.len;
cluster.tick();
const commits_after = state_checker.commits.items.len;
const progressed = commits_after > commits_before;
```

The `run()` loop (line 2266) allows up to 4100 ticks. The tick counter resets to 0
whenever new commits are observed. This gives the cluster time to recover from faults
without imposing a hard wall-clock limit.

**GV applicability**: not applicable at current single-node scale. Relevant when GV has
multi-client sync where multiple clients need to converge to the same server state after
concurrent offline writes.

---

## 9. VSR Sync: Catching Up a Lagging Replica

Relevant as a structural analog for GV's offline-client sync flow.

**File**: `src/vsr/sync.zig`, `src/vsr/replica.zig`, `src/vsr/client_sessions.zig`

### Sync stages — `src/vsr/sync.zig` lines 5–29

```zig
pub const Stage = union(enum) {
    idle,
    canceling_commit,     // wait for in-progress commit to finish
    canceling_grid,       // wait for in-progress grid I/O to finish
    updating_checkpoint,  // apply the new checkpoint + replay op range
};
```

The stage machine ensures that a sync never starts while a commit or grid I/O is in
progress — equivalent to ensuring GV's sync doesn't start while a local write transaction
is open.

### `sync_start_from_committing()` — `src/vsr/replica.zig` line 10322

Triggered when a replica detects it has fallen behind (its `commit_min` is below the
cluster's checkpoint). Steps:

1. Cancel any in-progress commit
2. Reset `state_machine`, `client_sessions`, `free_set`
3. Request the checkpoint from an up-to-date replica
4. Replay ops in range `[sync_op_min, sync_op_max)` on top of the checkpoint

### `ClientSessions` — `src/vsr/client_sessions.zig` lines 15–300

Tracks which client requests have been committed, for deduplication. Each entry is a
`(client_id, session_id, request_number)` tuple. Evicts the oldest-committed client
when the table is full.

**GV sync analog**: when an offline client reconnects:

1. **Detect divergence point**: TB uses `checkpoint_op` + op range; GV will use a global
   sequence number or HLC watermark to identify which server operations the client has
   not yet seen.
2. **Cancel in-progress local writes**: if the client is mid-transaction, roll back before
   sync begins.
3. **Apply server state up to divergence point**: download and apply the server's log of
   operations since the client's last sync sequence number.
4. **Rebase local writes on top**: replay the client's offline writes against the updated
   server state. Conflict resolution policy TBD (see `docs/sync.md`).

**Testing this requires**:
- Fault injection for mid-sync disconnect (PacketSimulator analog)
- Simulated clock drift (TimeSim analog) to test HLC rebase ordering
- `replica_crash` analog to test app-kill during step 3 or 4

---

## 10. Open Questions

1. **`assert_sometimes` semantics**: should the sim fail immediately when it ends without
   hitting a labeled path (like TB's `expect_hit`)? Or accumulate all uncovered labels and
   report a summary at the end? TB fails immediately per mark; the summary approach gives
   better diagnostics when many marks are missed at once.

2. **`assert_always` in production**: use `tracing::error!` (silent, observable via
   logging) or `tracing::warn!`? Or a `cfg(feature = "strict_assertions")` feature flag?
   Recommendation: `tracing::error!` always + `#[cfg(test)] panic!` — no feature flag
   needed since we want errors surfaced in both cases.

3. **`SimulationCoverage` registry**: thread-local or global static? What is the GV sim
   test harness entry point that resets the registry before a run and verifies it after?
   TB uses single-threaded test runner; if GV uses `tokio::test` with parallel tests,
   thread-local is safer.

4. **`Clock` trait for sync testing**: should this be added to `core/` now (easy), or
   deferred until sync is implemented? **Recommendation: add it now.** If timestamps are
   generated at the call site in mutators, retrofitting a `Clock` parameter later requires
   changing every mutator signature. The cost of adding `SystemClock` as a default
   implementation now is trivial.

5. **`ModelChecker` scope**: run in every integration test loop (slower, broader coverage)
   or only in a dedicated `cargo test --bin sim` binary (faster CI, isolated)? TB runs
   `StateChecker` on every cluster tick; the equivalent would be running `ModelChecker`
   after every `run_action()` call in integration tests.

6. **Swarm testing configuration**: should disabled action variants be chosen per-run
   (derived from the seed, like TB) or accepted as CLI args? **Recommendation: per-run
   from seed**, with the seed printed in the run header. This keeps the sim binary
   stateless and makes CI failures trivially reproducible without managing flag combinations.
