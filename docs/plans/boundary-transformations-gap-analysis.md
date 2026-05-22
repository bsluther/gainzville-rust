# Boundary Transformations — Gap Analysis

*Stage 2 output for the refactor proposed in [boundary-transformations-assessment.md](./boundary-transformations-assessment.md).*
*Status: Stage 3 (Phases A–C) shipped on main. Next: Stage 4 (FFI refactor). See "Notes from Stage 3 execution" near the bottom for handoff details.*

## How to read this document

The Stage 1 proposal hypothesized a two-layer factoring (leaf encoding +
structural reshaping) for both the DB and FFI boundaries, and listed open
questions about how the current code maps to that factoring. This document
answers those questions against the repo, calls out where reality forces the
Stage 1 plan to bend, and recommends sequencing for Stages 3 and 4.

Every claim is cited with `file:line`. When a Stage 1 caveat is confirmed, this
document just says so and moves on; when reality forces an amendment, that
amendment is in the "Constraints that force adjustments" section.

## Verdict at a glance

- **DB boundary.** The Stage 1 proposal is directionally correct. The `*Row`
  pattern is already in place for the nested models (Entry, Attribute, Value)
  but absent for the flat ones (User, Activity, Actor), and the read/write
  asymmetry the proposal flagged is real. The leaf-layer pattern Stage 1 wants
  (`Type<DB>` + `Encode/Decode` on newtypes) already exists in production for
  `Email` and `Username` — Stage 3 extends that pattern; it does not invent it.
  **The structural opportunity that emerged:** the shared SQL machinery (Row
  types + sqlx-aware leaf newtypes) doesn't belong in either `client` or
  `server` — both are "core logic that happens to use a particular DB" — and
  it doesn't belong in `core` either, since core should stay DB-agnostic. A
  new `gv_sql` crate is the natural home and the rest of this document
  assumes it.
- **FFI boundary.** The proposal is achievable as written, with no
  surviving non-mirror types. uniffi 0.31 with proc-macro mode is already
  in use, every crossing type is a Record/Enum (eligible for `[Remote]`),
  and all exposed fields are `pub` so uniffi's all-fields-by-value
  treatment bypasses no invariants. The two `FfiEntryJoin` differences
  resolve cleanly: drop the dead-overhead `HashMap` in core in favor of
  `Vec` (zero callers use the HashMap's lookup), and promote the computed
  `display_name` to a real field on core `EntryJoin` populated at
  construction. After both changes, every domain type is a pure 1:1 mirror.
- **Round-trip tests.** Stage 1 names `hegel` as the harness; `hegel` is not in
  the repo. The `generation/` crate's `Arbitrary` infrastructure plus the
  already-declared (and currently unused) `proptest` dependency are the
  realistic stack.

## DB boundary — findings vs. proposal

### Models that cross the DB boundary

Six core models cross:

| Model | Definition | Read path | Write path |
|-------|-----------|-----------|------------|
| User | `core/src/models/user.rs:9` | `client/sqlite_executor.rs:49-57`, `server/postgres_executor.rs:45-54` | `client/sqlite_delta_executor.rs:59-92`, `server/postgres_delta_executor.rs:76-124` |
| Actor | `core/src/models/actor.rs:21` | (insert/delete only — no read path) | `client/sqlite_delta_executor.rs:36-56`, `server/postgres_delta_executor.rs:42-73` |
| Activity | `core/src/models/activity.rs:7` | `client/sqlite_executor.rs:88-114`, `server/postgres_executor.rs:82-110` | `client/sqlite_delta_executor.rs:95-127`, `server/postgres_delta_executor.rs:127-177` |
| Entry | `core/src/models/entry.rs:11` | `client/sqlite_executor.rs:119-159` | `client/sqlite_delta_executor.rs:130-209`, `server/postgres_delta_executor.rs:181-266` |
| Attribute | `core/src/models/attribute.rs:23` | `client/sqlite_executor.rs:302-343` | `client/sqlite_delta_executor.rs:212-250`, `server/postgres_delta_executor.rs:269-314` |
| Value | `core/src/models/attribute.rs:175` | `client/sqlite_executor.rs:353-391` | `client/sqlite_delta_executor.rs:253-305`, `server/postgres_delta_executor.rs:317-363` |

Plus two read-only join shapes (`EntryJoin`, `AttributePair`) used by queries
that combine rows from multiple tables.

### `*Row` pattern: half-built and asymmetric

Row types that exist today:

- `EntryRow` (`core/src/models/entry.rs:43`) — both directions, but only the
  read direction goes through it; writes bypass it.
- `AttributeRow` (`core/src/models/attribute.rs:267`) — bidirectional via
  `from_attribute` (line 276) and `to_attribute` (line 287).
- `ValueRow` (`core/src/models/attribute.rs:300`) — bidirectional via
  `from_value` (line 311) and `to_value` (line 334).
- `EntryJoinRow` (`core/src/models/entry_join.rs:97`) — read-only.
- `AttributePairRow` (`core/src/models/attribute_pair.rs:137`) — read-only.

Models with no Row type at all: `User`, `Activity` (read directly into the core
type via `#[derive(sqlx::FromRow)]`), `Actor` (no `FromRow` either).

The read/write asymmetry Stage 1 anticipated is confirmed:

- **Reads:** Entry/Attribute/Value go `database → *Row → core`; User/Activity
  go `database → core` directly; Actor has no read path.
- **Writes:** every model goes `core → inline bind`. For Attribute/Value the
  Row type is used as a staging struct (mainly to host the JSON serialization
  of the config / value enum), but the binds are still positional and inline.
  Entry's writes unroll `Position` and `Temporal` at bind sites without going
  through `EntryRow` at all.

### Per-model nesting

Flat (no nested structs, no collections): **User, Activity, Actor.** A Row type
would be optional for these — `#[derive(FromRow)]` on the core struct already
works.

Nested (Row type is load-bearing): **Entry** has `Position` and `Temporal`;
**Attribute** has `AttributeConfig` (3-variant enum with per-variant configs);
**Value** has `AttributeValue` (3-variant enum, `MassValue` carries
`Vec<MassMeasurement>`).

### Multi-DB target is live, not aspirational

Both backends exist and diverge in leaf encoding:

- **UUID:** Postgres native vs. SQLite `BLOB`
  (`client/migrations/20250109000000_initial_schema.sql:7,14,21,30`).
- **Timestamp:** Postgres `TIMESTAMPTZ` (`DateTime<Utc>` binds directly,
  `server/postgres_delta_executor.rs:214,246`) vs. SQLite `TEXT` RFC3339
  (`.to_rfc3339()` at `client/sqlite_delta_executor.rs:43,163-164`).
- **Boolean:** Postgres `BOOLEAN` vs. SQLite `INTEGER` (sqlx handles).
- **Durations:** both use `i64`.

The pattern Stage 1 calls for at the leaf layer — per-DB encoding via sqlx
traits on a newtype — already exists for `Email` and `Username`, each of which
declares `Type<Postgres>`, `Type<Sqlite>`, and the corresponding `Decode` impls
in `core/src/validation.rs:8` and `core/src/validation.rs:87`. Extending that
pattern to `Uuid`, `DateTime<Utc>`, and `FractionalIndex` is straightforward.

### sqlx usage is split by backend, but the split is mostly operational

- **Reads (both backends):** runtime `sqlx::query_as::<_, RowType>(...)` with
  `.bind(...)`. Implementations are near-identical modulo `?` vs. `$N`
  placeholder syntax. Compare `client/sqlite_executor.rs:236` with
  `server/postgres_executor.rs:233` — same shape, same Row type, same bind
  chain.
- **Writes — Postgres:** compile-time `sqlx::query!` macros
  (e.g. `server/postgres_delta_executor.rs:46-68,80`). Type-checks each
  parameter against the column type using live-DB schema introspection;
  requires literal SQL strings and parameters as macro args.
- **Writes — SQLite:** runtime `sqlx::query(...)` with chained `.bind(...)`
  (e.g. `client/sqlite_delta_executor.rs:40,63,99`). No compile-time SQL
  check.

**What this asymmetry actually imposes on the Row refactor: not much.** Both
shapes can pull values out of a Row equally well — either
`query!("INSERT ... VALUES ($1, $2, ...)", row.a, row.b, ...)` or
`query("INSERT ... VALUES (?, ?, ...)").bind(row.a).bind(row.b)...`. The Row
removes the leaf-encoding tangling either way; the only thing the asymmetry
blocks is the most ambitious form of mechanization — a single derive macro
that auto-generates *both* backends' writes from one Row struct — which was
never the primary goal of this refactor. Hand-writing one literal SQL string
per model per backend is fine; it doesn't scale with the number of leaf
types.

`#[derive(sqlx::FromRow)]` is used on User, Activity, Entry, plus the explicit
Row types. `FromRow` is DB-generic, so the read half does work across both
backends from a single derive.

### Delta::Update shape: confirmed

`pub enum Delta<M> { Insert { new }, Update { old, new }, Delete { old } }`
(`core/src/delta.rs:10-14`). `old` is currently unused on the write path;
nothing about the current design blocks binding it into a `WHERE` clause for
optimistic concurrency later. `AnyDelta` enumerates the per-model deltas
(`core/src/delta.rs:25`). Updater builders (`EntryUpdater`, `ActivityUpdater`,
`UserUpdater`) construct `Delta::Update { old, new }` at the call site.

### Where the current write-side leaf mapping lives

- **Inline at bind sites** for simple cases: `.to_string()` /
  `.to_rfc3339()` calls passed straight to `.bind(...)`
  (`client/sqlite_delta_executor.rs:103,157-165`).
- **In helper methods** for the JSON-encoded enums: `AttributeRow::from_attribute`,
  `ValueRow::from_value` (`core/src/models/attribute.rs:276,311`).
- **Newtype sqlx impls** for the validated string newtypes: `Email`, `Username`
  (`core/src/validation.rs:8,87`).

The leaf code is not opaque, but it is scattered across three locations and
the choice of which location to use is per-field, not principled.

## FFI boundary — findings vs. proposal

### uniffi setup

uniffi 0.31 (`gv-ffi/Cargo.toml:16`), **proc-macro mode only** — no `.udl`,
no `build.rs`, just `uniffi::setup_scaffolding!()` at `gv-ffi/src/lib.rs:1`.
This is exactly the configuration Stage 1 assumes. No `custom_type!` or
`[Remote]` declarations exist yet.

### Every crossing domain type is a Record or Enum

About 35 FFI types are exported; all of the ones that mirror core domain types
are uniffi `Record` or `Enum`. The only uniffi `Object` types in the crate are
service interfaces (`GainzvilleCore`, `FfiQuerySubscription` at
`gv-ffi/src/core.rs:43-62`), which are not refactor candidates and would not be
remote-eligible anyway.

**Conclusion:** Stage 1's remote-record/remote-enum approach is applicable to
every domain type that currently crosses the boundary, with one exception
(see below).

### All exposed fields are `pub`; no invariants are bypassed by remote-record exposure

Spot-checked across User, Activity, Entry, Position, Attribute, Value,
MassMeasurement, AttributePair: every field a uniffi `Record` would expose is
`pub` on the core type. `Position::parse` and `Temporal::parse` exist as
fallible constructors, but they operate on already-public fields and don't
hide anything post-construction. `Username` and `Email` have private
constructors but are converted to `String` at the FFI boundary anyway.

This needs to be re-checked when any new domain model is added, but the
current surface is safe to expose as remote records.

### One small remaining non-mirror shape: `FfiEntryJoin`

`EntryJoin` is the only crossing type that differs in shape from its FFI
counterpart. Today there are two differences, but one is dead overhead and
should be eliminated regardless of the refactor.

**1. `attributes` collection type.** Core:
`HashMap<Uuid, AttributePair>` (private) at
`core/src/models/entry_join.rs:14-19`. FFI: `Vec<FfiAttributePair>` at
`gv-ffi/src/types.rs:646-670`.

The HashMap is dead structural overhead:

- `EntryJoin::attribute(attr_id)` — the O(1) lookup the HashMap exists for
  — has **zero callers** anywhere in the repo.
- `EntryJoin::attributes()` (the iterator) has **one caller**: the FFI
  conversion, which immediately flattens the HashMap into a Vec.
- The uniqueness invariant (one `AttributePair` per `attr_id`) is already
  enforced by the DB via `Value`'s composite PK
  `(entry_id, attribute_id)`; the HashMap is not the source of truth.

**Recommendation:** switch `EntryJoin.attributes` to `Vec<AttributePair>`
in core. This is a strict simplification — no behavior change, less code,
and it removes one of the two reasons `FfiEntryJoin` is non-mirror. The
change is independent of the rest of this refactor and could land
immediately.

**2. `display_name: String` precomputed field on FFI side** (around
`gv-ffi/src/types.rs:654`), populated via `EntryJoin::display_name()` so
Swift doesn't re-implement the entry-name / activity-name / "Unnamed"
fallback. uniffi `[Remote]` records expose fields only (no methods), so
the FFI already stores `display_name` as a real field.

**Decision:** promote `display_name: String` to a real field on core
`EntryJoin`, populated at construction time in `EntryJoin::from_row()`.
Keep `EntryJoin::display_name()` as the function that computes the value;
the constructor calls it once and caches the result. This makes core
match the shape the FFI already uses, makes `EntryJoin` a pure 1:1
mirror, and keeps the fallback rule defined in one place.

**Implication:** with both changes (HashMap → Vec, and `display_name` as
a core field), `EntryJoin` becomes a pure mirror eligible for `[Remote]`
with no hand-written transform. Stage 4 no longer needs to carry any
"this one type stays explicit" caveat — every crossing type is a 1:1
encoding mirror.

### Hand-written conversion code today

About 615 lines split across 32 `From` / `TryFrom` impls and 5 free functions
(`ffi_action_to_core`, `ffi_entry_to_core`, `ffi_temporal_to_core`,
`ffi_activity_to_core`, `ffi_position_to_core`) in `gv-ffi/src/types.rs`.
Representative examples: `FfiUser → User` (lines 144–152, infallible);
`FfiAnyQuery → AnyQuery` (lines 734–826, fallible, 92 lines);
`ffi_action_to_core` (lines 57–133, 76 lines). Once leaf custom types +
remote records are in place, almost all of this is expected to disappear;
the only survivors are `EntryJoin`'s transform and any future deliberate
non-mirror.

### Leaf types crossing the FFI boundary

Priority order for `custom_type!` declarations:

1. `Uuid ↔ String` — appears in ~15 field positions across records.
2. `DateTime<Utc> ↔ i64` (Unix milliseconds) — `FfiTemporal` variants.
3. `FractionalIndex ↔ String` — `Position.frac_index`.
4. `Username ↔ String`, `Email ↔ String`, `ActivityName ↔ String` — already
   have validating parsers; the custom type encapsulates them.

Current call sites worth referencing as the model: `parse_uuid` at
`gv-ffi/src/types.rs:43-45`, `parse_timestamp_ms` at
`gv-ffi/src/types.rs:52-55`, `FfiPosition::from` at
`gv-ffi/src/types.rs:185-192`, `FfiTemporal::from` at
`gv-ffi/src/types.rs:206-230`.

### Error handling

`FfiError` is a single-variant enum: `Generic(String)`
(`gv-ffi/src/types.rs:29-33`). Fallibility on the way in (Ffi → core) comes
from leaf parsing (UUID, timestamp, FractionalIndex, Email, Username) and from
domain validators (`Position::parse`, `Temporal::parse`). A `custom_type!`
declaration handles the leaf fallibility cleanly; the remaining domain
validators stay inside whatever hand-written transforms survive (today: just
`FfiEntryJoin`'s transform after the refactor).

## Cross-cutting — round-trip tests and harness

**`hegel` is not in the repo.** Searched workspace `Cargo.toml`, all path
deps, and source — the name appears only in
`docs/plans/boundary-transformations-assessment.md`. Stage 1 is the only
mention.

**What exists instead:** the `generation/` crate provides `Arbitrary`,
`ArbitraryFrom<T>`, and `ArbitraryFromMaybe<T>` traits
(`generation/lib.rs:95-103`) with implementations for the leaf types
(`Email`, `Username`, `Uuid`, `DateTime<Utc>`, `FractionalIndex`) and the
core models (Entry, Activity, Attribute, Value). It uses `rand` + distribution
sampling, not `proptest` or `quickcheck`. No round-trip tests are written on
top of it today.

**`proptest` is declared at `core/Cargo.toml:18` but unused.** It can either
be dropped or adopted to drive the `generation::Arbitrary` impls into
property-based round-trip tests with shrinking.

**The existing round-trip-shaped code is unguarded.**
`AttributeRow::from_attribute` / `to_attribute`, `ValueRow::from_value` /
`to_value`, `Position::parse`, `Temporal::parse` all encode/decode logic
without tests confirming `decode(encode(x)) == x`.

**Working assumption for Stage 3:** the longer-term plan is to adopt
`hegel-rust` for property-based testing, but doing so is its own unit of
work — `hegel` integration with the existing `generation::Arbitrary` infra
(which is broader-purpose, supporting DST and dev data generation) is
non-trivial. For *this* refactor, getting reasonable coverage from the
existing `Arbitrary` impls plus plain unit tests is the starting point.
PBT/`hegel` adoption is tracked as separate future work and is not a Stage 3
prerequisite.

## The real centralization gain

The refactor's primary value is not avoiding repeated SQL — it's eliminating
the scatter and repetition of encode/decode logic across the read and write
paths. Today, for a single field like `Entry.position.frac_index`, the
encoding rule (`FractionalIndex → String`) appears at the write bind site
(`client/sqlite_delta_executor.rs:157-165`) and the decoding rule shows up
again on the read side (currently via `EntryRow::to_entry`). Per-DB encoding
choices (Postgres `TIMESTAMPTZ`-native vs. SQLite RFC3339 text) are
interleaved with the same code that picks which column to bind to, so
changing an encoding means hunting through the bind chain.

The Row-plus-leaf-newtype pattern collapses this:

- **Encoding rules live in one place per leaf type**, as the leaf newtype's
  sqlx trait impls. Postgres-vs-SQLite divergence for `Uuid`, `DateTime<Utc>`,
  `FractionalIndex`, etc. is a property of `Type<Postgres>` vs. `Type<Sqlite>`
  on the newtype — invisible to the Row, invisible to the SQL.
- **The Row is the single coherent shape** for `core ↔ DB`. The conversion
  `Entry ↔ EntryRow` is the only hand-written, domain-aware code; the SQL is
  just plumbing for already-encoded values.
- **Reads and writes share the Row.** Today they don't: reads go through
  `EntryRow`, writes bypass it. After the refactor, both halves are the same
  Row → SQL bind pattern.

Avoiding the second SQL string per model would be nice but is not what this
refactor is buying.

## Structural recommendation: extract a `gv_sql` crate

The shared SQL machinery — Row types, sqlx-aware leaf newtypes, the
`core ↔ Row` transforms, and the executor impls themselves — doesn't
naturally belong in either `client` or `server`. Those crates are "core
logic that happens to use a particular DB backend"; the recent
`DeltaExecutor` trait introduction made that explicit by letting core
abstract over the DB connection. It also doesn't belong in `core`, which
should stay DB-agnostic so that non-DB consumers (`gv-ffi`, `generation`,
`ivm`) don't pull in sqlx.

**Proposed home: a new `gv_sql` crate.**

### What moves into `gv_sql`

- All `*Row` types currently in `core/src/models/`: `EntryRow`,
  `AttributeRow`, `ValueRow`, `EntryJoinRow`, `AttributePairRow`.
- The hand-written `core ↔ Row` conversion methods (`from_*` / `to_*`).
- New Row types added for uniformity: `UserRow`, `ActivityRow`, `ActorRow`.
- New leaf newtypes with per-DB sqlx impls: `UuidColumn`, `DateTimeColumn`,
  `FractionalIndexColumn`.
- The relocated leaf encoders currently in core: wrapper newtypes `EmailColumn`,
  `UsernameColumn`, `ActivityNameColumn` that hold the corresponding core type
  and carry the sqlx impls (see orphan-rule note below).
- **The executor impls themselves:** `SqliteQueryExecutor`,
  `PostgresQueryExecutor`, `SqliteDeltaExecutor`, `PostgresDeltaExecutor`.
  Verified to be pure DB plumbing — their only inputs are a sqlx connection
  and core query/delta types. Gated per-backend behind feature flags.
- The `migrations/` directories. Schema belongs alongside the executor that
  assumes it; `gv_sql::sqlite::migrate(conn)` and
  `gv_sql::postgres::migrate(conn)` become the entry points.
- Round-trip tests for the above — both in-memory (`core → Row → core`) and
  integration (write to a real DB, read back).

### What stays in `core`

- All domain types (`Entry`, `Activity`, `User`, `Attribute`, `Value`,
  `Position`, `Temporal`, `AttributeConfig`, etc.).
- `Delta`, `AnyDelta`, the `DeltaExecutor` trait, the `QueryExecutor` trait,
  and query type definitions.
- The validated newtypes themselves (`Email`, `Username`, `ActivityName`),
  but **without** their current sqlx impls — those move out behind the
  wrapper newtypes in `gv_sql`.
- Forest, mutators, actions.

### What stays in `client` and `server`

- `client/`: app shell. Owns the SQLite-backed app type, connection pool,
  app lifecycle, FFI integration glue, subscription / UI plumbing.
- `server/`: HTTP shell. Owns HTTP routes, auth middleware, request
  handling, the server type that holds a `PostgresQueryExecutor` and
  dispatches HTTP requests to `executor.execute(...)`.

These crates become thinner — "the SQLite app" and "the Postgres HTTP
server" — but the names stay accurate.

### The orphan-rule note

Today `core/src/validation.rs:60-84` (`Email`) and
`core/src/validation.rs:115-142` (`Username`) define their sqlx impls *in
core* because the orphan rule blocks `impl ForeignTrait for ForeignType`
from anywhere else: core owns `Email`, sqlx is foreign, so core is the only
legal home.

For `gv_sql` to own the sqlx impls without dragging sqlx into core, the
existing leaf types need wrapper newtypes that `gv_sql` owns:
`EmailColumn(Email)`, `UsernameColumn(Username)`,
`ActivityNameColumn(ActivityName)`, plus `UuidColumn(Uuid)`,
`DateTimeColumn(DateTime<Utc>)`, `FractionalIndexColumn(FractionalIndex)`.
The sqlx impls land on the `*Column` types; Row structs use the `*Column`
types as field types; `core ↔ Row` conversion wraps/unwraps at the
boundary.

This makes the leaf-encoding rule uniform — *every* leaf that crosses the
DB boundary is a `*Column` newtype in `gv_sql`, no exceptions. The Stage 1
doc sketched this same pattern with the name `GvUuid(Uuid)`; the
`*Column` suffix is preferred because it names the role (a row-column
encoder) rather than the owning crate. The tradeoff is one thin layer of
wrapping at the Row boundary in exchange for core having zero sqlx-shaped
knowledge.

### sqlx feature gating

`sqlx` is one crate with optional features `sqlite`, `postgres`,
`runtime-tokio-rustls`, etc. `gv_sql` should re-expose backend selection as
its own features so consumers opt into the backends they actually use:

```toml
# gv_sql/Cargo.toml
[features]
sqlite   = ["sqlx/sqlite"]
postgres = ["sqlx/postgres"]

# client/Cargo.toml
gv_sql = { path = "../gv_sql", features = ["sqlite"] }

# server/Cargo.toml
gv_sql = { path = "../gv_sql", features = ["postgres"] }
```

The Row *shapes* are not feature-gated; only the sqlx trait impls and the
per-backend executor modules are. `gv_sql::sqlite` and `gv_sql::postgres`
become the feature-gated submodules that house the backend-specific
executors and sqlx impls.

**Watch out for workspace feature unification** — the same gotcha that bit
the project with `arbitrary_precision`. When the workspace is built as a
whole, `client` enabling `sqlite` and `server` enabling `postgres` will
cause both features to be active in every binary. For sqlx this is probably
benign (additive: extra types/traits, no behavior change), but worth
verifying once the crate exists and noting in `CLAUDE.md` if confirmed.

### Dependency graph (no cycles)

```
core           — no deps; pure domain
generation  -> core
gv_sql      -> core, sqlx
client      -> core, gv_sql (feature = "sqlite")
server      -> core, gv_sql (feature = "postgres")
gv-ffi      -> core                              (deliberately no gv_sql)
ivm         -> core
integration -> core, gv_sql, client, server
```

`gv-ffi` deliberately does not depend on `gv_sql` — the FFI boundary has
nothing to do with the DB boundary, and keeping them in separate crates
makes that physical instead of conventional.

## Constraints that force adjustments to Stage 1

These are the items where Stage 1 should not be executed unchanged.

1. **The Postgres `query!` vs. SQLite runtime API split is operational, not
   architectural.** An earlier draft of this document called this "the biggest
   constraint" — that was wrong. Both shapes pull values out of a Row equally
   well: either `query!("INSERT ... VALUES ($1, $2, ...)", row.a, row.b, ...)`
   or `query("INSERT ... VALUES (?, ?, ...)").bind(row.a).bind(row.b)...`. The
   Row removes the leaf-encoding tangling either way; the only thing the
   asymmetry blocks is a single derive macro that auto-generates *both*
   backends' writes from one Row struct, which was never the goal. The Stage 3
   decision is just *do we keep `query!` on Postgres or switch to the runtime
   API for code-level uniformity with SQLite?* Default recommendation: keep
   `query!` — compile-time SQL checking is worth more than a tiny gain in
   shape-symmetry, and the "postgres docker required at compile time"
   convention is already an accepted norm in this project.

2. **The read direction is more mechanical than the write direction.**
   `FromRow` is DB-generic and already works for both backends from a single
   derive. The "single bidirectional `Row ↔ database` transform" framing of
   Stage 1 is correct in spirit, but the read half is auto-derived while the
   write half is a per-backend hand-written SQL string. That's still
   centralization (the leaf encoding is no longer scattered), just not
   symmetry.

3. **`EntryJoin`'s non-mirror status dissolves into two small core
   changes.** Today it differs from `FfiEntryJoin` in two ways, both
   resolvable in core: switch `attributes` from `HashMap<Uuid, AttributePair>`
   to `Vec<AttributePair>` (the HashMap has zero meaningful callers — see
   FFI section above), and promote `display_name: String` to a real field
   populated at construction by calling the existing `display_name()`
   function from `from_row()`. After both changes, `EntryJoin` is a pure
   1:1 mirror eligible for `[Remote]` with no surviving hand-written
   transform. The HashMap change is a strict simplification that can land
   independently of the rest of the refactor.

4. **`hegel-rust` adoption is deferred to a separate work block.** Stage 1
   names `hegel` as the harness; it is not in the repo today. The plan is to
   adopt `hegel-rust` for property-based testing eventually, but integrating
   it with the existing `generation::Arbitrary` infrastructure (which is
   broader-purpose — it supports DST and dev data generation, not just PBT)
   is non-trivial enough to be its own block of work. For Stage 3, build
   round-trip tests using the existing `Arbitrary` impls and/or plain unit
   tests; that's a starting point that can grow into full PBT later.

5. **Per-model Row decision should be made uniformly.** Stage 1 leaves "Row
   type per-model" as an option, noting flat models may skip it. Today:
   Entry/Attribute/Value have Rows; User/Activity/Actor don't.
   **Recommendation:** add Row types for User/Activity (and read-side for
   Actor when/if needed) so the single transform `core ↔ Row` is the universal
   shape. Trade: a handful of mostly-trivial wrapper structs, in exchange for
   eliminating per-model judgment calls and unifying the leaf-encoding entry
   point. Flag this as an explicit recommendation in Stage 3, not a silent
   choice.

6. **Actor has no read path and no `FromRow`.** Not blocking — Actor is
   insert/delete only today — but if a read path is added, it should adopt
   the same Row + leaf-newtype pattern as the others rather than reinventing.

## Recommended sequencing for Stages 3 and 4

### Stage 3 (DB refactor) — prerequisites and order

The guiding principle: **build `gv_sql` additively, then swap one model
at a time so each step is a small reversible commit with a working
build.** Two phases: a scaffold phase that adds infrastructure without
changing behavior, and a per-model swap loop that converts one model end
to end before moving to the next.

**Decisions resolved up front:**
- Keep Postgres `query!` macros. Revisit only if they get in the way.
- No optimistic concurrency `WHERE old.*` binding — sync concern, out of
  scope here.
- `Actor` has no read path; not adding one. Skip `ActorRow` entirely until
  a reader appears.

#### Phase A — scaffold (additive, no consumer changes)

1. **Create the `gv_sql` crate as an empty scaffold.** Cargo.toml, `lib.rs`,
   workspace member declaration, `sqlite` / `postgres` features wired to
   `sqlx/sqlite` / `sqlx/postgres`. No code yet. No consumers depend on it.
2. **Add `*Column` leaf newtypes in `gv_sql`** with `Type<Postgres>` /
   `Type<Sqlite>` / `Encode` / `Decode` impls: `UuidColumn`,
   `DateTimeColumn`, `FractionalIndexColumn`, `EmailColumn`,
   `UsernameColumn`, `ActivityNameColumn`. The sqlx impls for the wrapped
   types still exist in core during this phase — both coexist until the
   per-model swap deletes core's copies.
3. **In-memory round-trip tests for each `*Column`** using
   `generation::Arbitrary` (dev-dep) and/or plain unit tests.
4. **Add Row types and `core ↔ Row` transforms in `gv_sql`** for every
   model that crosses the DB: `EntryRow`, `AttributeRow`, `ValueRow`,
   `EntryJoinRow`, `AttributePairRow` (move-and-adapt from
   `core/src/models/`), plus new `UserRow` and `ActivityRow` for
   uniformity. All Row fields use `*Column` types. Make every
   `core ↔ Row` transform bidirectional. Old Row types in core stay put
   during this phase; `client`/`server` still reference them.
5. **In-memory round-trip tests for every `core ↔ Row` transform.** Proof
   that the new transforms preserve identity before any DB touches them.
6. **Move executors into `gv_sql` wholesale, unchanged.** Lift
   `SqliteQueryExecutor`, `SqliteDeltaExecutor`, `PostgresQueryExecutor`,
   `PostgresDeltaExecutor` and their `migrations/` directories into
   `gv_sql::sqlite` / `gv_sql::postgres`, gated behind the respective
   feature flags. **Do not change bind logic yet** — every executor still
   uses today's inline leaf encoding. Update `client` and `server` to
   import from `gv_sql` instead of their own modules; delete the now-empty
   originals from `client`/`server`. This is the last step that changes
   imports on the consumers; from here on, consumers don't know the
   per-model conversion is happening.
7. **Verify both backends build with both features active.** Single
   workspace build with `--features sqlite,postgres` should succeed
   (validates that sqlx feature unification is benign, per Constraint 7).

After Phase A: `gv_sql` exists, owns the executors, owns the Row types and
`*Column` newtypes — but no executor actually uses a Row type yet.
`client` and `server` are thinner and pass all their existing tests.

#### Phase B — per-model swap loop

For each model in the order **User → Activity → Attribute → Value →
Entry** (flat models first to shake out the pattern on simpler shapes
before tackling the nested ones), do these steps as one or two commits:

1. **SQLite side:** convert `SqliteQueryExecutor` reads for this model to
   go through `Row`; convert `SqliteDeltaExecutor` writes for this model
   to bind from `Row` fields (`*Column`-typed) instead of inline encoding.
2. **Verify `client` tests pass.**
3. **Postgres side:** same conversion for `PostgresQueryExecutor` and
   `PostgresDeltaExecutor`. Postgres writes continue to use `query!`; the
   macro receives `row.field` (a `*Column`) instead of an inline-encoded
   value.
4. **Verify `server` tests pass.**
5. **Delete this model's redundant sqlx impls / inline encoders from
   core** if and only if no other unconverted model still depends on them.
   (The validated newtype sqlx impls in `core/src/validation.rs` only get
   deleted after both `Email` and `Username` have been swapped through.)

`EntryJoin` and `AttributePair` are read-only join shapes, not standalone
models — they get swapped as part of `Entry` / `Attribute` respectively
(or last, after all six base models, if cleaner).

#### Phase C — cleanup

1. **Delete the leftover sqlx impls in `core/src/validation.rs`.** By
   this point every consumer goes through `EmailColumn` / `UsernameColumn`,
   and core can drop its `Type<Postgres>` / `Type<Sqlite>` / `Decode`
   impls. Sqlx dep can come off `core/Cargo.toml`.
2. **Verify `core` has no sqlx dependency.** `cargo tree -p core` should
   not mention sqlx. This is the structural goal of the refactor.

### Stage 4 (FFI refactor) — prerequisites and order

0. **(Preparatory — DONE, landed at the start of Stage 3.)** Two small
   core changes that together make `EntryJoin` a pure 1:1 mirror with
   the FFI side:
   - Switch `EntryJoin.attributes` from `HashMap<Uuid, AttributePair>` to
     `Vec<AttributePair>`. The HashMap has no real callers (see FFI
     section). Shipped in `65fcc2f` ("Simplify EntryJoin").
   - Promote `display_name: String` to a real field on `EntryJoin`,
     populated at construction by the existing fallback logic. Shipped
     in the same commit. Core also gained `EntryJoin::new(entry,
     activity, attributes)` (`334bc20`) for stitching from already-parsed
     parts — uniffi's record machinery won't use it, but any surviving
     hand-written FFI transform can.
1. **Leaf layer.** Declare `custom_type!` for `Uuid`, `DateTime<Utc>`,
   `FractionalIndex`, `Username`, `Email`, `ActivityName` (priority order in
   the FFI section above). On uniffi 0.29+ the closures can be elided where
   `From`/`Into` exist.
2. **Round-trip tests for each leaf custom type.**
3. **Structural layer.** Add `[Remote]` record/enum declarations for every
   crossing domain type, including `EntryJoin` (now a pure mirror after
   step 0). Order matters: a remote record can only reference types that
   are already uniffi-known, so leaf customs must land first, then leaf
   records, then composite records.
4. **Delete the superseded code.** The 32 `From`/`TryFrom` impls and 5
   `ffi_*_to_core` free functions in `gv-ffi/src/types.rs` should now
   disappear. Anything that doesn't disappear is evidence of a model whose
   FFI shape secretly diverges from its core shape — investigate before
   deleting.
5. **Swift-side rename: drop the `Ffi` prefix.** The remote-types switch
   naturally renames the bindings — Swift sees `Entry` instead of
   `FfiEntry`, `Activity` instead of `FfiActivity`, etc. — because uniffi
   takes the type's Rust name verbatim and there's no parallel `Ffi*`
   struct left to disambiguate. Regenerate Swift bindings first; the Swift
   compiler then surfaces every callsite that needs to be updated as a
   compile error, which makes the propagation mechanical and safe. FFI-only
   types with no core counterpart (`FfiError`, `GainzvilleCore`,
   `FfiQuerySubscription`) keep their names by default; renaming them is a
   separate cleanup pass, not part of this step. Watch for Swift name
   collisions on common identifiers (`Entry`, `Value`, `Action`); if any
   collide painfully, the fallback is a short module-level prefix like
   `GV*` instead of bare names — easier to decide once bindings exist and
   the breakage is visible.

### Round-trip tests are non-optional

Stage 1's design rests on "the transforms are total modulo bugs" and on the
identity `core == decode(encode(core))`. Today none of the existing transforms
(`AttributeRow::from_attribute` / `to_attribute`,
`ValueRow::from_value` / `to_value`, `Position::parse`, `Temporal::parse`, every
`From`/`TryFrom` impl in `gv-ffi/src/types.rs`) is guarded by a round-trip
test. Land the test scaffolding before — or alongside — each refactor step so
regressions catch themselves.

## Open questions — resolved

The Stage 2 → Stage 3 handoff resolved the open design questions:

- **Postgres `query!` macros vs. runtime API:** keep `query!`. Revisit
  only if it gets in the way of a per-model conversion.
- **Optimistic concurrency `WHERE old.*` binding:** out of scope. Sync
  concern, addressed elsewhere.
- **`Actor` read path:** not needed today; skip `ActorRow` entirely.
- **Wrapper newtype naming:** `*Column` (`UuidColumn`, `EmailColumn`,
  etc.). Names the role rather than the owning crate.
- **Crate name:** `gv_sql` (underscore). Disambiguates from "SQL writ
  large" and matches Rust identifier shape directly.
- **Test harness:** `generation::Arbitrary` + plain unit tests for Stage
  3. `hegel-rust` adoption is tracked as separate future work.
- **EntryJoin shape changes** (`HashMap → Vec`, `display_name` field):
  landing as the first commit of this work, ahead of Stage 3, since both
  simplifications eliminate code paths the refactor would otherwise have
  to carry.

## Notes from Stage 3 execution — relevant for Stage 4

Stage 3 (Phases A–C) shipped on main as 18 commits, all tests green,
`cargo tree -p gv_core | grep sqlx` returns 0. Findings that affect the
Stage 4 handoff:

- **Stage 4 step 0 is already complete.** `EntryJoin` is a pure 1:1
  mirror today: `Vec<AttributePair>` (not HashMap) and a stored
  `display_name: String` field. `FfiEntryJoin` was simplified to match
  in the same commit (`65fcc2f`). The Stage 4 planner can skip step 0
  entirely.

- **`generation` crate is dependency-clean.** It now depends only on
  `gv_core` plus arbitrary-data utilities — `sqlx`, `tokio`, `dotenvy`,
  `gv_server`, and `gv_sql` were all stripped (`2ae6fb8`). Stage 4
  round-trip tests for `custom_type!` declarations can dev-dep
  `generation` freely without dragging in DB plumbing.

- **`DbErr` extension trait pattern.** Core now has
  `error::DbErr` that converts any `std::error::Error` into
  `DomainError::Database(Box<dyn Error>)`. The variant is type-erased so
  core doesn't depend on sqlx. Stage 4 likely doesn't need a parallel
  pattern at the FFI boundary (`FfiError(Generic(String))` already
  flattens to a string) — but if any FFI conversion needs to surface a
  backend error into `DomainError` from the FFI side, `DbErr` is the
  ready-made bridge.

- **`Position::from_parts(parent_id, frac_index)` is now public in
  core** (`0652b52`). Direct constructor for already-decoded parts,
  bypassing `Position::parse`'s string round-trip. If any surviving
  hand-written FFI Position transform decodes leaves upstream, it can
  call this instead of re-stringifying.

- **The "additive scaffold → per-model swap → cleanup" sequencing
  pattern worked very well** for Stage 3 and is worth reusing for
  Stage 4. Translated:
  - *Phase A-FFI:* declare all `custom_type!` for leaves; add per-leaf
    round-trip tests; nothing else touches FFI structs yet.
  - *Phase B-FFI:* convert one FFI struct at a time to `[Remote]` (or
    elide it entirely if it becomes a pure mirror), deleting that
    struct's hand-written `From`/`TryFrom`. Tests catch breakage at each
    step.
  - *Phase C-FFI:* delete any residual transforms; the only ones that
    should remain are deliberate non-mirrors. With `EntryJoin` now a
    pure mirror, the doc's "investigate before deleting" sharpens to
    "after Phase C-FFI, zero hand-written transforms should remain — any
    survivor is a bug."

- **File line numbers in the FFI section may have drifted slightly.**
  `gv-ffi/src/types.rs` grew to ~1070 lines and the EntryJoin
  simplification touched lines around 644–670. Spot-check with
  `grep -n "^pub struct\|^impl From\|^pub fn ffi_"` before relying on a
  cited line number. Symbol names are stable.

- **`uniffi 0.31` proc-macro mode** is still the only configuration in
  use. The `[Remote]` and `custom_type!` machinery the Stage 4 plan
  relies on is supported on this version per Stage 1's hypothesis;
  Stage 3 didn't have occasion to verify it empirically.

- **Watch for partially-overridden trait surfaces on generic wrappers.**
  A late-Stage-3 regression (commit `423a727`) traced back to
  `*Column<DB>` impls only overriding `Type::type_info()` and inheriting
  the default `compatible()` (strict equality). sqlx's `DateTime<Utc>`
  overrides `compatible()` to accept multiple SQL types — TEXT in
  particular, since SQLite stores datetimes as TEXT despite the
  canonical `type_info` being DATETIME. The wrapper silently became
  stricter than its inner type, every `AllEntries` decode failed, and
  Swift's `try?` swallowed the error so the symptom was just "no
  entries visible." Stage 4's `custom_type!` declarations have an
  analogous shape (wrap a Rust type for cross-boundary conversion); if
  uniffi's custom_type macinery has any defaulted methods, delegate
  *all* of them to the inner type, not just the one mentioned in the
  example. Also: the leaf round-trip helper in
  `gv_sql/tests/columns_sqlite.rs` now uses `try_get` (the strict
  variant `#[derive(FromRow)]` generates) so this class of bug is
  caught at the leaf level next time.
