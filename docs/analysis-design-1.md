# Analysis / Query System — Design Notes

**Status: exploratory.** This captures an early design discussion about the analysis
& visualization subsystem (`core/src/analysis.rs`). Decisions here are provisional —
"settled" means "current strong leaning we'd need a reason to overturn," not "shipped."

This doc doubles as a **primer**: it's meant to bring a research agent (e.g. one
studying the [DataFusion](https://github.com/apache/datafusion) codebase) up to speed
quickly. See the [DataFusion research](#for-a-datafusion-research-agent) section at the
end for specific questions to investigate.

---

## 1. Design goals

We want users to build **dynamic data analysis and visualization** from their own
training data — not a fixed set of reports, but a system where the *user* specifies what
to compute and how to show it.

The workflow is a four-stage pipeline:

```
UI  →  AnalysisSpec  →  Execute  →  Viz
```

- **UI** — the user builds a query in **domain terms** (activities, entries, attributes),
  not SQL and not raw IDs. (UX is out of scope for these notes.)
- **`AnalysisSpec`** — a stored, structured, domain-level representation of the query (the
  **surface** — §3). Must be **saveable**, **serializable**, **composable**, and
  **round-trippable** back into the UI. *Lowers* to the relational IR (§4) for execution.
- **Execute** — run the plan against data, producing a result table.
- **Viz** — present the result: chart, table, or text. All three consume the same result.

### Driving use cases

1. Number of climbing sets per day (where "climbing" = a set of `activity_id`s).
2. Max YDS grade *sent* per week (max `YDS Grade` where `Outcome ∈ {Sent, Flash, Onsight}`).
3. Number of climbing **sessions** per week (a "session" is user-defined — e.g. any 4-hour
   period containing >10 climbing entries, or having `workout type = climbing`).
4. # of sport climbs per week.
5. # of boulders per week.
6. Miles ran per week.

These are all the same OLAP shape: **a measure, cut by dimensions, under filters.**

### Constraints

- **Offline-first**, small per-user data (one person's training log).
- **Multiple backends**: in-memory (the offline client, and our deterministic-simulation
  reference semantics), SQLite (client), Postgres (server).
- The plan is **domain-level but ID-referenced** (stores `activity_id`/`attribute_id`,
  not names — names change, IDs don't), consistent with how `Action`/`Query` already work.
- **Avoid reimplementing SQL.** Build a *vertical slice* of relational algebra sized to the
  use cases, grown one operator at a time. The architecture is small and necessary; the
  over-engineering risk lives entirely in letting the operator *set* sprawl toward generality.

---

## 2. The space this lives in

### What we're building (and what we're not)

This is a **query system**, but only part of one. Mapping to the classic SQL pipeline
(text → lex → parse → AST → semantic analysis → logical plan → optimizer → physical plan
→ execute):

| Stage | Us? |
|---|---|
| Lexer / parser / grammar | **Skip** — the UI builds the plan structurally; we never accept query text. |
| AST / IR | **Keep** — two layers: the persisted **surface AST** (`AnalysisSpec`, §3) and the **relational IR** it lowers to (§4). |
| Semantic analysis / type-check | **Keep (light)** — validate the plan against the live schema. |
| Optimizer (cost-based) | **Skip** — delegate to SQLite/Postgres, or evaluate naively in memory. |
| Execution | **Keep** — by *lowering* the IR to a backend, not a hand-rolled engine. |

So the precise name for what we're building is a **query builder** (programmatic
construction of a logical plan, no parsing — like jOOQ / LINQ / SQLAlchemy Core /
sea-query) producing a **logical plan / IR**, plus the **backends** that give it meaning.
We are *not* building the optimizer or physical planner. It's an **embedded/internal DSL**:
abstract syntax, no concrete syntax.

### The conceptual frameworks

- **Relational algebra** (Codd) — operators over *relations* (sets/bags of typed tuples):
  selection, projection, join, grouping+aggregation, sort, limit. Defining property:
  **closure** — every operator takes relations and returns a relation, so they compose.
  This is the "logical plan" in every real engine.
- **Pipeline / collection calculus** (LINQ, dplyr, KQL, Mongo aggregation, PRQL) — operators
  over collections (`filter`/`map`/`groupBy`/`reduce`) composed as a chain. Programmer-friendly.
- These are **complementary, not rival**: the **pipeline is the surface** the UI builds;
  **relational algebra is the IR** it lowers to. They answer different questions — the
  pipeline is a *composition shape*, relational algebra is a *carrier + operator set*.
  (Refined in §3: our *surface* is actually a **fixed-shape OLAP spec** — measures /
  dimensions / filters — not a free pipeline; the free `Relation → Relation` algebra is the IR.)
- **OLAP / dimensional model** (Kimball star schema, OLAP cubes, MDX — *not* a Malloy
  invention, though Malloy/Looker/Cube/Tableau all use it): **measures** (what you
  aggregate) cut by **dimensions** (what you group/slice/filter by). Every driving use case
  fits this. In our *dynamic* system, measure-vs-dimension is a **role a field plays in a
  given query**, not an intrinsic tag (YDS Grade is a measure when you `max` it, a dimension
  when you group by it).

### Closest existing category: the **semantic layer / headless BI**

The thing we're describing — *store a query semantically, compile it to SQL* — is exactly a
**semantic layer** (Cube.dev, dbt MetricFlow, LookML/Looker, Malloy, Superset). Right
concept, **wrong form factor**: server-side, YAML/DSL-configured, assume a warehouse star
schema, built for analysts — not embeddable in an offline-first Rust client where end users
dynamically build and save per-user viz queries over an EAV/forest domain model. *Study
them for the model; don't adopt them.* No existing tool drops into our whole loop, because
the **stored, domain-typed, user-owned, composable plan is specific to our model** — nobody
hands you that part. We build it; the leverage is keeping it small.

---

## 3. The surface layer (`AnalysisSpec`)

The persisted, UI-facing layer — what the user actually builds and what we save. Distinct
from the relational IR (§4), which is its *lowering target*. (Naming: the persisted artifact
is an **`AnalysisSpec`**; the layer is "the **surface**" / domain surface. We avoid
`Query`/`QuerySpec` — `Query`/`AnyQuery` is the existing read-path and would collide.)

### Two layers: persist the surface, derive the IR

- **Surface (`AnalysisSpec`)** — domain-level, OLAP-shaped; what the UI builds and what we
  **persist**.
- **Relational IR (§4)** — the unary relational chain it **lowers to**; the execution model.
  May be ephemeral (derived at execute time), like the schema (§5 duality).

**Why persist the surface, not the IR:** lowering (desugaring) is **one-way and lossy**.
Once "filter by Climb Outcome" becomes a join+filter, the *intent* ("this is an attribute
filter," "this is a 1-week time bucket") is dissolved — reconstructing it from the relational
form is decompilation (ambiguous; many plans map to the same UI, many map to none). Round-trip
works by **keeping** the surface, never reconstructing it. Same move as schema-derivation
(§5): persist the high-level program; derive the low-level forms. *(This supersedes the earlier
"v1's saved plan IS the relational chain" lean.)*

### The surface is a fixed-shape OLAP spec, not a free algebra

The free `Relation → Relation` operator algebra lives in the **IR** (§4). The **surface** is
constrained — a fixed shape matching the UI sketch (`SELECT measure / WHERE filter / GROUP BY
dimension`):

```
AnalysisSpec = { source, filters: [Filter], dimensions: [Dimension], measures: [Measure] }
```

Tradeoff: fixed-shape is simpler and round-trips trivially, but can't express arbitrary
multi-stage transforms; the general IR underneath is the escape hatch. Sufficient for all six
use cases.

### Source vs result: the type that governs composition

The carrier the surface vocabulary operates on is **more constrained than a general relation**
— it's *entry-shaped* (`E`): each row is an entry, so the **domain attribute vocabulary
applies**. Aggregation re-grains to a general result (`R`), where attributes no longer apply
(a `(week, count)` row has no Climb Outcome). This is the **source-vs-result** distinction
(Malloy/BI), and it's the **GROUP BY boundary** / **grain** change / **WHERE-vs-HAVING** split:

- **filters / derives**: `E → E` (stay entry-shaped; pre-aggregation = WHERE).
- **measures (aggregation)**: `E → R` (the boundary; a post-agg filter would be HAVING — a
  different carrier, deferred).

**Measure-presence is the type-determiner:**

> No measure ⇒ `E` (a reusable **source**). Has a measure ⇒ `R` (a terminal **query/result**).

So saved specs bifurcate: **sources** (no measure — reusable as another spec's `source` slot
via `PlanRef`) and **queries** (have a measure — terminal, feed viz). Whole-query reuse =
**source reuse**. (At the IR level this dissolves: `E` and `R` are just relations with
different schemas. It's a *surface-level* type discipline.)

### The vocabulary

| Element | Role | Type / effect |
|---|---|---|
| **source** | establishes `E` | the entry universe, or a `PlanRef` to another `E`-spec. Sets entry grain + attribute vocabulary. |
| **filter** | narrow `E` | `E → E` predicate over a `FieldRef` (WHERE). e.g. `AttributeFilter`, `ActivityFilter`. |
| **dimension** | group key | `row → key`; a `FieldRef` + a grouping transform yielding a **discrete** key. |
| **measure** | aggregate | `[Row] → Scalar` = `(aggregate, FieldRef?)`; the `E → R` transition. |

Output schema = (dimension columns ∪ measure columns).

**Source** is the simplest element by design: `Source = Entries | PlanRef(E-shaped spec)` — the
owner's entry universe, or another saved *source-shaped* (measure-less) spec. Three things are
**defaults baked into the impl**, not query parameters: it **excludes template entries**
(`is_template`), includes **only completed entries** (`is_complete = true` — comparing planned
vs actual completions is a deferred extension), and is **implicitly owner-scoped** (like every
query). Activity scoping is **not** a Source parameter — it's an `ActivityFilter` (it changes
neither grain nor vocabulary; "climbing" is just a source-shaped spec = `Entries +
filter(activity ∈ {…})`, reusable via `PlanRef`). The attributes pivoted in are **derived from
the FieldRefs referenced downstream**, not declared on Source. So the *design* is trivial; the
under-the-hood work (pivot, forest-derived columns) is demand-driven, and the only genuinely
open piece is the **catalog** (where each column physically lives) — which only the SQL backend
needs, so it's deferred with SQL lowering.

**Dimension = field + transform → discrete key.** A dimension is a function `row → key`; rows
with equal keys form one group (you compute a canonical key per row, you don't enumerate
buckets). Categorical fields (activity, select/text attrs) group by **identity**; continuous
fields (time, numeric) need a **bucketing transform** (time truncation now; numeric binning /
date-part later) — the discrete-vs-continuous split is read off the *resolved* type. A
**by-attribute** dimension pivots that attribute in (shares machinery with `AttributeFilter`);
**by-field** (activity, time) doesn't. A **multiselect** dimension needs `unnest` first (an
entry tagged `[a,b]` lands in *both* the `a` and `b` groups). Multiple dimensions → group by
the combination → a tidy table the viz maps to channels (x=week, color=activity). *Prior art:*
Looker `dimension_group` timeframes, Tableau date granularity.

> **Dimension vs separate queries.** When the split is over *one field's values* ("max grade
> by activity"), prefer a single dimension — dynamic, one artifact, tidy result. Reserve
> *separate queries combined at the viz layer* for genuinely different measures/sources
> (e.g. overlay "miles run" and "sessions" on a shared time axis).

**Measure = aggregate over a group** (`[Row] → Scalar`, *not* `Row → Scalar` — that's a
per-row derive). `count` is the one field-less aggregate (counts rows); `sum/max/min/avg`
take a `FieldRef`. Output column type via the measure's own type function (`count → int`,
`max → field's type`, …). Dimensions and measures arrive together at the GROUP BY — a
*source* has filters but no dimensions/measures. *Per-measure filters* ("filtered aggregates"
— `COUNT(*) FILTER (WHERE …)`, Malloy filtered aggregates) are **deferred**: query-level WHERE
covers single-condition queries; per-measure filters arrive only when one query needs measures
with *differing* conditions.

### `FieldRef` — the shared leaf

`FieldRef` is referenced by filter, dimension, *and* measure — the one primitive that names
"which domain quantity." **Identity only** (which field); the **type/capability is resolved
from the catalog, never stored** (same schema-derivation rule — an attribute's type, or a
Select's ordered-ness, can change). Resolution feeds everything downstream: the dimension's
discrete-key check, the measure's "can I sum this," the filter's "is this predicate valid for
this type."

```
FieldRef = Entry(EntryFieldRef) | Value(ValueFieldRef)

EntryFieldRef = Activity | IsComplete | IsSequence | Start | End | Duration
              | DerivedCanonicalInstant   // a *total* instant: when the entry has none,
                                           //   derive from ancestors (sequences carry
                                           //   temporal info downward) — needed so every
                                           //   entry lands in a time bucket
ValueFieldRef = { attribute_id, aspect: Plan | Actual, range_fold: Min | Max | Mean | … }
```

- **`attribute_id`, not a value id** — a `FieldRef` names a *column* (the attribute, pivoted
  in across all entries), not one entry's single cell.
- **A value resolves to a scalar in stages:** `attribute_id` (pivot) → `aspect` (Plan/Actual)
  → `range_fold` (collapse a *range* value to a point). This is the per-row counterpart of a
  measure's across-row fold — note the **two-level Min/Max/Mean**: `range_fold` folds *within
  one value*, the measure aggregate folds *across rows of a group*. Same names, different levels.
- **`range_fold` is conditional** on the resolved type being ordered/range-capable (Numeric,
  Select). Carry it always; validation ignores/rejects it for non-orderable types. (The sketch's
  `OrderedValue` enum is this *capability* — it belongs to the resolution layer as something
  like `is_ordered(attribute, catalog)`, not stored on the ref. A Select's ordered-ness is a
  config/runtime property, which is exactly why it can't be baked into the persisted ref.)
- **Temporal fields are single-aspect** — no Plan/Actual (that split exists only at the
  attribute/value level).
- **Forest fields omitted for now** (templates, parent / is-root). Additive later (a new enum
  variant is serialization-compatible). `DerivedCanonicalInstant` is already forest-derived, so
  forest fields can just be `EntryFieldRef` members; a separate `ForestFieldRef` would be
  taxonomy, not necessity. The use case that pulls them in is **grouping by container** (see
  containment dimensions, below), not counting — counting is better as a flat entry filter.

### Not the deferred semantic model

Keep this line sharp — it's the over-engineering guard:

- **`AnalysisSpec` (surface)** = *one query's* measures/dimensions/filters — basically the UI's
  data model, serialized. **Needed now.**
- **Semantic model** = a *named, reusable library* of measures/dimensions/filters (Malloy/LookML
  — `send := Outcome IN {…}` referenced by name across many queries). A registry + symbol
  resolution + dependency tracking. **Deferred.** Trigger to revisit: a user re-specifying the
  same fragment repeatedly. (Reuse is mostly **end-user**-facing here, since the user authors
  their own queries — there's no separate "data modeler" role.)

`PlanRef` gives whole-*relation* reuse (a saved source's output rows feed another spec) and is
in scope; named-*fragment* reuse (a reusable predicate/measure/dimension *expression*) is the
deferred part.

### Open / unsettled

- **Containment dimensions (recursive "contains") — UNSETTLED.** Grouping entries by the
  **session that (transitively) contains them**: "average sets per climbing session," "max
  grade per climbing session." It's still a dimension (`row → key`, key = containing-session id),
  with the key derived by walking ancestors to the **nearest one matching a session predicate**
  (which reuses the filter vocabulary: `is_sequence && activity/attribute …`). *Transitive*
  because the forest is multi-level (session ▷ set ▷ climb — sets are themselves sequences).
  Resolved at the `Source` (forest layer — `FindAncestors` exists; an in-memory ancestor walk is
  trivial, SQL would need a recursive CTE), so the downstream chain stays flat. Gated on pinning
  **what a session is**: *structural* (a sequence matching a predicate) vs *windowed* (>10 climbs
  / 4h). Both reduce to "assign each entry a session-id key," differing only in key-derivation;
  grouping downstream is identical.
- **Whether the IR is a separate reified type or just interpreter behavior over surface nodes**
  — the encoding question. Persisting the surface is forward-compatible either way; lean: lower
  surface → relational primitives *once*, then interpret/codegen the primitives (so the
  schema-fn / interpreter / codegen are each defined once on the small primitive set, and sugar
  like time-bucketing lives only in the lowering pass).
- **`EntryFieldRef` specifics** — canonical-instant semantics (is it `Start`, or a policy like
  midpoint / sequence-start?) and whether the field set is complete.

---

## 4. Architecture: settled, leaning, open

This section is the **relational IR / execution model** — the *lowering target* for the
surface (§3), and the backends that run it.

### Settled (strong leanings)

- **Reified IR (data, not behavior).** The plan is an inspectable data structure, not a
  bundle of closures. This is *forced* by the requirements: a SQL string can't be
  saved-and-round-tripped, validated against a mutable schema, composed, or lowered
  per-dialect. Implies an **initial encoding** (an `enum` + passes over it), not
  tagless-final (operators that know how to run themselves).
- **Carrier type = a Relation** — a bag of *typed rows over a computed schema*. Not
  `[Entry]` (domain objects can't describe their schema after a `group`/`aggregate`), and
  not nested collections. The carrier is the same after every operator, which is what lets
  operators compose (closure). `[Entry]` is at most the *source* schema, never the carrier.
- **Flat relational model (1NF).** We flatten to rows and use joins/pivots to pull in
  nested data, rather than a nested-collection (monoid-comprehension) calculus. The forest
  hierarchy and EAV/list nesting are handled by **boundary operators at the source**
  (`pivot`, `unnest`), not carried through the pipeline. All six use cases reduce to flat
  measure-over-dimensions tables.
- **Operators are `Relation → Relation`** (preserve closure). Implement only **unary**
  operators for now (so the plan is effectively a **chain**), but *type* them as
  `Relation → Relation` so a **tree** (binary `Relation × Relation → Relation`: join, union)
  is a **non-breaking later addition**. Chain ⟺ all operators unary; a tree is forced only
  by a genuinely binary operator.
- **Recursive plan representation** (a `Box`/`Arc` tree), with only unary variants populated
  now. This is the one thing we pay upfront to keep the door open for binary nodes — small
  ceremony (recursive folds vs linear), but adding `Join`/`Union` later is then just a new
  variant, non-breaking to every existing pass. (A flat `Vec<Op>` would be simpler now but
  the Vec→tree migration later would touch every pass.)
- **Small, closed, lazily-grown vocabulary.** ~6–8 operators (`filter`, `derive`,
  `group`+`aggregate`, `unnest`, `window`, `sort`, `limit`), ~5 aggregates
  (`count`/`sum`/`max`/`min`/`avg`), a small predicate set. **No general scalar-expression
  sublanguage** (no arbitrary arithmetic/`CASE`/string functions) and **no general `FROM`**
  (one parameterized source). Add an operator *because a use case needs it*, never because
  it's elegant.
- **Each operator has three faces** (see [the duality](#5-the-schemainstance-duality)):
  a **type/schema function** (`Schema → Schema`, static), an **eval function** (in-memory,
  dynamic), and a **codegen function** (→ SQL, static). The **schema function is the spine**:
  it's *shared* by validation and SQL lowering, and the SQL/eval functions hang off it.
  (Our `analysis.rs` `Gte { apply, clause }` sketch already has the eval + SQL faces; it was
  missing the schema/type face — the one that validates.)
- **Start with the in-memory interpreter.** Data is small and DST wants a reference
  semantics anyway. SQL lowering is a **later** performance/server optimization, possibly not
  needed for v1 at all.
- **Store IDs, display names.** The plan references `activity_id`/`attribute_id`; the UI
  resolves IDs↔names purely for display. "Domain-level" (entities, not SQL tables) ≠
  "domain-named" (labels).
- **Composition = a `Source` that references another saved plan** (`PlanRef(plan_id)`),
  like a view/CTE. The sessions use case ("count climbing sessions per week") is a chain
  whose source is the saved "climbing-sessions" plan. Composition needs no new machinery and
  stays a chain; only *combining two* plans would force a tree. (See §3 source-vs-result for
  *which* specs are reusable as a source: the measure-less, `E`-shaped ones.)
- **Windows, not joins, are the next reach.** "Compare to neighbors" / sessionization
  (gap-based sessions, "4-hour window count") are **window functions** — unary
  `Relation → Relation` (same rows, +1 column), not joins. Many computations have both a
  self-join (tree) and a window (chain) formulation; the window one is simpler and keeps us
  in a chain.
- **Naming, to avoid domain collisions:** the relational column concept is `Column` (not
  `Attribute` — that's our domain entity); a single cell is `Cell`/`Datum`/`ScalarValue`
  (not `Value`/`AttributeValue` — also domain types). Vocabulary: `Relation`, `Schema`,
  `Column`, `Row` (note gv-sql already has `*Row` table mirrors — adjacent meaning, different
  crate), `Cell`, `DataType`. The `Attribute → Column` rename usefully *marks the
  domain→relational boundary* (the source pivot).

### Leaning (provisional)

- **In-memory interpret is the primary path; SQL lowering is secondary.** Likely true given
  data sizes, but revisit if server-side analytics or large data appear.
- **Wide / query-directed pivot at the source** (pivot in *only the attributes the query
  references* as columns) rather than long form (one row per attribute-value). Wide keeps
  cross-attribute conditions (e.g. max `grade` where `outcome ∈ …`) **unary** — same-row
  column refs instead of a self-join. A fixed wide schema is impossible (attributes are
  user-defined), so the pivot is *parameterized by the plan*.
- **Bag (multiset) semantics**, SQL-style (duplicates allowed, order via sort), rather than
  pure-relational set semantics.
- **The viz/encoding spec travels with the saved plan** but is a separate concern from the
  algebra. Execution outputs a `Relation`; viz binds result columns → visual channels
  (grammar-of-graphics style: x/y/color for a chart, columns for a table, slots in a text
  template). Adding a viz type should never touch the plan or executor.

### Open questions

- **The `Source` catalog (physical mapping).** Source's *design* is settled (§3:
  `Entries | PlanRef`; template / completed / owner defaults; activity-as-filter; demand-driven
  pivot). What remains open is the **catalog** — where each column physically lives (which JSON
  path in the value blob, which table) — needed only by the SQL backend (in-memory walks
  structs), so it's deferred with SQL lowering. (The containment-session predicate Source would
  also carry is gated on the unsettled containment dimension, §3.)
- **The semantic-model layer is deferred** (named, reusable measure/dimension/filter
  *fragments* — see §3 "Not the deferred semantic model"). What we build now is the surface
  `AnalysisSpec` (§3). Revisit when fragment reuse across queries becomes painful.
- **Validation / binding details.** Because attributes are user-defined and *mutable*, a
  saved plan can go **stale** (references a renamed, deleted, or retyped attribute). Governing
  rule: **the schema is a derived value, not stored state.**
  - **Persisted (serde) form = the program only**: structure + IDs + literals. **No schema.**
    (`schema = f(structure, live catalog)`, and the catalog can drift after a plan is saved, so
    a persisted schema would just be a stale snapshot.)
  - **Loading = binding = deriving each node's output schema** bottom-up against the *live*
    catalog. That single pass *is* the staleness gate: success → a usable plan; failure → a
    type/staleness error pinpointing the offending node (attribute deleted / retyped / moved).
    "Validate" and "derive the schema" are the same operation, not two.
  - **v1: recompute on demand** — `schema(plan, catalog) -> Schema` as a pure function, run
    whenever needed. **Caching** the derived schema on an in-memory plan (DataFusion-style) is a
    **deferred optimization**, not needed first — and recomputing sidesteps cache invalidation
    entirely.
  - *(Optional, later)* persisting a *snapshot of assumed attribute types* purely for better
    staleness diagnostics ("was Numeric, now Text") is diagnostic metadata, not source-of-truth
    — probably over-engineering for v1.
- **SQL lowering specifics** (when we get there): the leaning is **naive nested-subquery
  codegen** (`SELECT <op> FROM (<lowered input>) t`), delegating optimization to the DB; a
  small **per-dialect function table** (`strftime` vs `date_trunc`, `json_each` vs
  `jsonb_array_elements`); and **parameterized** SQL (placeholders + bind), never string
  interpolation. A query builder (sea-query) would handle placeholders/dialects.

---

## 5. The schema/instance duality

A key insight worth its own section. **A relational operator is a typed function:**

- its **schema-transformer** (`Schema X → Schema Y`) is its **type** — static, *no tuples
  needed*;
- its **tuple-transformer** (`Relation_X → Relation_Y`) is its **implementation** — dynamic,
  needs the data.

This is the relational model's **heading vs body**, and PL's **type-level vs value-level /
type-checking vs evaluation**. It cleanly explains who needs what:

| Task | Needs | Nature |
|---|---|---|
| Build a **valid** plan | schema functions (type-check the composition) | static |
| **Lower to SQL** | structure + schema + embedded literals + catalog — **not the tuples** | static (a *compiler*) |
| **Run in-memory** | all of the above **plus the input tuples** | dynamic (an *interpreter*) |

So the **SQL backend is a compiler** (it consumes only the static half and emits a *residual
program* — the SQL — that the DB runs later against the data: this is **partial evaluation /
staging**), and the **in-memory backend is an interpreter** (runs both halves together). The
duality is *why* a compiler can exist here at all.

The enabling property is **schema-determinism**: an operator's output schema is a pure
function of its input schema(s) and parameters, **independent of the data**. That single
property licenses both static type-checking *and* compilation to SQL. The famous exception is
**PIVOT**, whose output columns depend on the *data values* — which is exactly why we
**enumerate pivot columns as plan parameters** (lifting the data-dependence into the
declaration restores schema-determinism).

**Design consequences:**

- Factor `Operator::output_schema(input) -> Schema` as a **first-class, shared** function used
  by *both* validation and SQL codegen — if they disagree, the interpreter and the SQL backend
  silently diverge. Three passes over the reified plan: `validate` (static), `to_sql` (static),
  `eval` (dynamic); the first two share the schema function.
- **The schema is derived, never stored state.** Treat `output_schema` as recomputable on
  demand (v1); caching it on a node is an optional optimization, and recomputing avoids cache
  invalidation. DataFusion confirms the shape: its `LogicalPlan` *caches* a schema per node but
  exposes `recompute_schema()` to re-derive it after a structural rewrite — i.e. the stored
  schema is a memoized cache, not a source of truth. (See the persisted-vs-derived rule under
  §4 Open questions.)
- **`schema()` delegates through schema-preserving operators.** Only schema-*changing* ops
  (source-pivot, project, group, window, unnest, join) compute a new schema; schema-*preserving*
  ops (filter, sort, limit) forward the child's. Keeps the schema function lean. (DataFusion
  does exactly this — filter/sort/limit return `input.schema()`.)

---

## 6. Key terms

- **Surface / `AnalysisSpec`** — the persisted, UI-built, domain-level **OLAP spec**
  (`{ source, filters, dimensions, measures }`); the thing the user saves and round-trips.
  Lowers to the relational IR (§4). Distinct from the deferred **semantic model** (reusable
  named fragments).
- **Carrier type** — the value that flows between operators. Here: a **Relation** (bag of
  typed rows over a computed schema). Picking this was the central early decision.
- **Source vs result (`E` / `R`)** — `E` = an *entry-shaped* relation (the domain attribute
  vocabulary applies); `R` = an *aggregated result* (it doesn't). Aggregation (a measure) flips
  `E → R` — the GROUP BY boundary. A *surface-level* type discipline (governs composability:
  measure-less `E`-specs are reusable sources); at the IR level it's just relations-with-schemas.
- **Grain** — row granularity (one row per entry vs per group). Aggregation re-grains; the
  domain attribute vocabulary lives only at entry grain.
- **Relation / tuple / schema** — a *relation* is a set/bag of *tuples* over a common
  *schema* (heading = the typed columns; body = the actual tuples). A tuple is one row; a
  relation is the whole table. (`tuple : relation :: row : table :: element : set`.)
- **`FieldRef`** — the shared leaf naming a domain quantity: `Entry(EntryFieldRef)` (activity,
  temporal, …) or `Value(ValueFieldRef)` (an attribute's value). **Identity only**; type +
  capability resolved from the catalog, never stored. Referenced by filter, dimension, *and*
  measure.
- **aspect / `range_fold`** — within-value resolution of a `Value` to a scalar: pick
  `Plan`/`Actual` (aspect), then collapse a *range* value to a point (`range_fold`). The
  per-row counterpart of a measure's across-row aggregate — **two-level Min/Max/Mean**.
- **Dimension / measure** — OLAP: a **dimension** is `row → key` (a `FieldRef` + a transform →
  a *discrete* key; the group/slice axis); a **measure** is `[Row] → Scalar` (an aggregate over
  a group — the `E → R` step). In our dynamic model, dimension-vs-measure is mostly a *role per
  query*, not an intrinsic field tag — though `count` is a measure with no field at all, evidence
  that the *aggregation*, not the field, is the essence. **We mean "measure" in the semantic-layer
  / BI sense** (Malloy / LookML / Tableau): a measure *is* an **aggregation expression**
  (`sum(distance)`), the *definition* side. This differs from the classic **OLAP-cube** sense,
  where "measure" names the numeric *cell value* — the *instance* side. Same concept, opposite
  ends (the schema/instance duality, §5): a cube cell value is just our aggregation already
  applied. We're a query *builder*, so we live on the definition side.
- **Containment dimension** *(unsettled)* — grouping entries by the (transitive) ancestor
  **session** containing them; the key is derived at the `Source` by an ancestor-walk to the
  nearest entry matching a session predicate. Gated on defining "session" (structural vs windowed).
- **Reification** — representing the query as inspectable *data* rather than executable
  *behavior*. Enables save/compose/round-trip/validate/multi-backend.
- **Logical plan / relational IR** — the reified relational-algebra tree; the **lowering
  target** for the surface. (The *persisted* artifact is the `AnalysisSpec`, §3 — the IR may be
  ephemeral.)
- **Lowering** — translating the surface/IR down to a backend target (a SQL string, or
  in-memory operations). For SQL it's *generation from a small closed IR* — a code generator,
  **not** a SQL parser; a different and much easier complexity class. Also: surface → IR
  desugaring (one-way, lossy — which is why we persist the surface).
- **Interpreter vs compiler** — in-memory eval (interpreter) vs SQL codegen (compiler /
  partial evaluation).
- **Schema function / instance function** — an operator's static type vs its dynamic
  implementation (the [duality](#5-the-schemainstance-duality)).
- **Schema-determinism** — output schema is a pure function of the plan, not the data. What
  makes static type-checking and SQL compilation possible. PIVOT is the exception.
- **Closure property** — operators take and return the same carrier type, so they compose
  arbitrarily.
- **Pipeline vs tree / chain** — *chain* = every operator unary (one input); *tree* = some
  operator is binary (join/union). We're chain-now, tree-ready.
- **Pivot / unnest** — boundary operators bridging nested storage to flat relations. *Pivot*:
  EAV attribute → column (a join under the hood). *Unnest*: a list-valued column (our
  multiselect JSON arrays) → one row per element. Inverse of group+aggregate.
- **EAV (entity-attribute-value)** — our attribute/value storage shape; the reason the source
  needs a pivot.
- **1NF / normal form** — atomicity (one atomic value per column). Why a JSON list in a column
  is "tension" (a repeating group) resolved by `unnest`, and why GROUP BY must fold to a flat
  result rather than a tuple-of-tuples.
- **Source / `PlanRef`** — the source operator: the owner's non-template, completed-entry
  universe (or a `PlanRef` to another source-shaped spec), with referenced attributes pivoted in
  on demand. `PlanRef` is the composition mechanism (a source that reads another saved spec).
- **Semantic layer / headless BI** — the existing-tool category our idea most resembles.
- **Expression problem / initial vs final (tagless) encoding** — why we reify (data + passes)
  rather than put behavior on operators: a small closed operator set with several backends
  favors initial encoding.

---

## 7. Reference resources

### Codebases / tools

- **Apache DataFusion** (Rust) — *the* closest reference. A reified relational algebra in
  Rust: `LogicalPlan`, `LogicalPlanBuilder`, `Expr`, Arrow's `Schema`/`Field`,
  `ScalarValue`, `DataType`. Also has **Substrait** support (a serializable cross-engine
  logical-plan format — relevant to our save/serialize requirement). See the dedicated
  section below.
- **Apache Calcite** (Java) — canonical logical-plan (`RelNode`) + expression (`RexNode`) +
  optimizer framework. Reference for the *model*; heavy on the optimization we delegate.
- **Spark Catalyst** — `TreeNode` + rule-based analysis/optimization.
- **Polars** `LazyFrame` (Rust) — a reified lazy plan with an optimizer.
- **DuckDB** — embeddable OLAP engine with a relational API; a "what good embedded analytics
  feels like" reference.
- **Malloy** — **the primary inspiration to draw from.** Semantic measures/dimensions → SQL,
  plus the **source-vs-query** distinction we rediscovered (§3), and the semantic-layer "measure
  = aggregation expression" sense we adopt (§6). Closest to our *intent*; open source, strong
  docs, and well-regarded in the DB-research community (a Malloy lead was interviewed by Andy
  Pavlo's CMU DB group). Study the model, not the form factor (server-side, warehouse-oriented).
  Our **lineage** overall is the **BI / semantic-layer** tradition: **Malloy, LookML, Tableau**.
- **PRQL** — a pipeline AST that compiles to SQL. Closest to "pipeline that lowers to SQL."
- **Cube.dev / dbt MetricFlow / LookML** — the semantic-layer / headless-BI category (the
  deferred reusable-fragment model; LookML `dimension_group` is the time-dimension prior art).
- **sea-query** (Rust) — a standalone query builder; a candidate for the *lowering* step
  (placeholders, dialects) if/when we generate SQL.

### Ideas / papers

- Codd 1970, *A Relational Model of Data* — relational algebra, the closure property.
- Kimball, *dimensional modeling* (star schema) — origin of measures/dimensions; OLAP/MDX.
- Buneman et al. 1994, *Comprehension syntax*; Fegaras & Maier, *monoid comprehension
  calculus* — the theory behind pipeline/LINQ queries and nested data (the flat-vs-nested
  fork).
- Carette/Kiselyov/Shan, *Finally Tagless*; Wadler, *the expression problem* — encoding
  choices (why we reify).
- Grammar of graphics (Wilkinson); **Vega-Lite**; Tableau **VizQL/Polaris** — for the viz
  layer, where query spec and chart spec unify.

---

## For a DataFusion research agent

**Goal:** draw inspiration for our *logical plan + schema propagation + expression typing*.
DataFusion is far bigger and more general than we need (columnar Arrow execution,
cost-based optimization, distributed execution, large data). **We want the modeling ideas,
not the engine.** Read with our constraints in mind: small data, in-memory-first, no
optimizer, unary-chain-now, lower-to-SQL-later, plan-as-saved-artifact. Note that DataFusion's
`LogicalPlan` corresponds to our **relational IR (§4)**, not our **surface `AnalysisSpec`
(§3)** — DataFusion has no surface layer (its frontend is SQL/DataFrame text), so the
surface vocabulary is ours to design.

### Early findings (first read of `datafusion/expr/src/logical_plan/plan.rs`)

- **Crate scoping:** `datafusion-expr` is the top-level **logical layer** — it holds `Expr`
  *and* `LogicalPlan` *and* the logical operators, not a scoped-down piece. The crate split is
  logical-vs-physical (the physical plan lives in `datafusion-physical-plan`) plus modularity
  (`datafusion-common`, `-optimizer`, `-sql`, umbrella `datafusion`).
- **Schema is a memoized derivation, not source-of-truth.** `LogicalPlan::schema()` returns a
  stored `DFSchemaRef`, but only at schema-*changing* variants (`Projection`, `Aggregate`,
  `Window`, `Join`, `Union`, `DistinctOn`); schema-*preserving* ones (`Filter`, `Sort`, `Limit`,
  `Repartition`) delegate to `input.schema()`. `try_new` *derives* the schema at construction;
  `recompute_schema()` *re-derives* it after a structural rewrite — proof it's a cache. Our
  recompute-on-demand default and the delegate-through-preserving-ops pattern both follow this.
- **Lifecycle difference that drives our rule:** DataFusion's `LogicalPlan` is *ephemeral* (built
  per-query against a fixed catalog, then discarded), so caching a schema in it is safe
  memoization. Ours is a *persisted artifact* that outlives its catalog → we don't persist the
  schema; we re-derive on load (§4).
- **Serialization is via Substrait**, a separate representation — *not* the in-memory
  `LogicalPlan`. Relevant to our saved-plan-format question (Q8).

### Questions to investigate

(Several are partly answered in *Early findings* above — treat those as "go deeper.")

1. **`LogicalPlan` structure** — how is the enum shaped? How are child plans referenced
   (`Arc<LogicalPlan>`)? How are unary vs binary (Join) nodes represented? How uniform is it?
2. **Schema propagation** — how does each operator compute its **output schema** from its
   inputs (`DFSchema`, `LogicalPlan::schema()`)? This maps directly to our "schema function"
   spine — the most important thing to study.
3. **Expressions (`Expr`)** — representation, and how an `Expr` gets a `DataType` against a
   schema (type-checking). How are **column references** vs **literals** vs **aggregates**
   distinguished? Note the `Field` (schema entry) vs `Column` (reference-in-expression)
   split — relevant to our naming, and to our `FieldRef` (§3).
4. **Builder ergonomics** — how does `LogicalPlanBuilder` construct plans, and is anything
   validated *during* construction vs in a later pass?
5. **Logical → physical separation** — how is `LogicalPlan` turned into `ExecutionPlan`?
   We delegate physical to the DB, but the *seam* (and what lives on each side) is
   instructive for our schema-vs-instance duality.
6. **Aggregates, group-by, and window functions** — how are these represented in the logical
   plan? (Windows are our next operator; aggregates + group-by are our measures + dimensions.)
7. **Catalog / table source** (`TableProvider`) — how is "where data comes from and its
   schema" abstracted? Relevant to our `Source` operator and the EAV→column catalog.
8. **Serialization** — how does **Substrait** serialize a logical plan? Is it a viable model
   for *our* saved-plan format, or too general?

### What to deliberately ignore

The optimizer / rule framework, physical planning, Arrow columnar execution and vectorization,
partitioning/parallelism/distribution, and anything streaming or out-of-core. None of it
applies at our data scale, and copying it is exactly the "reimplementing SQL" trap we're
avoiding.
