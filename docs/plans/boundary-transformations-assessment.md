# Centralizing Core-Model Boundary Transformations

*Assessment & refactor plan — Gainzville (`gv_core`, `gv-ffi`, DB layer)*
*Status: Stage 1 — proposal. Next: Stage 2 — assess against the codebase.*

## Objective

Centralize the transformations between Gainzville's core domain models and its two
boundaries — the **FFI surface** and **DB persistence** — so that each transformation
is defined once, lives in one place, and is covered by round-trip tests.

Today this logic is scattered:

- **DB conversion** is split asymmetrically across the read and write paths. The write
  path maps core types to simpler representations inline; the read path introduces
  `*Row` types (e.g. `EntryRow`) to read into and then converts back. The two
  directions are not a single coherent transform — the asymmetry is an accident, not
  a design.
- **FFI conversion** is a large set of hand-written `From` / `TryFrom` impls
  (core → FFI) plus free functions (FFI → core, because the orphan rule blocks
  `From` on core types). Each interleaves representational logic with field-by-field
  plumbing.

The two boundaries are independent in code but share one underlying shape. This
refactor makes that shape explicit and gives each boundary a single, tested home for
its transforms.

### Invariant the design relies on

All of these transforms are **total modulo bugs**. The loose representations (`String`
for a UUID, `i64` for a timestamp) *can* be malformed in principle, but a correct
transform always succeeds, and a round-trip is the identity:

```
core == decode(encode(core))
```

This is what justifies treating the transforms as mechanical plumbing rather than
domain logic, and what makes property-based round-trip tests the right safety net
(harness is an implementation choice — `hegel`, `proptest`, etc.).

## The unifying principle: leaf encoding vs. structural reshaping

Every boundary transform decomposes into two layers that are currently tangled
together:

1. **Leaf encoding** — representation changes for individual leaf types:
   `Uuid ↔ String`, `chrono` time `↔ i64`, `FractionalIndex ↔ String`, unsigned
   duration `↔` signed integer. This is per-`(leaf type, target)`. It carries the
   actual representational knowledge, and it is where all fallibility lives.

2. **Structural reshaping** — visiting fields, flattening nested structs, mapping
   `Option` / `Vec`, rebuilding the target struct. This carries zero domain
   knowledge; it is fully derivable from the struct's shape. It is the part Serde's
   derive macro generates for serialization.

Both refactors below are the same move: **pin leaf encoding to a small set of
per-leaf definitions, and make structural reshaping mechanical or derived.** DB and
FFI differ only in *which mechanism* supplies each layer.

## Proposal — DB boundary

**Structural layer: one `*Row` type per model, as a single bidirectional pivot.**
`EntryRow` is the DB-shaped `Entry`: same data, but nested structs flattened to
columns, and leaf types swapped for DB-representable newtypes. The transform
`Entry ↔ EntryRow` is written once and serves *both* read and write. This removes
today's read/write asymmetry.

**Leaf layer: sqlx's own per-database traits.** Encoding differences belong in
`Type<DB>`, `Encode<DB>`, `Decode<DB>` on leaf newtypes (e.g. `GvUuid(Uuid)` with
`Encode<Postgres>` native and `Encode<Sqlite>` via text). Because these traits are
per-database, the Postgres/SQLite divergence lives entirely in the leaf newtypes —
the `Row` struct stays DB-agnostic in *shape*, and likely a single `Row` type with
`#[derive(sqlx::FromRow)]` works for both backends.

**Division of labor:**

- `Row` struct's job: structural reshaping only (flatten nesting, hold leaf newtypes).
- Leaf newtypes' job: representational encoding, per-DB, via sqlx traits.
- `EntryRow ↔ database` (the `FromRow` read half and the positional-bind write half):
  mechanical, derived / macro-generated.
- The only hand-written, domain-aware code is `Entry ↔ EntryRow`.

**Conditions and caveats to carry into Stage 2:**

- The `Row` type only pays for itself if the `Row ↔ database` half is genuinely
  mechanical. If that half is still hand-written, the `Row` type is a layer for
  nothing.
- The `Row` type's necessity is **per-model**, driven by nesting. A model with no
  nested structs may not need one; a flat wrapper with `FromRow` could suffice.
- Updates are **whole-struct**: `Delta::Update { old, new }` carries both full
  structs, so the `Row` type covers insert, delete, and update **uniformly** — every
  operation is a whole-`Row` bind, no column-subset special-casing. The changed-column
  set is derivable by diffing `old` / `new`, but the DB writer does not need it:
  emitting `UPDATE ... SET <all columns>` is the mechanical default. Diff-derived
  partial `SET` clauses remain a possible later optimization, and are the only path
  that would reintroduce a column-level mapping.
- `old` additionally enables an optimistic-concurrency check: bind it into the
  `WHERE` clause so `rows_affected == 0` signals a failed precondition. This uses the
  *same* `Entry → EntryRow` transform — `new` for the `SET`, `old` for the `WHERE` —
  with no new machinery. Whether to do this is an orthogonal DB-semantics decision,
  independent of the centralization design.

## Proposal — FFI boundary

**Use uniffi remote types + custom types so `gv_core` stays uniffi-agnostic and the
hand-written transforms are largely replaced by declarations.**

uniffi 0.31 (confirmed) has the mature API for this:

- **Custom types** (`custom_type!`) handle the **leaf layer**: declare once that
  `Uuid` crosses the boundary as `String`, chrono time as `i64`, etc. On 0.29+ the
  conversion closures can be omitted where `From` / `Into` already exist.
- **Remote types** (`[Remote]` attribute / proc-macro form) handle the **structural
  layer**: declare the uniffi treatment of a type defined in a *non-uniffi* crate
  (i.e. `gv_core`) from *outside* that crate. Core needs no uniffi macros and no
  redefined structs.

**Reframing:** with these in place, uniffi *generates* the lift/lower code. The FFI
crate becomes a thin **declaration** layer, not a translation layer — most of the
current `From` / `TryFrom` impls and `ffi_*_to_core` free functions are expected to
**disappear**, not move.

**Dependency order:** a remote record requires every field type to already be
uniffi-known. So leaf custom types must be declared *first*; only then do the remote
record declarations for `Entry` etc. compile.

**Constraints and caveats to carry into Stage 2:**

- Remote support covers **records and enums only — not interface/object types**. Any
  core type that must be a uniffi `Object` cannot be remote.
- A remote record is a **shadow declaration**: the field list is re-stated in the FFI
  crate. Not a full redefinition + conversion function, but it must stay in sync with
  core — adding a field to `Entry` requires updating the remote declaration. Round-trip
  property tests are the intended guard against drift.
- uniffi records expose **all fields** by value. Core types relying on private fields
  or constructor-enforced invariants would have those bypassed. (`Entry` appears to be
  a plain data record; verify across all models.)
- This assumes the FFI type is a pure **encoding mirror** of the core type (confirmed
  as the intent). Verify no model currently has a *deliberately different* FFI shape;
  any that does cannot use remote types and keeps an explicit transform.

## Open questions — to resolve against the codebase (Stage 2)

### DB

- How many models cross the DB boundary? Is the `*Row` pattern already present, and
  is it consistent across them or ad hoc?
- Where does the current write-side leaf mapping live? How tangled is it with query
  construction?
- Which models have nested structs / collections requiring flattening? Which are flat
  enough not to need a `Row` type?
- *(Resolved.)* Updates are whole-struct deltas (`Delta::Update { old, new }`); the
  `Row` type covers all three operations uniformly. Remaining sub-question: emit
  full-column `SET` (mechanical default) vs. diff-derived partial `SET`, and whether
  to bind `old` into `WHERE` for optimistic concurrency — both orthogonal to the
  transform design.
- Is the multi-DB target (Postgres + SQLite) live now or aspirational? Which leaf
  types actually need per-DB encoding divergence?
- Are sqlx's macros (`query!` / `query_as!`) in use, or the runtime query API? This
  affects how mechanical the `Row ↔ database` half can be.

### FFI

- Inventory every core type crossing the FFI boundary: are they all records/enums, or
  are any objects (which cannot be remote)?
- Inventory every leaf type crossing the boundary that needs a custom type.
- Do any core types have private fields or constructor invariants that remote-record
  exposure would break?
- Is the current uniffi setup UDL, proc-macro, or mixed? (Affects the exact remote /
  custom-type spelling — verify against the 0.31 `proc_macro` docs.)
- How are errors handled across the boundary today, and does that interact with the
  custom-type fallibility path?
- Confirm no model has a deliberately non-mirror FFI shape.

### Cross-cutting

- Round-trip property tests: chosen harness (`hegel`), location, and whether a single
  test generic over a `RoundTrip` trait can cover all models given the meta-model
  architecture.

## Staged plan

1. **Stage 1 — this document.** Objective, proposals, constraints.
2. **Stage 2 — codebase assessment.** Answer the open questions above against the repo.
   Well-suited to a Claude Code agent briefed with this document. Output: a gap
   analysis — where the current code already fits the proposal, where it diverges,
   and any constraints that force adjustments.
3. **Stage 3 — DB refactor.** Leaf newtypes + sqlx trait impls first; then `Row` types
   and the single bidirectional transform; then collapse read/write onto it.
4. **Stage 4 — FFI refactor.** Leaf custom types first; then remote record/enum
   declarations; then delete the superseded `From` / `TryFrom` impls and free
   functions.

In both refactors the **leaf layer comes first**, because the structural layer depends
on it. Round-trip property tests should land alongside (or just before) each refactor
so regressions are caught immediately.
