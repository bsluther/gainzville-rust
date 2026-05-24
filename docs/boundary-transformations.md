# Boundary Transformations

Gainzville's domain types (`gv-core`) cross two boundaries: **DB persistence** (via
`gv-sql`) and the **FFI surface** consumed by the Swift app (via `gv-ffi`). This doc
describes how those transforms are organized. The guiding goal: `gv-core` stays free
of both `sqlx` and `uniffi`, and every type that crosses a boundary does so as a pure
encoding mirror, defined once and covered by round-trip tests.

## The unifying principle: leaf encoding vs. structural reshaping

Every boundary transform decomposes into two layers:

1. **Leaf encoding** — representation changes for individual leaf types: `Uuid ↔ String`,
   `DateTime<Utc> ↔ i64`, `FractionalIndex ↔ String`, etc. This is per-`(leaf type, target)`,
   carries the actual representational knowledge, and is where all fallibility lives.
2. **Structural reshaping** — visiting fields, flattening nested structs, mapping `Option`/`Vec`,
   rebuilding the target struct. This carries zero domain knowledge; it is derivable from a
   struct's shape.

Both boundaries apply the same move: **pin leaf encoding to a small set of per-leaf definitions,
and make structural reshaping mechanical or derived.** They differ only in *which mechanism*
supplies each layer.

The invariant that justifies treating these as mechanical plumbing: a round-trip is the identity,
`core == decode(encode(core))`. That is what makes round-trip property tests the right safety net.

## DB boundary (`gv-sql`)

`gv-sql` owns the entire DB boundary. `gv-core` has **zero `sqlx` dependency** (verify with
`cargo tree -p gv-core | grep sqlx` → no output).

- **Leaf layer: `*Column` newtypes** (`gv-sql/columns.rs`). Each wraps a core leaf type and
  carries the per-database sqlx trait impls (`Type<DB>`, `Encode<DB>`, `Decode<DB>`):
  `UuidColumn`, `DateTimeColumn`, `FractionalIndexColumn`, plus `EmailColumn`,
  `UsernameColumn`, `ActivityNameColumn` wrapping core's validated newtypes. The Postgres-vs-SQLite
  encoding divergence (UUID native vs. BLOB, `TIMESTAMPTZ` vs. RFC3339 TEXT) lives entirely here.
  The wrappers exist because of the orphan rule: core owns the leaf types, sqlx is foreign, so the
  impls can only live on newtypes `gv-sql` owns.
- **Structural layer: `*Row` types** (`gv-sql/rows.rs`). Each `*Row` is the DB-shaped mirror of a
  model — nested structs flattened to columns, leaf types swapped for `*Column`. `EntryRow`,
  `AttributeRow`, `ValueRow`, `UserRow`, `ActivityRow`, plus the read-only join shapes
  `EntryJoinRow`, `AttributePairRow`. The hand-written, domain-aware code is just `core ↔ Row`;
  the SQL is plumbing for already-encoded values.
- **Executors** (`gv-sql/sqlite/`, `gv-sql/postgres/`, behind `sqlite`/`postgres` features).
  Reads go through `query_executor.rs` (runtime `query_as::<_, Row>`); writes through
  `delta_executor.rs`. Reads and writes share the same `Row`, removing the old read/write
  asymmetry. Postgres writes keep compile-time `sqlx::query!` macros (hence the "postgres docker
  required at compile time" convention); SQLite writes use the runtime `.bind(...)` API. The read
  half is DB-generic via `#[derive(sqlx::FromRow)]`.

The write path is defined in core by the `DeltaExecutor<M>` / `AnyDeltaExecutor` traits
(`core/src/delta_executor.rs`); `gv-sql` provides `SqliteDeltaExecutor` and
`PostgresDeltaExecutor`. Errors cross back into core via the `DbErr` extension trait
(`core/src/error.rs`), which boxes any `std::error::Error` into
`DomainError::Database(Box<dyn Error>)` — type-erased so core never names `sqlx::Error`.

### Gotcha: delegate *all* defaulted sqlx trait methods on `*Column`

`sqlx::Type` has a defaulted `compatible()` that defaults to strict type-equality. sqlx's own
`DateTime<Utc>` overrides `compatible()` to accept multiple SQL types (notably TEXT, since SQLite
stores datetimes as TEXT). A `*Column` wrapper that overrides only `type_info()` silently becomes
*stricter* than its inner type and every decode fails. **Override every defaulted method, not just
the one in the example** (the `423a727` regression). The leaf round-trip tests in
`gv-sql/tests/columns_sqlite.rs` use `try_get` (the strict variant) to catch this class of bug.

## FFI boundary (`gv-ffi`)

`gv-ffi` exposes core types to Swift via uniffi 0.31 (proc-macro mode, no `.udl`). `gv-core` has
**zero `uniffi` dependency** (`cargo tree -p gv-core | grep -i uniffi` → no output). Types cross as
**pure 1:1 mirrors** — there is no parallel `Ffi*` struct for domain types; Swift sees `Entry`,
`Activity`, etc.

- **Leaf layer: `custom_type!`** (`gv-ffi/src/types.rs`). Declares once that `Uuid` crosses as
  `String`, `DateTime<Utc>` as `i64`, `FractionalIndex`/`Email`/`Username`/`ActivityName` as
  `String`.
- **Structural layer: `#[uniffi::remote(Record)]` / `#[uniffi::remote(Enum)]`**. Declares uniffi's
  treatment of a type defined in `gv-core` from *outside* that crate. uniffi generates the
  lift/lower code; the FFI crate is a thin *declaration* layer, not a translation layer. ~35 domain
  types are declared this way.

A remote record exposes **all fields by value**, so it only works for types that are pure data
records. Any core type with constructor-enforced invariants would have them bypassed and needs a
hand-written transform instead. The two intentional survivors in `types.rs` are `FfiError`
(flattens every `DomainError` to a string) and the `parse_uuid` / `parse_timestamp_ms` helpers used
by the forest helpers in `core.rs` that take/return raw `String` IDs.

### uniffi gotchas

- `custom_type!` on a foreign-crate type (e.g. `uuid::Uuid`, `gv_core::validation::Email`) needs the
  `remote,` keyword inside the macro body, or it errors with `type parameter "UT" must be used...`.
- Generic types like `DateTime<Utc>` can't be parsed by `custom_type!` directly (`Custom types must
  only have one component`). Use a type alias: `type UtcDateTime = DateTime<Utc>;` then
  `custom_type!(UtcDateTime, i64, { remote, ... })`.
- `[uniffi::remote(...)]` inherits core's exact variant shapes. Tuple-style core variants
  (`NumericValue::Exact(f64)`) reach Swift without field labels (`.exact(x)`, not `.exact(value: x)`),
  and variant names that differ from their inner struct (`Action::CreateActivity(CreateScalarActivity)`)
  surface honestly as `.createActivity(CreateScalarActivity(...))`.

## Crate separation

`gv-ffi` deliberately does **not** depend on `gv-sql`. The two boundaries have nothing to do with
each other, and keeping them in separate crates with no edge between them makes that physical rather
than conventional. Dependency graph:

```
gv-core        — pure domain; no sqlx, no uniffi
generation  -> gv-core
gv-sql      -> gv-core, sqlx        (features: sqlite, postgres)
gv-client   -> gv-core, gv-sql      (sqlite)
gv-server   -> gv-core, gv-sql      (postgres)
gv-ffi      -> gv-core, gv-client   (no gv-sql)
ivm         -> gv-core
```

## Round-trip tests

The transforms are total modulo bugs, guarded by `decode(encode(x)) == x` round-trip tests built on
the `generation` crate's `Arbitrary` infrastructure (`gv-sql/tests/`). Adopting `hegel-rust` for
full property-based testing is intended future work but is not yet wired in; `proptest` is declared
but currently unused.
