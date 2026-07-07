# Analysis Query Model — Design Takeaways

> **Status: design-discussion notes (captured 2026-07-04/05).** Conclusions from a working
> session that drilled into Malloy's serializable structures (`malloy-interfaces`) to decide
> how GV should represent, schema-type, and execute analysis queries. Provisional — "decided"
> means "current strong lean," not shipped.
>
> Companions: [`analysis-malloy-evaluation.md`](./analysis-malloy-evaluation.md) (the
> expressiveness evaluation — the direction being pursued), [`malloy-core-model-map.md`](./malloy-core-model-map.md)
> (map of Malloy's source), [`analysis-design-1.md`](./analysis-design-1.md) /
> [`analysis-design-2.md`](./analysis-design-2.md) (earlier full designs — some detail here
> supersedes or sharpens them), [`analysis-features.md`](./analysis-features.md) (the feature bar).

---

## 1. Representations: one persisted spec, everything else derived

- **Only the `QuerySpec` is persisted.** Its output schema and its execution are *derived*,
  never stored. This holds across all prior designs (design-1 "persist the surface, derive the
  IR"; design-2 D6) and is the single load-bearing structural commitment.
- **Carrier ≠ IR.** Two different axes people conflate:
  - **IR** = a description of the *program* (the query — the ordered operations). Program-side.
  - **Carrier** = the *data* value flowing through execution — a **`Relation` = `{ schema, rows }`**.
    Data-side. The `Relation` handed between steps is an "intermediate result," but it is *not*
    the intermediate representation of the program.
- **Malloy's two *serializable* forms collapse to one for us.** Malloy has a stable interface
  layer *and* a SQL-compiler IR; the second exists only for SQL generation. We interpret
  in-memory, so we drop it → one persisted representation.
- **A query is one pipeline evaluated at two levels** (the schema/instance duality):
  - **type-level** `derive_schema(Source → … → O) → OutputSchema` (static, no rows)
  - **value-level** `run(Source → … → O) → Result` (dynamic, needs rows)
  
  Each operator therefore has a type-level face (`Schema → Schema`) and a value-level face
  (`Relation → Relation`). The `(Type, Value)` pair carried at each step **is just the
  `Relation`** (schema-tagged body); it is not a new struct.
- **`ResolvedQuery` is optional.** "Resolve against the catalog" = bind each `FieldRef` to its
  catalog type/capability. The value-level walk needs those types anyway, and `derive_schema`
  already computes per-node schemas — so a `ResolvedQuery` is just *the type-level walk kept
  instead of recomputed*, so `run` reads it. Start with the two functions; reify a
  `ResolvedQuery` later only as a caching/clarity optimization.
- **"Provably identical" = literally the same function.** Schema-derivation and execution must
  get each node's type from the *same* schema function (never sniff types from the data), else
  the declared schema and the runtime output can diverge and the chart lies. Whether the result
  is stored/reused is then pure caching.

## 2. The persisted `QuerySpec` is Malloy's *surface*, not lowered primitives

- **Persist the surface, derive the IR.** Lowering a `{ source, filters, dimensions, measures }`
  spec to relational primitives (`GroupBy`/`Aggregate`/`Join`/…) is one-way and lossy —
  reconstructing user intent from the primitives is decompilation. So the serialized form speaks
  the **Malloy-style domain vocabulary**; the relational pipeline is a derived, throwaway
  execution detail.
- **"Malloy-style" means *altitude*, not *flatness*.** A single flat `{ filter, dimensions,
  measures }` record can't express the re-graining use cases (e.g. "average climbs per session,"
  "sessions with ≥10 climbs per week"). Malloy handles those *while staying domain-level* via
  segments/`nest`. So the surface stays high-altitude (measures/dimensions/nest) but is
  multi-stage — it just never drops to `GroupBy/Join` primitives.

## 3. Pipelines: segments, arrows, `nest`

- **Use a flat `Vec<Segment>`, not Malloy's recursive `arrow`.** Malloy models `A -> B -> C` as
  nested `arrow` nodes, but an arrow is *function composition* — both slots sit on one stream
  (source's output = view's input), it's associative, so any nesting **linearizes** to a
  sequence. A flat `Vec<Segment>` is a lossless simplification.
- **The only thing that forces a genuine (non-flattenable) tree is a truly binary operator with
  two independent inputs — a join/union.** That's "chain-now, tree-ready": arrows never make a
  tree; joins do. Flat costs nothing until a join lands.
- **Malloy's `arrow` generality (a stage can be a named-view reference or a `+ {}` refinement)
  is exactly the deferred named-fragment/semantic-model reuse.** With mid-pipeline references and
  refinements removed, every arrow's left side is either the base source (→ the `source` field)
  or the previous segment (→ adjacency in the `Vec`).
- **A `Segment` is one *grain transformation* — a reduce *or* a projection**; recursion reappears
  *only* through `nest`. Sketch:

  ```
  QuerySpec { source, pipeline: Vec<Segment> }
  Segment   { operations: Vec<Operation> }          // one stage: reduce (groups) or project (grain-preserving)
  Operation = Filter | GroupBy | Aggregate | Having | OrderBy | Limit | Nest | Calculate | …
  Nest      { name, pipeline: Vec<Segment> }         // the one recursive point
  ```
- **Reduce vs project is exclusive per stage, and it's the grain rule, not taste.** A stage either
  collapses rows (reduce = `group_by`+`aggregate` → keys+aggregates, one row per group) or preserves
  them (project = select/derive columns, one row per input row) — never both, because a stage is
  exactly one grain change (the SQL rule: with a `GROUP BY`, every output column must be a key or an
  aggregate). Malloy's compiler IR types this (`QuerySegment.type = 'reduce' | 'project' | 'partial'`);
  its **stable interface models only the reduce family** (`ViewOperation` has no `select`), so at the
  layer being ported the distinction is latent. **GV's choice: enforce the grain invariant either
  structurally (typed `ReduceSegment | ProjectSegment`) or in the resolve/schema pass** (a segment
  with a `Group` must output only keys+aggregates) — the usual "types where cheap vs resolve pass for
  the rest" fork.
- **Malloy executes over a flat pipeline too.** Its compiler IR is `Query.pipeline: PipeSegment[]`
  (a flat array); the recursive `arrow` is purely the authoring surface. So the flat `Vec<Segment>`
  matches what Malloy actually runs, not just a simplification of it.

## 4. The carrier type

- **The carrier is the *inter-segment* type: a flat `{ schema, rows }`.** groupBy does *not*
  break it, because **the segment (not the individual op) is the unit of closure**:
  `Segment: Relation → Relation` (holds whether the segment is a reduce or a projection).
  groupBy's nested `{ key → [rows] }` state lives *inside* a reduce segment's evaluation and never
  escapes; the segment's *output* is a flat relation (one row per group for a reduce, one row per
  input row for a projection).
- **`nest` is the one wrinkle** (deferrable): a segment with a `nest` yields rows whose *cell* is
  a sub-relation. Two options, affecting only nest:
  - **(a) nested cells** — a `Cell` can be a `Relation` (Malloy's real model). Keeps nest first-class.
  - **(b) flatten to long/tidy at the segment boundary** — `{v4:{send:7}}` → rows
    `(grade, outcome, count)` (design-2's approach). Keeps the carrier strictly flat.
  
  Since the viz contract already commits to tidy/long, **flatten by default**; reach for nested
  cells only if multiple sibling nests at different grains show up.
- **Re-grain ≠ nest.** Re-grain = the *main stream's* grain changes stage-to-stage, output is one
  flat table at the final grain. Nest = the main grain is unchanged, a *finer* grain is held
  *inside* each row. `{v4:{send:7,attempt:2}}` is a row (v4) carrying a small `(outcome, count)`
  table — "one row per grade" means one *output* row per grade; the N climbs are collapsed inputs.

## 5. Filters — the many faces, and what GV keeps

- **`where` and `having` are the same schema-preserving operation**; only position distinguishes
  them (pre- vs post-aggregation). Type-level: `filter: Relation → Relation`, output schema =
  input schema.
- **Collapse Malloy's split predicate into one structured `Predicate`.** Malloy carries filters
  two ways — a tiny structured shell (`LiteralEqualityComparison`, `FilterStringApplication`) plus
  a separate *typed text sub-language* (`malloy-filter`: per-type grammars holding `and/or/not`,
  ranges, `contains`, temporal ops). That split exists because Malloy *authors filters as text*.
  GV is structural (no text grammar), so it uses one recursive structured `Predicate` with the
  boolean logic inline — which is what design-2 already does.
- **`FilterCompare`, not a general `Compare`.** Comparison appears only inside filter predicates,
  so name it for that use and constrain its `lhs` to comparable-producing things (a `FieldRef`,
  later maybe a time-truncation/measure) — don't build a general-purpose compare. Generalize if a
  second consumer (e.g. a computed boolean) ever appears.
- **Operand shapes** — think "how many sides read the row":
  - **reference op literal** (one row-reading side) — the norm (`activity in {…}`, `weight >= 100`).
  - **reference op reference** (two sides) — real and legitimate (`actual > planned`), but **deferred**;
    the generalization is additive (RHS widens from a literal to a comparable). Not clearly in the
    feature bar.
  - **literal op literal** (zero sides) — degenerate (constant across rows); never represent it.
  
  Malloy's stable model is always expression-op-literal; design-2's `Cmp { lhs, op, rhs: ScalarLit }`
  matches.
- **The filtered aggregate** (`count() { where: … }`, Malloy's `FilteredField`) is the *same*
  predicate scoped to one measure — reuse the filter op, embedded in an expression.
- **Filters-as-values** (Malloy's `FilterExpressionType`/`FilterExpressionLiteral`, for
  `filter<string>` source parameters) are part of the deferred parameterized-source feature. Skip.

## 6. Cross-cutting design principles

- **Unroll generality: use-case enums for membership, leaf traits for behavior.** Prefer
  purpose-specific enums (`FilterComparable = FieldReference | TimeTruncation`) over one reused
  general `Expression`, to make illegal shapes unrepresentable. But:
  - **Membership stays in the enum** — closed, serializable, exhaustively-matchable (this is the
    persisted vocabulary; the same reasoning as "initial encoding, not tagless-final"). Do *not*
    define "what's allowed" via a trait (open set, fights serde).
  - **Behavior goes on a leaf trait** — `output_type(&Schema)`, `eval(…)` — implemented per leaf,
    since expression semantics are context-independent; each use-case enum is thin dispatch. This
    is what avoids logic duplication. A *use-case* trait earns its place only if role-specific
    shared behavior appears across several enums.
- **No hard rule; accept some representable-but-invalid states.** Precise types are the "type
  system" tier; the **catalog resolve pass** is the "mutator" tier for this subsystem (analogous
  to how GV mutators enforce rules the type system can't). Precise enums remove *structural*
  illegality but not *semantic* (data-dependent) illegality — resolve/validate doesn't disappear,
  it shrinks. Don't split into six near-duplicate enums for one rare structural invariant; let the
  resolve pass catch it.
- **Catalog resolution is one pass with three consumers.** `resolve(fieldref, catalog) →
  { type, capability, value-domain }` powers (1) output-schema derivation, (2) staleness/validation,
  and (3) the authoring UI's legal-op/value-picker offering (e.g. a Select gets `<=`/range ops only
  if its config marks it ordered; the picker is the config's options in config order). Same
  function; validation *rejects* illegal, the builder *offers* legal.
- **`FieldRef` stays identity-only; type/capability/ordered-ness are derived fresh, never stored.**
  So config drift behaves correctly under GV's additive-only config discipline: adding Select
  options grows the dropdown; marking a Select ordered adds comparison ops — neither strands a
  saved filter. Storing capability on the ref would silently mismatch after such a change.
- **The RHS of a filter is a literal *of the LHS's resolved type*** (an option id for a Select, a
  number for Numeric) — validation checks both op-legal-for-type and RHS-compatible-with-type.
- **Legality/typing logic lives in `gv-core`**, exposed as a capability descriptor; Swift renders
  pickers from it (thin FFI — same pattern as the `DistinctTextValuesForAttribute` autocomplete).

## 7. Reading Malloy's source (aside)

- `malloy-interfaces/src/types.ts` is **generated** from the hand-written IDL
  `thrift/malloy.thrift`; the generated `.ts` strips comments, so **read the `.thrift` for intent**
  (it carries the design `TODO`s). See `malloy-core-model-map.md`.
- `Reference` (name + qualifying `path`) is general — it names fields, sources, views, or queries.
  Its optional `parameters: [ParameterValue]` are **arguments to a *parameterized source*** (the
  `source: x(F::type) is …` feature) — a named-argument call, part of the deferred reuse layer.
