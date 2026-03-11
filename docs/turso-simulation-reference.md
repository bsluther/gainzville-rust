# Turso Simulation Reference

Reference for Turso's simulation-based testing approach and how it applies to GV. Turso
repo: `~/dev/rust/turso` at commit `d67d2ac83586f5aa293b4ba1fcb72c7742190a59`.

---

## Core Idea

Turso's simulator is a **model-based, property-driven fuzzer**. It generates random SQL
workloads, executes them against the real database, and verifies that the results match
a parallel in-memory model. The model is authoritative — it is never queried from the DB.
This avoids "breaking the fourth wall" and lets generation stay inside the model's world.

For GV the core insight applies directly: maintain an in-memory model of the world, use
it to generate valid operations, apply mutations to both the model and the real DB, then
verify domain-level properties (forest structure, deletion semantics, sync convergence)
against the model.

---

## Key Concepts

### 1. GenerationContext — Model + Config

The central abstraction. A trait that bundles everything a generator needs:

```rust
pub trait GenerationContext {
    fn tables(&self) -> &Vec<Table>;   // current model state
    fn opts(&self) -> &Opts;           // generation configuration
}
```

**File:** `sql_generation/generation/opts.rs:14-18`

The trait is implemented by concrete structs that carry actual state:

- `ConnectionGenContext` — per-connection struct created in `simulator/runner/env.rs:602`
- `TestContext` — empty default for unit tests (`sql_generation/generation/mod.rs:210`)

**GV Application:** `SimulationContext` expands to hold both model state (`Vec<Entry>`,
`Vec<Activity>`, etc.) and generation config (`SimulationOpts`). Generators pull from
context instead of receiving data through `ArbitraryFrom`.

---

### 2. Opts — Generation Configuration

A nested struct that controls *how* each operation is generated. Separate from query-type
weights (which control *which* operation is chosen).

**File:** `sql_generation/generation/opts.rs:20-208`

Key fields:

| Field | Default | Purpose |
|---|---|---|
| `table.column_range` | `1..11` | Columns per normal table |
| `table.large_table.large_table_prob` | `0.1` | Probability of a large table |
| `table.large_table.column_range` | `64..125` | Columns for large tables |
| `query.select.order_by_prob` | `0.3` | ORDER BY inclusion probability |
| `query.select.compound_selects` | `[0:w95, 1:w4, 2:w1]` | UNION/INTERSECT distribution |
| `query.from_clause.joins` | `[0:w90, 1:w7, 2:w3]` | JOIN count distribution |
| `query.insert.min_rows..max_rows` | `1..10` | Rows per INSERT VALUES |
| `query.alter_table.alter_column` | `false` | Enable ALTER COLUMN generation |
| `arbitrary_insert_into_select` | `false` | Enable INSERT INTO ... SELECT |

Opts uses `WeightedIndex` for distributions and `serde`+`garde` for JSON5 loading and
validation.

**Profiles** bundle specific Opts overrides:

- `write_heavy`: bumps `large_table_prob` to 0.4, insert rows to 5..11, insert weight 70
- `differential`: disables `alter_column` (SQLite doesn't support it)

**File:** `simulator/profiles/mod.rs:51-85`

**GV Application:** Map your planned config params directly:

```rust
pub struct SimulationOpts {
    pub p_semantic: f64,         // probability of domain-meaningful data vs random
    pub p_valid: f64,            // probability of schema-valid data
    pub date_mean: DateTime<Utc>,
    pub date_std_secs: f64,
    // Operation mix (separate from Opts — see QueryProfile below)
    pub create_entry_weight: u32,
    pub delete_entry_weight: u32,
    pub update_entry_weight: u32,
    // ...
}
```

---

### 3. QueryProfile — Operation Mix Weights

Separate from `Opts`. Controls the probability *distribution over operation types*, not
how each operation is generated internally.

**File:** `simulator/profiles/query.rs`

Default weights:
```
select: 60, insert: 30, update: 20, delete: 20
create_table: 15, alter_table: 2, drop_table: 2
create_index: 5, drop_index: 2, pragma: 2
```

This clean separation — mix weights vs. per-operation config — is worth copying. It lets
you define profiles like:

- `bulk_populate`: high create/insert weights
- `churn`: balanced create/update/delete
- `sync_stress`: interleaved multi-actor operations

---

### 4. Shadow Trait — Mirroring DB State in the Model

After executing a query on the real DB, the same logical effect is applied to the
in-memory model via `shadow()`.

**File:** `simulator/generation/mod.rs:19-22`

```rust
pub(crate) trait Shadow {
    type Result;
    fn shadow(&self, tables: &mut ShadowTablesMut<'_>) -> Self::Result;
}
```

Each query type implements `Shadow`. Implementations live in `simulator/model/mod.rs`.

| Operation | Model Effect | Lines |
|---|---|---|
| CREATE | Pushes new `Table` | ~265 |
| INSERT | Extends `table.rows` | ~353 |
| DELETE | Removes matching rows | ~293 |
| UPDATE | Records delete (old) + insert (new) | ~593 |
| DROP | Removes table | ~331 |
| ALTER TABLE | Renames/adds/drops columns | ~660 |
| CREATE INDEX | Appends to `table.indexes` | ~281 |
| BEGIN | `create_snapshot()` — clones committed state | ~567 |
| COMMIT | `apply_snapshot()` — replays ops onto committed | ~577 |
| ROLLBACK | `delete_snapshot()` — discards transaction | ~585 |

**GV Application:** Each `Delta<M>` variant is the equivalent of a Shadow impl.
Applying a `Delta<Entry>::Insert` to both the DB (via your existing mutators) and
`Vec<Entry>` in the model is the same pattern. The key difference: Turso's shadow
is invoked *after* execution; in GV the delta can be verified *before* applying too.

---

### 5. Transaction Model in the Shadow

Two levels of table state in `SimulatorEnv`:

- `committed_tables` — stable, committed DB state
- `connection_tables: Vec<Option<TransactionTables>>` — per-connection transaction snapshot

**File:** `simulator/runner/env.rs:40-96, 171-274`

During a transaction, DML operations are recorded as `RowOperation` entries (`Insert`,
`Delete`, `DropTable`, `RenameTable`) in the snapshot. On COMMIT they are replayed onto
`committed_tables` in order — this correctly handles mid-transaction renames, where later
operations reference the table's new name.

---

### 6. Property Checking — What Gets Verified

Turso doesn't just check random assertions; it builds structured **properties**: sequences
of interactions with explicit pre-conditions (assumptions) and post-conditions (assertions).

**File:** `simulator/generation/property.rs`

Key properties:

| Property | Pattern |
|---|---|
| `InsertValuesSelect` | INSERT → optional intermediate queries → SELECT matching row → assert present |
| `ReadYourUpdatesBack` | UPDATE → SELECT updated columns → assert new values |
| `DoubleCreateFailure` | CREATE → CREATE same table → assert error |
| `TableHasExpectedContent` | SELECT * → assert rows == model rows |
| `AllTableHaveExpectedContent` | Same, for every table |

**Assertion structure** (`simulator/model/interactions.rs:386-425`):
```rust
type AssertionFunc =
    dyn Fn(&Vec<ResultSet>, &mut SimulatorEnv) -> Result<Result<(), String>>;
```

Assertions take the result stack and the full env (model), and return `Ok(Ok(()))` on
pass or `Ok(Err(message))` on failure.

**Assumptions** are checked before running a property. If preconditions aren't met (e.g.,
"table must be non-empty"), the property is skipped rather than failed.

After all interactions: `PRAGMA integrity_check` runs on the real DB.

**File:** `simulator/main.rs:539`

**GV Application:** GV's properties are domain-level rather than DB-level:

- "entries form a forest" — check after every mutating operation
- "no child of a scalar entry" — check after any INSERT/UPDATE of position
- "recursive delete removes only entry + descendants" — compare model before/after delete
- "sync convergence" — per-actor model states agree after simulated reconnect

These run on the model, not via DB queries — cheaper and more targeted.

---

### 7. Generation Helpers

**`frequency`** — weighted random selection across generators
**File:** `sql_generation/generation/mod.rs:88-107`

```rust
frequency(vec![
    (60, Box::new(gen_select)),
    (30, Box::new(gen_insert)),
    (20, Box::new(gen_update)),
], rng)
```

Useful for any weighted operation dispatch.

**`backtrack`** — retry-based selection for fallible generators
**File:** `sql_generation/generation/mod.rs:118-141`

```rust
backtrack(vec![
    (3, Box::new(gen_child_entry)),    // try up to 3 times
    (1, Box::new(gen_root_entry)),     // fallback
], rng)
```

Each choice has a retry count. On failure the count decrements; when exhausted that
choice is dropped. Returns `None` only if all choices are exhausted.

**GV Application:** Directly applicable. Generating a child entry might fail if no
sequence entries exist yet. `backtrack` lets you retry and fall back to a root entry
rather than panicking or silently skipping.

---

### 8. Whopper — Fast Fuzzer Without a Model

A second, lighter simulator that skips the shadow model entirely.

**File:** `whopper/main.rs`

- Executes randomly against the real DB at high speed
- No property or row-level verification
- Verification is only: `PRAGMA integrity_check` at the end
- Modes: `fast` (100K steps), `chaos` (10M), `ragnarök` (1M + simulated bit flips)

Useful for finding crashes and corruption bugs quickly before the heavier property-based
simulator runs. Whopper finds "does it crash"; the main simulator finds "does it lie."

---

## How a Simulation Run Works (Turso)

```
1. Load profile (JSON5 or preset)
2. Generate initial schema (CREATE TABLE interactions)
3. Main loop (up to max_ticks):
   a. Generate next interaction (query or property)
   b. Execute on real DB connection
   c. shadow() — apply effect to in-memory model
   d. If assertion: compare result stack to model
   e. If assumption fails: skip property, continue
4. Final: PRAGMA integrity_check
5. On failure: shrink the interaction sequence to minimal reproducer
```

**Relevant files:**
- Plan generation: `simulator/generation/plan.rs`
- Execution loop: `simulator/runner/execution.rs:74-146`
- Query execution + shadowing: `simulator/runner/execution.rs:179-299`
- Shadow implementations: `simulator/model/mod.rs`
- Property definitions: `simulator/generation/property.rs`

---

## Differences in GV

| Aspect | Turso | GV |
|---|---|---|
| Model type | Generic relational (`Vec<Table>`) | Typed Rust structs per table |
| Mutation representation | SQL AST + Shadow impl | `Delta<M>` enum |
| Properties verified | DB-level ("read what you wrote") | Domain-level ("entries form a forest") |
| Verification mechanism | Execute SELECT, diff against model rows | Run invariant fns over model |
| Schema flexibility | Tables created/dropped dynamically | Schema is fixed; model tracks data |
| Sync testing | Not a concern | First-class goal |
| Query mirroring | Required (model predicts SELECT results) | Not needed (no SELECT verification) |

The biggest advantage GV has: mutations are already reified as `Delta<M>`, so the model
update is just pattern-matching on variants. Turso has to teach every SQL query type how
to update the model via Shadow impls.

The biggest complexity GV avoids: mirroring SQL query semantics in the model. Turso must
implement filtering, joining, and column projection in the shadow to predict SELECT
results. GV never needs this because it doesn't verify query output — it verifies
domain properties over the model state.

---

## Leads for Further Investigation

- **Shrinking** (`simulator/runner/execution.rs`, search `shrink`): When a failure is
  found, Turso minimizes the interaction sequence to the smallest reproducer. This is the
  property-based testing equivalent of QuickCheck shrinking. Worth implementing once the
  basic simulator works.

- **Differential testing** (`simulator/runner/env.rs`, `cli_opts.differential`): Turso
  can run the same workload against both Limbo and SQLite simultaneously and diff results.
  GV could do something similar against a reference implementation or a previous version.

- **Fault injection / I/O simulation** (`simulator/profiles/mod.rs`, `IOProfile`):
  Turso simulates disk faults, latency, and crashes. Relevant for GV's sync testing —
  e.g., simulate a dropped connection mid-sync.

- **Multi-connection / MVCC** (`simulator/runner/env.rs`, `connection_tables`): Turso
  runs multiple concurrent connections and tests MVCC correctness. The per-actor model
  pattern maps directly to GV's offline/sync scenario.

- **`ArbitraryFromMaybe`** (GV's `generation/lib.rs:41`): You already have this. Turso
  uses the equivalent via `backtrack`. Worth unifying these into one consistent fallible
  generation pattern.

- **JSON5 profile loading** (`simulator/profiles/mod.rs:140`): Turso loads profiles from
  JSON5 files with schema validation via `garde`. Worth doing once GV's `SimulationOpts`
  struct stabilizes — makes it easy to run different profiles from CI.
