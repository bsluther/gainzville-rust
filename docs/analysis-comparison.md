# Analysis subsystem — design comparison (neutral evidence base)

> **Purpose.** This document compares two independently-produced designs for the new **analysis**
> subsystem (visualizations + goals over a user's training data) so maintainers can synthesize a
> final design *decision by decision*. It is **evidence only**: no winner, no ranking, no merged
> design. Strengths and failure modes are reported per design as input to a follow-on collaborative
> synthesis. It is written to be self-contained.
>
> **The two designs.**
> - **A1** = `docs/analysis-design-1.md` — "Analysis / Query System — Design Notes." (The prompt
>   refers to this as `analysis-design.md`; it was renamed.)
> - **A2** = `docs/analysis-design-2.md` — "Analysis & Visualization — Design (alt)." (Renamed from
>   `analysis-design-alt.md`.)
>
> **The neutral bar** is `docs/analysis-features.md` (what users want, in domain terms). The
> assessment basis is `docs/analysis-rubric.md`.
>
> **Shared ancestry (context, not a judgment).** Both designs descend in part from a removed draft
> sketch (`core/src/analysis.rs` at commit `33236df`, deleted in `152794b`). A1 is the *direct*
> descendant — its surface vocabulary (`Filter`/`ActivityFilter`/`EntryFieldRef`/`FieldRef`/
> `RangeFold`/`Facet`/`Relation`/`Column`/`Catalog`/`resolve_type`) matches the draft almost
> verbatim. A2 reused only the *leaf* vocabulary (`RangeFold`→`RangePolicy`, `Facet{Plan,Actual}`→
> `Source{Actual,Plan}`) and reframed everything else around kinds + a pipeline. This explains some
> genuine terminology overlap and is worth keeping in mind when reconciling terms.

---

## 1. Codebase ground truth (feasibility anchor)

Both designs make claims about the repo. These were verified first-hand (file:line below); the
synthesis should rely on *these facts*, not on either design's characterization of them.

**Domain model** (`core/src/models/`):
- `Entry` (`entry.rs:10`) has `activity_id: Option<Uuid>`, `owner_id`, `is_template`,
  `is_sequence`, `is_complete`, `display_as_sets` (all `bool`), `position: Option<Position>`
  (`{parent_id, frac_index}`), and `temporal: Temporal`. There is **no scalar `canonical_instant`
  field** — it is a *method* on `Temporal` (`infer_start().or(end())`, `entry.rs:248`). `Temporal`
  (`entry.rs:97`) is an enum over None/Start/End/Duration/StartAndEnd/… (durations are `u32` ms).
- The **forest is a flat `Vec<Entry>` newtype** (`forest.rs:7`); structure is derived by scanning.
  `Forest::ancestors(id)` (`forest.rs:102`) walks the *full* parent chain. There is **no
  `nearest_ancestor_where` / predicate-ancestor helper** (A2 proposes one as new foundation code).
  A `FindAncestors` *query* exists (`queries.rs:275`), returning `Vec<Uuid>`.
- **Attributes** (`attribute.rs`): exactly six kinds — `Numeric, Select, Multiselect, Mass, Length,
  Text`. **Range** values exist as an `Exact | Range` axis on `Numeric/Select/Mass/Length` only
  (Text and Multiselect have no range). `Select` carries a **config-time `ordered: bool`**; the
  ordinal is the option's index in `options` (`attribute.rs:375,406`). `Mass`/`Length` carry **unit
  enums** on the value (config holds a `default_unit`). There is **no Duration/Time *attribute*
  kind** — time lives on `Entry.temporal`.
- **Plan vs Actual** lives at the *value* level: `Value { entry_id, attribute_id, plan:
  Option<AttributeValue>, actual: Option<AttributeValue>, … }` (`attribute.rs:700`).
- **No activity category / is-a / taxonomy / DAG exists in code.** The only activity→activity field
  is `source_activity_id` (copy provenance, not categorization). Categories are a *future design* in
  `docs/model.md:248` only. Both designs correctly treat categorization as deferred-behind-a-seam.

**Persistence / write path** (`core/src/{actions,mutators,delta,delta_executor}.rs`, `gv-sql/`):
- `Action` (16 variants) → free `mutator` fns → `Mutation { id, timestamp, action, changes:
  Vec<AnyDelta> }` → `AnyDelta` (6 entity variants: User/Actor/Activity/Entry/Attribute/Value) →
  `DeltaExecutor` per backend. Adding a new entity touches **~9 places**, including a hand-written
  SQL `DeltaExecutor` impl and a migration for **both** SQLite and Postgres.
- **`AttributeConfig` is the precedent for "store a typed enum as one external-tagged JSON-as-TEXT
  column" + a sibling `data_type` discriminator column** (`gv-sql/rows.rs:216`). External tagging is
  mandatory workspace-wide (internally-tagged enums break under `serde_json/arbitrary_precision`,
  enabled by `ivm`). Any saved-query declaration would follow this exact pattern.

**Read path** (`core/src/{queries,query_executor}.rs`, `gv-sql/*/query_executor.rs`):
- `define_query!` macro + sealed `Query` trait; `AnyQuery`/`AnyQueryResponse` are ~20-variant enums
  with per-variant `From` impls. A new query variant needs a **hand-written SQL executor for both
  backends** plus `AnyQuery` plumbing.
- **There is no in-memory query interpreter.** *Every* `QueryExecutor` impl is `Sqlite*`/`Postgres*`
  over sqlx. `generation::Model` holds entities in maps and *applies deltas* (write side) but does
  **not** implement `QueryExecutor`. **Both designs' in-memory execution engine is net-new work** —
  neither is building on an existing evaluator.
- **`QueryStore`** (`client/query_store.rs`, client/SQLite only) is a real change-broadcast + cache:
  on each committed action a `broadcast::<()>` fires and **every subscribed query is re-run from
  scratch** against SQLite (full recompute, no incrementalism). This is the "on-demand recompute"
  plumbing both designs rely on; it exists, client-side only.

**Sync** (`docs/sync.md`): **design only, no code.** Planned mechanism is an integer global sequence
number + a local mutation-log table + rebasing — explicitly **not HLC** (`sync.md:83`). No entity is
synced yet (only commented stubs). So "syncable" means "flows through Action→Delta"; **neither
design is advantaged on sync** — both inherit the same unbuilt machinery.

**FFI** (`gv-ffi/`, uniffi 0.31 proc-macro, no UDL):
- Domain types cross via `#[uniffi::remote(Record/Enum)]`. **Finite, non-recursive nested enums
  cross cleanly today** — `AttributeValue → MassValue → MassMeasurement` is live proof.
  `custom_type!` bridges leaf scalars (`Uuid`, `DateTime`→i64). **No domain type is JSON-stringified
  across FFI today.**
- **Opaque `#[derive(uniffi::Object)]` interfaces are already in use** (`GainzvilleCore`,
  `FfiQuerySubscription`) — the "opaque builder Object" pattern A2 proposes is not exotic.
- **Recursive enums cannot cross as Swift enums in uniffi 0.31** — verified directly in the bindgen
  Swift template ("we don't yet support `indirect` for enums"). This is a Swift-codegen limit (not a
  wire limit) and is version-scoped. It is the concrete constraint that prices A2's recursive
  `Predicate`/`Measure` (see §4, Contrast F).
- Query path to Swift: `GainzvilleCore.run_action(Action)`, `.subscribe_query(AnyQuery) →
  FfiQuerySubscription`, `.read_query(AnyQuery) → Option<AnyQueryResponse>`, `CoreListener` callback.

---

## 2. Terminology reconciliation (A1 ↔ A2)

The same word often means different things across A1, A2, the core model, and prior art. This table
is load-bearing for the synthesis; the **⚠ collisions** are where confusion is most likely.

| Concept | A1 term | A2 term | Notes / collisions |
|---|---|---|---|
| The persisted, user-facing artifact | **`AnalysisSpec`** ("the surface") | **`SavedQuery` declaration** | Both: a domain-level declaration persisted as one external-tagged JSON column; the IR is derived, never persisted. Close match in *role*. |
| The user-facing *shape* | a **single fixed-shape OLAP spec** `{source, filters, dimensions, measures}` | a **curated set of `Kind`s** (Aggregate, Frequency, Distribution, Record, Sequential, Correlation, Goal) | A1's whole surface ≈ A2's **Aggregate kind** specifically. A2 has no single uniform shape; A1 has no per-objective templates. |
| The derived execution algebra | **relational IR / logical plan** (free `Relation→Relation`, reified tree, unary-now/tree-ready) | **`Pipeline = Vec<Step>`** (linear; one binary `Join`) | Both reified data, never persisted. A1 = closed algebra w/ closure; A2 = linear steps + bounded join. |
| Lowering | "**lowering**" (surface→IR, IR→SQL) | "**expands to**" (kind→pipeline), "compiles to" (pipeline→SQL) | Same two-step shape. |
| The entry universe | **`Source = Entries \| PlanRef`** | **`SourceSpec = Entries \| Spine`** | ⚠ **Collision.** Both name it Source/SourceSpec but the *second variant differs*: A1's is `PlanRef` (composition); A2's is `Spine` (calendar densification). A1 has no Spine; A2 puts composition elsewhere (`QueryRef`). |
| Plan/Actual selector | **`aspect: Plan \| Actual`** | **`Source { Actual, Plan }`** (in value coercion, §5.1) | ⚠ **Collision.** A2 reuses the word **"Source"** for *both* the entry universe (`SourceSpec`) *and* the plan/actual selector. A1 keeps these distinct (`Source` vs `aspect`). Pin this hard. |
| Reduce a range value to a scalar | **`range_fold: Min\|Max\|Mean`** | **`RangePolicy: Min\|Max\|Mean\|Omit`** | Same semantics, different name (both descend from the draft's `RangeFold`). |
| Group key | **dimension** (`field + transform → discrete key`) | **`KeySpec`** (`Activity\|Category\|AncestorWhere\|Time\|AttrValue\|Band`) | Same concept. A2 enumerates variants; A1 describes transforms (identity / time-truncation / binning). A1 "numeric binning" ≈ A2 `Band`. |
| Aggregation over a group | **measure** = `[Row]→Scalar` (BI/semantic-layer sense: *an aggregation expression*) | **`AggSpec`** = `Count\|Sum\|Min\|Max\|Mean\|Median\|Percentile\|StdDev\|Histogram` | ⚠ **The most dangerous collision.** A1 **measure** = the aggregation. A2 **`Measure`** = *something else* (next row). |
| Per-row computed value | **derive** (`E→E`), but A1 disclaims a scalar-expression sublanguage | **`Measure`** = a per-row numeric expression (`Attr \| Const \| Bin(arith)`) yielding a `Quantity` | ⚠ A2's **`Measure` is NOT an aggregation** — it is A1's *derive*. A1.measure ≈ A2.AggSpec; A2.Measure ≈ A1.derive. |
| The leaf naming a domain quantity | **`FieldRef`** = `Entry(...) \| Value(...)` — one universal leaf used by filter, dimension, *and* measure | scattered: **`Measure::Attr`**, **`KeySpec::AttrValue`**, **`Predicate`** attr refs, **`FieldRef`** (only inside `ExtremumSpec`, for projection) | ⚠ **Collision.** Both have a `FieldRef`, but A1's is the *universal* quantity reference; A2's is *only* the extremum-projection field list. A2 has no single universal leaf. |
| The uniform carrier between operators | **`Relation`** (bag of typed rows over a computed schema) — one closed carrier type | **rows** reshape per step (entry-in-context → grouped → joined); only the *output* is a named type | A1 has *one* closed carrier (closure property); A2's intermediate shape shifts and only the final `ResultTable` is named. |
| The viz-facing output | a **`Relation`** + a separate viz/encoding spec ("binds columns→channels") — concrete type unspecified | **`ResultTable { columns: Vec<ColumnSpec>, rows: Vec<Vec<Cell>> }`** with a typed `Cell` enum | A2 specifies the contract concretely (typed cells, column role/kind); A1 leaves it abstract. |
| Output cell / column | **`Cell`/`Datum`/`ScalarValue`**, **`Column`/`Schema`**, **`DataType`** (Float/Int/String/Timestamp) | **`Cell`** (Quantity/Ordinal/Category/TimeBucket/Instant/Text/Count/Null), **`ColumnSpec {role, kind}`** | A2's cell types carry *semantic* type (Ordinal vs Category vs Instant vs Quantity+Dim); A1's `DataType` is closer to *storage* type. |
| Reuse / composition | **`PlanRef`** = a `Source` reads another saved measure-less (`E`-shaped) spec (view/CTE-style *source substitution*) | **query-references-query**: `MatchesQuery(QueryRef)` predicate, `SideInput::Saved(QueryRef)` join input, "a filter is any entry-returning query" | Both = reuse-by-reference, *different splice point*: A1 substitutes the source; A2 references in a predicate/side-input. |
| Source-vs-result type discipline | **`E` / `R`** (entry-shaped vs aggregated; measure-presence determines which; only `E` is reusable) | *(no equivalent)* — grain change is implicit in pipeline position | **A1-only concept.** Governs A1's composability. |
| Sessions | **containment dimension** (group by transitive ancestor session; *unsettled*) | **`AncestorWhere`/`nearest_ancestor_where`** (settled, D1) | Same idea (ancestor-walk to a session predicate); A1 unsettled, A2 settled. |
| argmax-with-context | *(unaddressed; `max` is an aggregate measure)* | **`Extremum`** (row-selection; keeps + projects the winning row) | **A2-only operator.** |
| Absence / densify | *(no equivalent)* | **`Spine`** (generated dense axis + outer join + `Fill`) | **A2-only.** Prior art calls this a **time/date spine** (§8). |
| Windows | **`window`** (named as "next reach", unary `Relation→Relation`, undetailed) | **`WindowSpec`** = `FrameAggregate \| Scan` (Delta/Streak/TimeSinceLast/IsRecordMax) | Both agree windows are unary chain ops. A2 specifies the taxonomy; A1 names it as a future operator. |
| Value coercion types | **`DataType`** (Float/Int/String/Timestamp) | **`Quantity{value, dim}` / `Comparable{Quantity\|Ordinal}` / `Dim`** | A2 models units/ordinals explicitly; A1 does not. |
| Goals | *(barely mentioned)* | **`Goal` = (query + hoped-for result)**; progress derived | **A2-only specification.** |
| The "named, reusable measure/dimension library" | **semantic model** (explicitly **deferred**) | *(no named layer; D5 "one fewer concept")* | Both defer the Malloy/LookML named-fragment layer. Prior art (§8) says these are complementary, not either/or. |
| Static-type vs eval split | **schema/instance duality**, **schema function**, **schema-determinism** (§5) — a first-class architectural concept | *(implicit; the interpreter just evaluates)* | **A1-only concept**, and a genuinely reusable one (see §6, A1 strengths). |
| Kind ↔ template | *(no curated-template layer)* | **`Kind`** = three-way contract (input form ↔ pipeline template ↔ result schema) | **A2-only concept.** |

---

## 3. Similarities (shared, explicitly or in substance)

Despite different framing, the designs agree on a large architectural core:

1. **Two layers: a persisted domain-level *declaration* that lowers to an internal execution *IR*.**
   (A1: `AnalysisSpec` → relational IR. A2: `Kind` → `Pipeline`.) Both **persist the declaration,
   derive the IR, and never persist the IR.**
2. **Persist the high-level declaration, not the compiled plan** — for the same reason (lowering is
   one-way/lossy; round-trip by keeping the high-level form; a compiled plan would couple to a schema
   snapshot). A1 argues this most explicitly (§3 "decompilation is ambiguous"); A2 states it as D6.
   Prior art strongly endorses this (every semantic layer persists the declaration, compiles SQL at
   run time — §8).
3. **Reified IR (data, not behavior)** — inspectable, so it can be validated, lowered, and run on
   multiple backends. (A1 explicit; A2's `Vec<Step>` is data.)
4. **In-memory execution first; in-DB (SQL) execution deferred but kept open** because the IR is a
   clean compilation target. Both note in-DB is a future, not foreclosed (A1 §4; A2 D10).
5. **Results are never persisted — always recomputed on demand**, reusing the existing
   change-broadcast/`QueryStore`. Both make "run a saved query" a new `Query` variant.
6. **Declaration stored as one external-tagged JSON column**, flowing the Action→Mutation→Delta path
   and syncing like any entity. (A2 explicit D6; A1 implies it via serde + the persistence rule.)
7. **Domain-level but ID-referenced** (`activity_id`/`attribute_id`, never names — names change, IDs
   don't). A1: "store IDs, display names." A2: `Uuid` throughout.
8. **Types/schema are resolved from the *live* catalog, never stored; a saved query can go stale and
   is re-validated on load.** A1 makes this a centerpiece (§5, "schema is a derived value"); A2
   states "IR derived, never persisted" (less explicit about the validation/staleness gate).
9. **OLAP measure/dimension vocabulary at the core**, with dimension-vs-measure treated as a *role*,
   not an intrinsic field tag. (A1 explicit; A2 via the operator algebra.)
10. **Range reduction + plan/actual selection at the value leaf** — both reduce a range to a scalar
    via Min/Max/Mean and select Plan vs Actual. (A1: `range_fold`+`aspect`; A2: `RangePolicy`+
    `Source`.) Shared with the core model's `Value{plan, actual}` and `Exact|Range`.
11. **Sessions are not a primitive** — both express "group by containing session" as an ancestor-walk
    against a predicate, leaving "what a session is" to user intent. (A1 containment dimension; A2 D1.)
12. **Multi-valued grouping (categories) fans one row into several groups**, and per-category totals
    can exceed the grand total. (A1: multiselect dimension needs `unnest`; A2: `KeySpec::Category`
    multi-valued, D4.)
13. **Flat / 1NF carrier** — both flatten the forest/EAV to rows with pivot/unnest at the boundary,
    rather than carrying nested collections. (A1 explicit; A2's rows reshape but stay flat.)
14. **Tidy/long output to the viz layer, with column→channel binding kept as a separate concern**
    (grammar-of-graphics style). (A1: "viz binds result columns→channels"; A2: long/tidy
    `ResultTable` + kind↔viz contract.)
15. **Determinism / `Arbitrary`-generatability for property & simulation testing** is a stated goal.
    (A2 explicit; A1 cites "deterministic-simulation reference semantics.")
16. **Categorization deferred behind a resolver seam**, while the analysis layer is designed to roll
    up by category. (A1: taxonomy via categorizing activities; A2: `CategoryResolver`, D4.)
17. **Both defer the named-fragment "semantic model"** (reusable named measures/dimensions/filters).
    (A1 explicitly; A2 by choosing query-references-query instead.)

---

## 4. Contrasts (with the commitment behind each)

Each contrast below states the *surface* difference and the **commitment** it encodes — the thing a
synthesis actually has to choose between.

### Contrast A — The user surface: one fixed-shape OLAP spec (A1) vs. a curated set of kinds (A2)
- **A1:** a single shape `{source, filters, dimensions, measures}`. The UI builds measures/
  dimensions/filters; anything beyond the shape is reached by the general IR underneath (the "escape
  hatch"). Round-trips trivially.
- **A2:** N per-objective templates (Aggregate, Distribution, Correlation, Sequential, Record,
  Frequency, Goal), each a three-way contract (input form ↔ pipeline template ↔ result schema), each
  mapping 1:1 to a visualization.
- **Commitment.** A1 bets *one OLAP shape* covers the surface and that composability is best offered
  *within* that shape (free recombination of measures/dimensions/filters) — novel cases need no new
  surface code. A2 bets the surface should be *curated per objective* (even a uniform OLAP builder is
  "turning users into programmers"), accepting that each new objective is a code change because kinds
  map 1:1 to viz components built anyway. **A1 = one general user-composable shape; A2 = N specific
  guided shapes.** Where the composability lives flips: A1 exposes it at the surface (within OLAP);
  A2 keeps it interior (the algebra) and exposes only curated slices.
- **Note.** A1's single shape "can't express arbitrary multi-stage transforms" (A1 admits this).
  Crucially its surface vocabulary (filters/dimensions/measures) has **no slot** for windows,
  spines, joins, derived arithmetic, or statistics beyond count/sum/max/min/avg — so the non-OLAP
  feature families (sequential, correlation, distribution, absence) have **no A1 surface** as
  written, even though its *IR* could host some of them. A2's kinds cover them explicitly.

### Contrast B — IR shape: free `Relation→Relation` algebra, tree-ready (A1) vs. linear `Vec<Step>` + one bounded binary join (A2)
- **A1:** operators are `Relation→Relation` with the **closure property**; a recursive plan tree
  (`Box`/`Arc`) populated with unary variants now but **typed for binary later** (join/union as
  non-breaking additions). The carrier is one closed `Relation` type with a computed schema.
- **A2:** a **linear** `Vec<Step>` where re-graining is just a longer sequence
  (`Group→Aggregate→Having→Group→Aggregate`); the **only branch is a single binary `Join` step**
  whose side-input is *linear* (no join-of-joins; N-way/lagged deferred, D12).
- **Commitment.** A1 commits to a **general carrier-closed algebra** (DataFusion-shaped), paying
  upfront for a recursive representation to keep arbitrary binary operators a non-breaking future.
  A2 commits to a **flat operator sequence bounded to linear-plus-one-join**, betting that the linear
  shape covers the use cases with less ceremony and that re-graining (the usual reason to nest) is
  just a longer line. Prior art supports *both*: a linear surface can host joins via a step whose
  argument is itself a (sub-)plan (PRQL/Calcite `RelBuilder`), but "ship a flat `enum Step` with only
  column-ref arguments" cannot host a join later without a format break (§8, dataframe brief). A1's
  tree-ready typing is exactly the "cheap insurance" that brief recommends; A2's bounded `SideInput`
  is the "step argument can be a sub-pipeline" form of the same insurance.

### Contrast C — Scalar-expression sublanguage: none (A1) vs. bounded arithmetic-over-measures (A2)
- **A1:** "**No general scalar-expression sublanguage** (no arbitrary arithmetic/CASE/string
  functions)." Its surface `{source, filters, dimensions, measures}` has **no derive slot at all**
  (the IR lists a `derive` operator, but the *surface* can't carry one).
- **A2:** `Measure = Attr | Const | Bin(Box<Measure>, ArithOp, Box<Measure>)` — a recursive
  arithmetic expression — plus named derived measures (§6.6).
- **Commitment.** A2 accepts a (bounded) scalar-expression sublanguage as *necessary* for the
  "arithmetic measures (derived metrics)" feature family — tonnage (`reps×weight`), Epley 1RM, pace,
  load, density. A1 tries to *avoid* one. The consequence is concrete: **A1 as written cannot express
  derived metrics at its surface**; reaching them needs either a derive slot + the disclaimed
  scalar-expression mini-language, or modeling derived measures as their own type. Prior art is
  unambiguous here: "**invest in a scalar-expression sublanguage before adding operators** — this is
  the dividing line between PRQL-clean and ad-hoc," and skipping it causes operator sprawl (§8,
  dataframe brief). This contrast also *prices Contrast F* (recursion ⇒ FFI cost).

### Contrast D — Coverage of the hard feature families
A1 *explicitly scopes itself* to a vertical slice (its 6 driving use cases, "all the same OLAP
shape") and grows "one operator at a time." A2 *front-loads* the full feature surface. So this is
partly a framing difference the rubric warns against rewarding — but it also reflects real
structural differences in what each model can host as written:

| Feature family (from `analysis-features.md`) | A1 as written | A2 as written |
|---|---|---|
| Counting / frequency / simple aggregation | ✅ core (the 6 use cases) | ✅ Aggregate/Frequency kinds |
| Arithmetic / derived measures (tonnage, 1RM, pace…) | ❌ structurally excluded at surface (Contrast C) | ✅ `Measure::Bin` + derived measures |
| Extremes *with context* (PR + date/route) | ❌ measure-is-aggregate can't carry the winning row | ✅ `Extremum` (row-selection, D-stress-test §11.1) |
| Sequential / windows (moving avg, ACWR, streaks, deltas, time-since, PR-detection) | ⚠ named as "next reach", undetailed | ✅ `WindowSpec` (FrameAggregate + Scan), stress-tested |
| Statistics (histogram, percentile, median, stddev, monotony) | ❌ not in its aggregate set (count/sum/max/min/avg) | ✅ full `AggSpec` family + Distribution kind |
| Cross-source correlation (scatter) | ⚠ deferred with trees (binary join) | ✅ bounded `Join` + Correlation kind, stress-tested |
| Absence analysis (rest days, gaps, layoff) | ❌ no spine; needs deferred join + generated source | ✅ `Spine` + outer-join + `Fill` |
| Filtering/re-grouping on computed values (HAVING + re-grain) | ⚠ HAVING deferred; needs `PlanRef` chaining + relax `E/R` | ✅ `Having` + re-`Group` (linear) |
| Goals & forecasting | ⚠ barely mentioned | ✅ `Goal = (query + hoped-for result)`; forecasting math deferred |
| Taxonomy / multi-valued categorization | ⚠ multiselect dimension + `unnest` (sketched) | ✅ `KeySpec::Category` multi-valued (D4) |

- **Commitment.** A1's commitment is **architectural depth on a narrow slice + a disciplined
  extensibility story** (closure + schema duality + tree-ready). A2's is **breadth with concrete
  operators for nearly every family**, with extensibility resting on *adding kinds* (code) and an
  algebra "grown kind-driven." The gate question (rubric: coverage "now or via a *clearly-foreseen*
  extension") bites two A1 cells specifically: **derived measures** (cuts against its no-scalar
  commitment) and **extremes-with-context** (needs a row-selection operator A1 never names) are *not
  clearly foreseen* in A1's stated commitments; windows and the spine are foreseeable but undesigned.

### Contrast E — The visualization data contract (the literal in-scope deliverable)
- **A1:** output is "a `Relation`" with `DataType` columns (Float/Int/String/Timestamp); "viz binds
  result columns→channels," concrete output type unspecified; the encoding spec "travels with the
  saved plan but is a separate concern."
- **A2:** output is a concrete `ResultTable` with a typed `Cell` enum (Quantity-with-`Dim` / Ordinal
  / Category / TimeBucket / Instant / Text / Count / Null) and `ColumnSpec {role: Dimension|Measure,
  kind: CellKind}`; long/tidy; bivariate axis assignment lives in the kind↔viz contract.
- **Commitment.** A2 makes the viz contract a **first-class, concrete, semantically-typed
  deliverable**; A1 treats it as "a Relation + a separate encoding spec, shape TBD." Prior art (§8,
  grammar-of-graphics) says the boundary *must* carry per-column **measurement type** (the
  nominal/ordinal/quantitative/temporal distinction), **role**, **sort order**, and the **full
  category domain** (to show empty buckets). A2's typed cells cover the N/O/Q/T-equivalent (Ordinal
  vs Category vs Instant vs Quantity); A1's storage-typed `DataType` is, per that brief, *insufficient
  on its own* (a string may be nominal or ordinal; a number may be quantitative or an ordinal code).
  A1's `FieldRef` resolution *knows* orderedness, so it *could* emit richer types — it just doesn't
  specify the output type. **Both** under-specify sort-order and full-category-domain (the
  empty-bucket problem), which the brief flags as acute for sparse fitness data.

### Contrast F — FFI strategy: A2 confronts the recursive-AST-across-uniffi problem; A1 is silent (and may not need to be)
- **A2:** recursive `Predicate`/`Measure` "cannot cross uniffi as enums" → an **opaque
  `PipelineBuilder` uniffi Object** holds the AST Rust-side and serializes to the JSON declaration on
  `build()`. Flat kinds/`ResultTable` cross directly.
- **A1:** silent on FFI. But its *surface* (`{source, filters, dimensions, measures}` over finite,
  largely non-recursive enums — `FieldRef` is finite; filters are a flat list; `Source = Entries |
  PlanRef(uuid)`) is plausibly **non-recursive**, which the grounding shows **crosses uniffi directly
  as nested enums today** (like `AttributeValue`), needing *no* builder Object.
- **Commitment / the real fork.** This is a *consequence* of Contrasts C and A, made concrete by the
  verified uniffi-0.31 "no `indirect` enums" limit. A2's richer expressiveness (recursive predicates
  with And/Or/Not; recursive arithmetic measures) is exactly **what forces** the builder-Object
  ceremony. A1's flatter surface crosses FFI cleanly **because** it forgoes that expressiveness — but
  the moment A1 needs boolean OR / nested predicates / arithmetic, it inherits A2's FFI problem.
  So the fork is **expressiveness (recursive predicates/measures) vs. FFI-simplicity (flat surface)**,
  and uniffi 0.31 is what makes the trade bite. (Both the opaque-Object workaround *and* the
  non-recursive-enum path are verified feasible in this repo.)

### Contrast G — How reuse/composition is governed: typed source-reuse (A1) vs. uniform query-references-query (A2)
- **A1:** the **source-vs-result (`E`/`R`) type discipline** governs reuse: only measure-less
  (`E`-shaped) specs are reusable as another spec's `Source` via `PlanRef`. Reuse is *typed* and
  *structural* (a saved "climbing" source = `Entries + activity-filter`, reused by reference).
- **A2:** reuse is **query-references-query** at the predicate/side-input level — "a filter is any
  entry-returning query reused in a selecting role" (`MatchesQuery`), `SideInput::Saved(QueryRef)`
  for joins. No type ceremony; any entry-returning query is reusable.
- **Commitment.** A1 makes reusability a *typed property* (measure-presence decides it), which is
  elegant but introduces a real limitation: an **aggregate-of-an-aggregate** (nested aggregation,
  e.g. "average of per-session counts") has the inner spec as an `R` (terminal), which A1's rule
  forbids as a source — so A1 must relax `E/R` or add multi-level surface support it hasn't
  specified. A2's uniform references express nested aggregation directly (linear re-grain) but
  without A1's typed-reuse guarantee. Prior art adds a third axis neither fully has: the
  **define-once-reference-by-name** named-fragment layer (Malloy `source`/LookML `view`), which both
  defer — and which the semantic-layer brief notes is *complementary* to query-references-query, not
  a substitute (§8).

### Contrast H — Units / dimensional analysis
- **A1:** unaddressed. `DataType` is unit-blind.
- **A2:** `Quantity` carries a `Dim` (Mass/Length/Duration/Count/Dimensionless), normalized to a base
  unit at the *leaf*; arithmetic does **not** propagate composite dimensions (derived results are
  labeled scalars, D11).
- **Commitment.** The core model has `Mass`/`Length` values carrying units; "max load across all
  bench presses" logged in both kg and lb **requires** normalization for a correct answer. A2 takes a
  position (normalize at leaves, don't propagate); A1 is silent. Minor in scope, but a real
  correctness concern A2 closes and A1 leaves open.

### Contrast I — Architectural rigor on validation / staleness / SQL-readiness
- **A1:** the **schema/instance duality** (§5) is a first-class, well-developed concept: each
  operator has a *schema function* (static, `Schema→Schema`) shared by **both** validation and SQL
  codegen, plus an *eval function* (dynamic). "Validate" and "derive the schema bottom-up against the
  live catalog" are the *same* operation; the result is a precise staleness gate. Schema-determinism
  (output schema is a pure function of the plan, not the data — PIVOT excepted) is what licenses
  SQL-lowering-later.
- **A2:** commits to the same outcomes (IR derived, in-memory now, SQL deferred) but does **not**
  articulate the schema-function-as-shared-spine insight or a precise validation/staleness gate.
- **Commitment.** This is less a fork than **an A1 insight worth adopting regardless of which surface
  wins.** Prior art independently confirms its value: DataFusion's per-operator
  `output_schema(input)` + `recompute_schema()` is exactly A1's model, and "schema is a pure function
  of the plan, quarantine PIVOT" is the standard way to keep a *saved* plan offline-validatable (§8,
  dataframe brief).

---

## 5. Common use-case runs

Per the rubric and prompt, both designs are run against a neutral set: representative cases from
`docs/analysis-features.md` plus **three devised cases** chosen to stress dimensions *neither* design
tuned for (so we avoid rewarding either design's own worked examples). "✅ direct / ⚠ awkward or via
deferred machinery / ❌ not expressible as written."

### From the features doc
| Use case | A1 | A2 |
|---|---|---|
| "Climbing sets per day" (count by time bucket) | ✅ `{source: climbing, dim: day, measure: count}` | ✅ Frequency/Aggregate kind |
| "Max YDS grade *sent* per week" | ✅ filter outcome∈{sent…}, dim week, measure max(grade) | ✅ Aggregate kind (ordinal max) |
| "Count of sends by grade" (outcome by grade, nested) | ⚠ two dimensions → tidy table ok; but **filtered counts** (sends *and* attempts side by side) need filtered aggregates — **deferred** | ⚠ two `KeySpec` ok; **filtered aggregates** (Count where outcome=send) **also not first-class** — see devised UC-C note |
| "Sessions with ≥10 climbs, counted per week" (HAVING + re-grain) | ⚠ needs deferred `HAVING` + `PlanRef` of an inner spec; inner is an `R`, which `E/R` forbids as a source ⇒ relax the discipline | ✅ linear `Group(session)→Agg(count)→Having(≥10)→Group(week)→Agg(count)` |
| "Longest run (max distance)" | ✅ measure max(distance) | ✅ Aggregate kind |
| "Volume per muscle group" (multi-valued category) | ⚠ multiselect/by-category dimension + `unnest` (sketched), and needs a derived measure it can't express (see UC-C) | ✅ `KeySpec::Category` (multi-valued) — but tonnage needs `Measure::Bin` (has it) |

### Devised case UC-A — "Estimated 1RM (Epley) by month for the back-squat, as a line"
*Stresses: derived measure (`weight×(1+reps/30)`) × time-bucketing × max-per-period.* (From the
"arithmetic measures" + "max load by period" families; not a worked example in either doc.)
- **A2:** ✅ `Filter(activity=back-squat) → Derive(epley = weight·(1+reps/30)) → Group(Time month) →
  Aggregate(Max(epley)) →` line. Direct.
- **A1:** ❌ **as written.** The derived measure `weight×(1+reps/30)` requires a scalar-expression the
  surface has no slot for and the design disclaims (Contrast C). Resolving it needs a surface derive
  slot + arithmetic, or modeling 1RM as its own saved measure type.

### Devised case UC-B — "Average climbs per session, per month"
*Stresses: two-level aggregation across a grain change (count per session, then average those counts
per month).* (From "nested aggregation" + "per-session analysis"; not a worked example in either.)
- **A2:** ✅ `Group(AncestorWhere=session)→Aggregate(count)→Group(Time month)→Aggregate(mean)` — a
  linear pipeline; the grain change is just the next `Group`.
- **A1:** ⚠ The inner "climbs per session" is an aggregation ⇒ an `R` (terminal). To average those
  per month you'd feed the inner result as a source to an outer aggregation — but A1's `E/R`
  discipline says **only measure-less `E`-specs are reusable as a source**. So A1 must either relax
  the discipline (an `R` becomes a re-grained source) or add multi-level surface support it hasn't
  specified. Expressible in principle (A1 notes "at the IR level `E`/`R` dissolve"), but **not via
  the stated surface composition model**.

### Devised case UC-C — "This month's tonnage (`reps×weight`) per muscle group, where one exercise belongs to several muscle groups"
*Stresses: multi-valued grouping (category fan-out) × derived measure × the category seam.* (From
"taxonomy" + "arithmetic measures" + multi-valued categorization; not a worked example in either.)
- **A2:** ✅ `Derive(tonnage = reps·weight) → Group(Category /*multi-valued*/) → Aggregate(Sum)`. The
  category fan-out (totals exceed grand total) is explicit (D4); tonnage uses `Measure::Bin`.
- **A1:** ❌/⚠ Two blockers: the derived measure (tonnage) is again unexpressible at the surface
  (Contrast C), and the multi-valued grouping needs the `unnest` boundary operator A1 sketches but
  doesn't fully specify. The category seam is deferred in both.
- **Shared note (prior art, §8):** *both* designs under-name the **allocation/weighting policy** for
  multi-valued categories. Kimball's bridge-table prior art: per-category subtotals exceeding the
  grand total is correct *only* under an explicit "impact (unweighted)" choice; reconciling to the
  total needs a weighting factor. Neither design encodes this choice.

### Devised case UC-D (brief) — "Histogram of run distances this year" + "p90 session length"
*Stresses: the statistics aggregate family.*
- **A2:** ✅ `Aggregate(Histogram{…})`, `Aggregate(Percentile(.,0.9))` — first-class.
- **A1:** ❌ as written — its aggregate set is count/sum/max/min/avg; histogram/percentile/median/
  stddev are not present and would be a (foreseeable) extension.

### Cross-cutting gap surfaced by the runs (affects BOTH)
**Filtered / conditional aggregates** — e.g. "send rate = sends / attempts" (features doc) needs two
counts over the *same* group differing only by predicate. A1 **explicitly defers** per-measure
filters. A2 has `Filter` and `Having` but **no `Count { where }`** and no conditional in `Measure`
(only `Attr|Const|Bin`), so it can only reach filtered aggregates via a heavyweight `Join` of two
pipelines. Prior art (§8, semantic-layer brief) calls filtered aggregates **table-stakes** in every
semantic layer (Malloy `count(){where}`, LookML `filters:`, Cube `CASE WHEN`, MetricFlow filters) and
warns that deferring the *named* feature doesn't remove the need — it pushes users to inline
conditionals or uncomposable separate queries. **Neither design has a first-class story here.**

---

## 6. Per-design evaluation (rubric criteria)

Per-criterion evidence; **no aggregate score, no winner.** The rubric's "Fit with the project"
criteria are flagged as matching A2's emphasis — A1 is assessed charitably there.

### Gates
- **Feature coverage.** *A2* covers all of `analysis-features.md` as written (some families deferred
  but clearly foreseen: forecasting math, time-gap sessions). *A1* covers the OLAP-shaped
  counting/aggregation majority gracefully but, **as written**, cannot express **derived/arithmetic
  measures** (Contrast C) or **extremes-with-context** (no row-selection operator), and omits
  statistics, the spine, and goals. These are not "structurally impossible" for a *synthesis* — but
  two of them cut against A1's *stated commitments*, so under the gate's "now or via a
  clearly-foreseen extension" test they are the place A1 is weakest. **Not a disqualification; a
  precise list of what A1 must add and which additions fight its own stance.**
- **Executability & boundary feasibility.** *Both pass.* Both execute in-memory over one user's data
  (neither has an existing interpreter — both are net-new). Both persist + sync via Action→Delta
  (verified path; sync unbuilt for all entities, so neither is advantaged). Both reach Swift — A1's
  flat surface trivially; A2 via the verified-feasible opaque builder Object for its recursive parts.
  The *grace* differs (FFI ceremony for A2's recursion; A1 FFI-trivial but less expressive — Contrast
  F).

### Structure
- **Simplicity / parsimony.** *A1* is more parsimonious **as written** (one carrier type, one surface
  shape, ~6–8 IR operators) — but partly because it *specifies less* (windows/statistics/spine/goals
  are named, not built). *A2* has more total moving parts (kinds + 12-step algebra + foundation:
  coercion/resolvers/calendar/spine/ResultTable) — richer, but more surface area; A2 itself flags
  "algebra build cost, worst for trivial scope (filter+count alone)." Honest read: A1's parsimony is
  partly *deferral*; A2's complexity is partly *coverage*.
- **Modularity.** *A1* draws a sharp seam (surface vs IR; the schema function as a shared spine; the
  `Attribute→Column` rename marking the domain→relational boundary). *A2* draws seams by crate
  (`gv-core` declarations+interpreter+foundation; `gv-sql` row+loader; `gv-ffi` builder) and by
  concern (kind vs pipeline vs foundation). Both are cleanly decomposed; A1's is more *layered*
  (two-tier lowering), A2's more *componentized* (foundation + operators + kinds).
- **Composability & extensibility.** *A1*: real building blocks (closure algebra; `PlanRef`; the
  schema duality) — **novel cases reachable by combination** *within the relational model*, and
  binary operators are a non-breaking future. But user-facing composability is bounded to the OLAP
  shape, and arithmetic/row-selection aren't reachable by combination (no scalar sublanguage). *A2*:
  the algebra composes internally (linear steps + bounded join recombine for re-grain, spine,
  correlation), but **user-facing extensibility is via new kinds (code)** — novel *objectives* aren't
  reachable by combination. So the composability locus differs: A1 composes at the
  relational-operator level and exposes it (within OLAP); A2 composes at the step level and gates it
  behind kinds.

### Fit with the project
- **Fit with existing architecture & patterns.** Both extend `Query`/`QueryExecutor` (a new
  `RunSavedQuery`/`RunQuery` variant) and Action→Mutation→Delta (a new `SavedQuery` entity). Both
  inherit the real per-backend cost (a hand-written SQL executor + migration for SQLite *and*
  Postgres) and the `QueryStore` recompute plumbing. *A2* maps its persistence to `AttributeConfig`'s
  external-tagged-JSON precedent explicitly (D6) and names the crate placement precisely. *A1* states
  the persistence *rule* (program-only serde; derive schema on load) crisply but is lighter on the
  Action/Delta/crate mechanics. *A1's* relational-IR + DataFusion lineage is a *new internal
  paradigm* relative to the current hand-written-query codebase (more new concepts to introduce);
  *A2's* `Vec<Step>` interpreter is also new but smaller-vocabulary per step. **Neither introduces a
  parallel write/read paradigm; both fit the boundaries.**
- **Testability & determinism.** *A2* is explicit: `Arbitrary`-generatable types, deterministic
  tiebreaks (`Extremum` tiebreak = earliest-instant then entry-id), a tz captured in the query for
  re-runnability (D3), full-recompute (no incremental state to diverge). *A1* cites
  deterministic-simulation reference semantics and bag-with-explicit-sort, and its schema-determinism
  property aids reproducibility — but it does not address tiebreak determinism for `max`/extremes
  (and prior art flags argmax/top-k/limit as **nondeterministic on ties** — a *saved* analysis that
  returns different rows each run is a correctness bug, §8). A2 closes this; A1 leaves it open.
- **Visualization data contract.** Covered in Contrast E. *A2* delivers a concrete, semantically-typed
  `ResultTable` (the literal in-scope deliverable); *A1* leaves it abstract (a `Relation` +
  storage-typed `DataType` + a separate encoding spec). Both under-specify sort-order and
  full-category-domain (empty buckets).

### Leverage & delivery
- **Prior art.** *A1* explicitly maps to relational algebra, OLAP/dimensional modeling, the
  semantic-layer category, DataFusion/Calcite/Polars/Malloy/PRQL, and the schema/instance duality +
  expression-problem framing — a strong ubiquitous-language anchor, and its DataFusion-shaped
  schema-propagation is exactly the proven pattern (§8). *A2* draws on the same lineage less
  explicitly but *operationalizes* several prior-art lessons (the spine = "time/date spine"; the
  statistics family; argmax-with-carry; time-range window frames; the curated-surface-over-algebra
  pattern resembling Tableau/Polaris). Both *miss or under-name* some prior-art items — see §8's
  "what both may be missing."
- **Incremental path.** *A1*'s vertical-slice discipline gives a genuinely small first slice ("count
  per day" needs source+dimension+count+the interpreter) **without** windows/joins/spine. *A2* admits
  its smallest slice is heavier (the shared foundation + algebra + at least one kind), "worst for
  trivial scope," mitigated by "grow kind-driven." So A1 reaches *a* first feature with less
  machinery; A2 reaches *more* features once the (larger) foundation exists.
- **UX enablement.** *A2*'s curated kinds directly target "express intent in domain terms without
  becoming a programmer" — a guided per-objective form, 1:1 with a chart. *A1* exposes a single OLAP
  builder (measures/dimensions/filters in domain terms) — domain-termed and ID-referenced, but the
  user still assembles a (constrained) query, and non-OLAP objectives have no surface. The fork is
  **guided per-objective forms (A2) vs. one general domain-term builder (A1)**; prior art shows both
  are viable (semantic layers expose curated explores/views *and* general metric+group-by queries).

### Distinctive strengths (summary)
- **A1:** the schema/instance duality + schema-determinism (clean validation *and* SQL-later from one
  shared function); persist-surface/derive-IR with the sharpest lossy-lowering rationale; the
  closure-property tree-ready algebra (binary ops as non-breaking future); the `E/R` typed-reuse
  discipline; deepest prior-art/ubiquitous-language grounding; most parsimonious *core*.
- **A2:** broadest verified feature coverage (windows, spine/absence, statistics, extremum,
  correlation, goals, derived measures); the concrete typed viz contract (`ResultTable`); the only
  explicit FFI strategy; explicit decisions (D1–D12) + re-runnable stress tests; strongest
  determinism posture (tiebreaks, captured tz); the spine as both absence mechanism and
  window-correctness prerequisite.

### Distinctive failure modes (summary)
- **A1:** derived/arithmetic measures unexpressible at the surface (cuts against its no-scalar
  stance); no row-selection ⇒ extremes-with-context unreachable as written; statistics, spine,
  windows, goals named-not-built; abstract viz contract (storage-typed); nested aggregation fights
  the `E/R` discipline; no tiebreak-determinism story; filtered aggregates deferred.
- **A2:** algebra build cost is real and worst at trivial scope; user-facing extensibility gated
  behind kinds (new objective = code); the recursive `Predicate`/`Measure` forces the opaque-builder
  FFI ceremony (method surface still an open question A2 flags); more total concepts/moving parts;
  filtered aggregates not first-class (reachable only via a heavyweight join); multi-valued category
  allocation policy under-named.

---

## 7. Open decision points (forks for the synthesis — deliberately unresolved)

Framed as forks with tradeoffs. These are the choices a synthesis must make; this document does not
make them.

1. **User surface shape.** One fixed OLAP builder (A1) · a curated set of per-objective kinds (A2) ·
   or a **hybrid** (a general OLAP builder for the OLAP-shaped majority + curated kinds for non-OLAP
   objectives: sequential, correlation, distribution, goal). *Trade:* uniformity + user-composability
   + no per-objective code (A1) vs. guided UX + 1:1 viz mapping + non-OLAP coverage + per-objective
   code (A2). The hybrid is latent in both (A1's whole surface ≈ A2's Aggregate kind).

2. **IR shape & the binary-operator question.** Free `Relation→Relation` algebra with a tree-ready
   recursive representation (A1) · a linear `Vec<Step>` with one bounded binary `Join` whose
   side-input is a sub-pipeline (A2). *Trade:* general closure + arbitrary-binary-later (A1) vs.
   linearity + bounded join + less ceremony (A2). Sub-fork: is re-graining a longer linear pipeline
   (A2) or `PlanRef` composition of separate specs (A1)? Prior art: either works, but a flat
   `enum Step` with only column-ref arguments can't gain joins later without a format break — design
   the step/source so an argument *can* be a sub-plan.

3. **Scalar-expression sublanguage — how much?** None (A1) · bounded arithmetic-over-measures (A2's
   `Measure::Bin`) · arithmetic + **conditional/CASE** (needed for *filtered aggregates* — both
   currently lack it) · fuller (string fns — both omit). *Trade:* derived metrics (tonnage/1RM/pace/
   load) and send-rate **require** some expression layer; "no scalar sublanguage" forecloses a whole
   feature family. Prior art: invest in a small expression AST early or suffer operator sprawl. This
   decision also prices the FFI fork (#9).

4. **Filtered / conditional aggregates.** First-class `agg { where }` (semantic-layer norm) · inline
   `CASE`/conditional measure · or via join-of-two-pipelines (A2's only current path) · or defer (A1).
   *Trade:* send-rate, "sends and attempts side by side," subset-vs-total goals all need it; deferring
   the *named* feature doesn't remove the underlying need. Decide where conditional aggregation lives.

5. **Reuse / composition model.** Typed source-reuse via `E/R` + `PlanRef` (A1) · uniform
   query-references-query (A2) · add a **named-fragment semantic-model layer** (define-once measures/
   dimensions/filters, referenced by name — both defer it). *Trade:* principled typed reuse (A1, but
   blocks aggregate-of-aggregate) vs. uniform references (A2, no typed guarantee) vs. the
   Malloy/LookML named layer (the property that keeps queries short; complementary to
   query-references-query, not a substitute — but more machinery). When does the named layer become
   necessary? (Trigger: a user re-specifying the same fragment repeatedly.)

6. **The viz data contract.** Abstract `Relation` + separate encoding spec (A1) · concrete typed
   `ResultTable` with `Cell`/`ColumnSpec` roles (A2). *Decide:* (a) how much **measurement type** the
   output carries (storage-typed Float/Int/String vs N/O/Q/T-equivalent Ordinal/Category/Instant/
   Quantity); (b) where **axis/channel assignment** lives (a global `ColumnRole` vs the kind↔viz
   contract vs a grammar-of-graphics encoding spec traveling with the saved query); (c) whether
   single-value/table/text are *degenerate charts* (one model) or special-cased; (d) **how the
   contract carries sort-order + the full category domain** so empty buckets/absent categories render
   (both under-specify this — acute for sparse fitness data).

7. **Absence / the spine.** Build it now as core machinery (A2: `Spine` + outer-join + `Fill`, also a
   *correctness prerequisite* for time-windowed metrics like ACWR) · or defer (A1: needs the binary
   join + a generated source). *Trade:* a whole feature family (absence) + correct time-windows now,
   at the cost of the spine+join machinery, vs. defer and gate them. Prior art: this is the standard
   **"time/date spine"** (Kimball date dimension → MetricFlow time spine); load it with week-start/
   fiscal/ISO-week columns, and decide empty-period = 0 vs null and how that composes with filters.

8. **Windows / sequential — design now or defer.** Specify the window taxonomy (A2: frame-aggregate
   vs stateful-scan; **time-range vs row-count frames**) · or name it as a future unary operator (A1).
   *Trade:* the sequential family (moving avg, ACWR, streaks, PR-detection, deltas, time-since) is
   large and core to fitness data. Prior art: **time-range frames are mandatory** on a sparse series
   (row-count frames are wrong when you don't log daily), and you need **one stateful scan with reset**
   (streaks/sessionization) that frames can't express. Sub-fork: are stateful scans "windows" (A2
   lumps them under `WindowSpec::Scan`) or a distinct operator family?

9. **FFI crossing of the persisted declaration.** Keep the declaration **flat / non-recursive** so it
   crosses uniffi directly as nested enums (A1's surface today) · or accept **recursive Predicate/
   Measure + an opaque builder Object** (A2). *Trade:* expressiveness (recursive boolean/arithmetic
   composition) forces the builder-Object ceremony under uniffi 0.31 (verified: no `indirect` enums);
   a non-recursive flat surface crosses free but can't express OR/arithmetic. **This is a consequence
   of #1 and #3**, not fully independent — but it is the concrete FFI price that those decisions pay.
   (Sub-option: bound nesting depth so the AST is non-recursive-but-deep, mirroring `AttributeValue`
   — would cross directly, at the cost of a fixed depth.)

10. **Validation / staleness / SQL-readiness.** Adopt A1's **schema-function-as-shared-spine** (one
    `output_schema(input)` used by *both* validation and future SQL codegen; "validate" = "derive the
    schema bottom-up against the live catalog"; recompute-on-demand) — *regardless of which surface
    wins*? *Trade:* the rigor buys a precise staleness gate and clean SQL-later for low upfront cost;
    the question is whether to commit to it now or let validation be ad hoc. Prior art strongly favors
    the per-operator pure-schema-function model (DataFusion), with **PIVOT quarantined** as the one
    data-dependent-schema operator (enumerate its columns to keep the plan offline-validatable).

11. **Units / dimensions.** Normalize-at-leaves + labeled-scalar arithmetic (A2, D11) · or unaddressed
    (A1). *Trade:* the core model carries units on Mass/Length, so *some* normalization is required
    for correct cross-unit aggregation ("max load" across kg and lb). How much dimension tracking —
    leaf normalization (enough for correct aggregation) vs full dimensional propagation (heavier, only
    buys axis labels the surface already knows)? A2's "normalize at leaves, don't propagate" is a
    reasonable default a synthesis can adopt.

12. **Determinism of row-selection.** Require a **total order / explicit tiebreak** everywhere a row
    is selected or limited (A2 does: `Extremum` tiebreak earliest-instant→entry-id) · or leave it
    (A1 doesn't address `max`/extremes ties). *Trade:* a saved analysis that returns different rows on
    ties is a correctness bug (prior art: argmax/top-k/DISTINCT-ON/LIMIT are all nondeterministic on
    ties). Low cost to require; the fork is only whether to make it a global invariant.

13. **Multi-valued category allocation.** Unweighted "impact" totals (per-category subtotals exceed
    the grand total, by design) · weighted/allocated totals (reconcile to the grand total via a
    weighting factor). *Trade:* both designs allow multi-valued grouping but **neither encodes the
    allocation choice** (Kimball bridge-table prior art). Decide and name it, since "volume per muscle
    group" with multi-category exercises hits it immediately.

---

## 8. Prior-art findings

Three sub-agents surveyed prior art first-hand (web sources cited inline). Established terminology,
proven boundaries, pitfalls — and what **both** designs miss.

### 8.1 Grammar-of-graphics / visualization data models (Vega-Lite, D3, Observable Plot, ggplot2, Tableau VizQL/Polaris)

**Ubiquitous language.** *Mark/geom* (the geometric primitive: point/line/bar/area/text) carries
*encodings/aesthetics*, each binding a data column to a *channel* (position `x`/`y`/`x2`/`y2`;
mark-property `color`/`size`/`shape`/`opacity`; facet `row`/`column`/`fx`/`fy`; other `text`/`tooltip`/
`detail`/`order`), mediated by a *scale* (domain→range) and made legible by a *guide* (axis/legend).
*Stat/transform* = an in-spec computation (bin, aggregate, window, stack). *Facet/small multiple* =
split into subplots by a categorical column. (Sources: Vega-Lite encoding/type/transform docs;
ggplot2 "Mastery"; Plot marks/facets; Tableau Polaris/VizQL.)

**The data shape charts consume is tidy/long** (Wickham: each variable a column, each observation a
row, each value a cell) — grammar tools want one addressable column per variable. *Fold/melt/
`pivot_longer`* (wide→long) and *pivot/`pivot_wider`* (long→wide) are the boundary operators. **Both
designs emit long/tidy output — correct and well-precedented.** Pitfall for a long table: folding
columns into one `value` column **erases each former column's type**; everything folded together must
share one measurement type, and the `key` column's full category set must travel explicitly.

**The boundary carries more than rows+columns.** Decisively, what crosses the query↔viz seam is a
table **plus per-column metadata**: the column **name**, its **measurement type**
(nominal/ordinal/quantitative/temporal — N/O/Q/T), its **role** (key vs measure), its **sort order**,
and the **full expected category domain**. Tools that hide this (D3) force every chart to rebuild it;
tools that formalize it (Vega-Lite/ggplot) make the *type* the mandatory second half of every binding.
**Lesson for A1/A2:** carry explicit measurement types across the boundary and let the chart override.
A1's storage-typed `DataType` is insufficient alone (a string may be nominal *or* ordinal; a number
may be quantitative *or* an ordinal code); A2's typed `Cell` (Ordinal/Category/Instant/Quantity)
covers the N/O/Q/T distinction but **neither** ships sort-order or the full category domain.

**Empty buckets vanish by default.** A discrete scale draws its domain from values *present in the
data*, so a category/period with no rows produces **no bar/segment** — acute for fitness data (a rest
day breaks a line; a never-trained muscle disappears). Fixes are explicit: set the scale domain to the
full category list, or **`impute`** synthetic rows. **This is exactly A2's spine generalized**, and a
gap **both** designs under-specify at the contract level (who supplies the full domain?).

**Where aggregation lives.** Vega-Lite allows transforms *in the spec* (adaptive re-binning at chart
time) *and* upstream. Putting everything upstream (A1/A2's "hand the chart a finished table") is clean
but **rigid**: a table pre-aggregated to daily totals **cannot** drive a monthly chart without
re-binning, so the grain decision leaks into the query. The distinct cases the viz layer often wants
*at chart time* — `aggregate` (collapse grain), `joinaggregate` (N→N, attach a group stat like
"% of total"), `window` (running/rank) — argue for either exposing enough columns to re-bin or
deciding explicitly "who owns grain, and can the chart re-aggregate?"

**Single value / table / text are degenerate charts**, not separate subsystems (a `text` mark over
0–2 position channels). A design that special-cases them multiplies code paths.

**Multi-series = one dimension column + a color/facet channel**, not multiple separate queries — this
shares scales/legends, enables faceting, shows empty facets, and lets you add a series by adding rows.
**Direct support for a single tidy table with an explicit dimension/series column** over a
per-series-query model — *provided* that column carries its type, sort order, and full domain.

*(Sources: vega.github.io/vega-lite docs [encoding, type, transform, aggregate, joinaggregate, window,
fold, impute, scale, invalid-data]; d3js.org/d3-scale; observablehq.com/plot [marks, facets];
vita.had.co.nz/papers/tidy-data.pdf; r4ds.hadley.nz/data-tidy; ggplot2-book.org/mastery;
tableau.com Polaris/VizQL.)*

### 8.2 Query / dataframe / relational-algebra models (relational algebra, dplyr, Polars, LINQ, PRQL, DataFusion/Calcite, KQL, Mongo, monoid calculus)

**The universal verb set** every system has first-class: **filter** (row predicate), **project/select**
(column choice), **derive/mutate** (add a per-row column), **group+aggregate** (re-grain), **join**,
**sort**, **limit/take**, **union**. Near-universal-but-often-omitted: the **per-row derived column**
(dplyr `mutate` / Polars `with_columns` / PRQL `derive` / SQL `SELECT expr`); pure relational algebra
has *no separate operator* for it (it folds into "extended projection"), **which is exactly why
RA-shaped designs tend to omit it and bolt it on** — directly relevant to A1's missing surface derive.
*Terminology trap:* RA "selection" (σ) = filter rows, but SQL `SELECT` = projection (columns); Polars
`.select()` is projection *and* the aggregation context. Don't let "select" be ambiguous.

**Closure & chain-vs-tree.** Closure (every operator returns the carrier type) is what lets a pipeline
be a list of steps. Unary operators form a **linear chain**; a **binary operator (join/union) forces a
tree**. A linear language hosts a join by making the second input an *argument that is itself a
sub-pipeline* (PRQL `join`; Calcite `RelBuilder`'s stack). **Cost of "chain-now, tree-ready":** a flat
`enum Step` with only column-ref arguments **cannot** gain joins later without a format break;
designing a step/source whose argument *can* be a sub-plan is the cheap insurance. (A1's tree-ready
typing and A2's `SideInput::Pipeline` are two forms of this insurance.)

**The grain boundary.** Per-row transform (preserves cardinality) vs. across-group reduction (collapses
grain). SQL's logical order `WHERE → GROUP BY → HAVING` splits pre- vs post-aggregation filters
(many engines add `QUALIFY` for post-*window*). **The pipeline lesson:** let **filter mean "filter at
the current grain"** and pick WHERE/HAVING/QUALIFY at *compile* time — strictly more composable than a
single "filter raw rows" operator. dplyr/PRQL express "filter→group→filter-on-aggregate→re-group" as a
plain linear sequence (A2's `Having` + re-`Group` is this; A1's deferred HAVING + `PlanRef` is the
awkward path).

**Windows.** Two shapes: **frame-based** (trailing N rows/days — moving average, running total) and
**stateful ordered scans** (streak, record-detection, delta, time-since, sessionization — a fold that
carries state and may *reset*). **KQL has the cleanest stateful primitive** (`row_cumsum` with a reset
arg; `scan`); SQL has no general stateful scan (you do gaps-and-islands). **ROWS vs RANGE is decisive
for fitness data:** a "7-day moving average" must be a **time-range** frame (`RANGE '7 days'` / Polars
`rolling(period="7d")`), *not* a row-count frame — "last 7 logged entries" can span months on a sparse
series. **A2's `Frame::Trailing(Duration)` (time-ranged primary) + `Scan` taxonomy matches this prior
art precisely; A1 names windows without the frame/scan or time/row distinction.**

**argmax-with-context** ("the row achieving the max," not just the value) is **row-selection, not
reduction** — `DISTINCT ON` / `row_number()=1` / `slice_max` / `top_k` / `arg_max`. **It is
nondeterministic on ties** unless the order is total — fix with a tiebreaker tuple. **A2's `Extremum`
(keep+project the winning row, deterministic tiebreak) is the textbook treatment; A1's
measure-as-reduction cannot carry the winning row.**

**Flat (1NF) vs nested (monoid-comprehension).** Flat = rows + joins/unnest/pivot (DataFusion, SQL);
nested = a cell holds a collection (Mongo `$push`, Malloy `nest`, the Buneman/Fegaras NRC; LINQ query
syntax *is* a monad comprehension via `SelectMany`). Nested buys hierarchical/drill-down output and
**avoids join fan-out double-counting**; it costs harder SQL-compilation and a richer type system.
**Pivot's special pitfall:** its **output columns are data-dependent** (one per distinct value), which
breaks the otherwise-reliable "schema is a pure function of the plan" property. **Both designs are
flat** (correct for SQL-later); both need `unnest` (the forest is nested) and should keep `pivot` at
the edge with enumerated columns. A1 names this; A2 implies it via long/tidy output.

**Reified plan + schema propagation.** DataFusion `LogicalPlan` (enum, each variant carries a
`DFSchema`; `recompute_schema()` re-derives after rewrites) and Calcite `RelNode` are the template for
a *saved, validated, recompilable* plan: give every operator `output_schema(input) → Result<Schema>`,
make validation a pure pass over the plan (a `ColumnNotFound`/type-mismatch = the saved plan no longer
fits the live schema), and **quarantine PIVOT** (the one data-dependent-schema operator).
**This is A1's schema/instance duality, independently confirmed as the proven pattern.** A2 commits to
the outcome (derive the IR, validate on load) without articulating the shared schema function.

**What both designs likely overlook (dataframe brief):** (1) **a scalar-expression sublanguage** — the
dividing line between PRQL-clean and operator-sprawl; (2) **minimal orthogonal operator set** vs
grab-bag; (3) making the **grain boundary explicit**; (4) **null vs absent-row semantics** on a sparse
series (densify before windowing — KQL `make-series`); (5) **bag-vs-set semantics + join fan-out**
double-counting; (6) **deterministic tiebreaks** everywhere a row is selected/limited; (7) **ID-ref vs
name-ref** plan stability (ID-refs survive renames but must detect deletion — both correctly choose
IDs); (8) **closure leaks at the viz edge** (a "chart step" that returns a non-table breaks closure —
keep rendering a separate mapping from the final table).

*(Sources: en.wikipedia.org/wiki/Relational_algebra; dplyr.tidyverse.org; docs.pola.rs; PRQL
prql-lang.org transforms/window; learn.microsoft.com LINQ; docs.rs datafusion-expr LogicalPlan +
DFSchema; calcite.apache.org RelBuilder; learn.microsoft.com/kusto serialize/summarize +
arcanecode.com row_cumsum; mongodb.com aggregation; dl.acm.org/10.1145/377674.377676 Fegaras&Maier;
clickhouse.com argMax; postgresql.org DISTINCT ON; database.guide dynamic PIVOT.)*

### 8.3 Semantic / metrics layers (Malloy, dbt MetricFlow, LookML/Looker, Cube; OLAP/MDX, Kimball)

**The "measure" collision — pin it.** *Instance side* (OLAP/Kimball): a measure is a **numeric cell
value / fact**. *Definition side* (modern semantic layers — Malloy, LookML, Cube, dbt): a measure is
an **aggregation expression** (a recipe: `agg` + `expr`). **A1 explicitly adopts the
definition/aggregation sense** (correctly, citing Malloy) — but note A2's `Measure` means a *per-row
expression*, **not** an aggregation (A2's aggregation is `AggSpec`). So three senses are in play; the
synthesis must pin one (see §2).

**The second split most likely missed: measure vs *metric* (two tiers).** Only dbt MetricFlow
separates a *measure* (an aggregation) from a *metric* (an expression over measures: simple/ratio/
derived/cumulative). This exists precisely to build **ratios and windowed comparisons breadth-first**.
**ACWR = (acute load) / (chronic load) is a ratio of two windowed measures — exactly the `type: ratio`/
`derived` case.** If a design has only one tier and no "metric = expression over measures," derived
ratios and week-over-week comparisons have **no clean named home** (you re-derive them ad hoc per
query). Kimball's lineage agrees: ratios are non-additive — store additive components, combine last.
**Both designs should check whether "ratio of two windowed aggregates" is a named, reusable thing.**
(A2 reaches ACWR via two window steps + a `Derive` ratio — works, but un-named; A1 has neither windows
nor the ratio at its surface.)

**Source-vs-query / reuse (directly informs Contrast G & decision #5).** Malloy ships **both** reuse
models: a **`source`** holds named measures/dimensions/views (the **define-once-reference-by-name**
layer), **and** a query result can become another query's source (query-references-query), plus
refinement (`query + {…}`) and pipelining (`->`). **Lesson:** the named-fragment "semantic model"
(which A1 defers and A2 omits) and "query references query" (A2's mechanism) are **complementary, not
either/or**. Query-references-query gives composition/pipelining but **not** define-once aggregation
reuse; omitting the named layer tends to push aggregation-expression duplication into every saved
query. Every surveyed layer provides the named layer via a per-table model object (source/view/cube/
semantic model).

**Measure-vs-dimension is intrinsic, not a per-query role — in every tool** (LookML/Cube/MetricFlow
declare them separately; Malloy fixes it per *expression*: a field is a measure iff its expression
aggregates, and is illegal in the wrong clause). This **contradicts both designs' framing** of
"dimension-vs-measure is just a role per query." The industry norm is intrinsic typing with the
*clause* (group-by vs aggregate) deciding legality — a subtle but real difference worth a deliberate
choice. (To use an aggregate as a group key, LookML makes you "dimensionalize" via a derived table —
i.e. re-grain — not flip a tag.)

**Filtered/conditional aggregates are table-stakes** in *every* layer (Malloy `count(){where}`, LookML
`filters:`, Cube `CASE WHEN`, MetricFlow filters) — confirming the §5 cross-cutting gap. Deferring the
*named* feature (A1) doesn't remove the need; A2's lack of a conditional in `Measure`/`AggSpec` has the
same effect. Without it you cannot define "subset vs total" side-by-side measures for a ratio.

**Grain / fan-out / multi-valued (informs decisions #4, #13 and UC-B, UC-C).** A one-to-many join
duplicates the "one" side, so naïve `SUM` double-counts (Looker's canonical 223.44-vs-124.84). Every
tool handles it with **declared primary key + relationship**: LookML **symmetric aggregates** (auto
`SUM(DISTINCT key+val) − SUM(DISTINCT key)`), Cube PK-dedup, Malloy "aggregate locality" (errors on a
bare aggregate across `join_many`), MetricFlow refuses fan-out joins. **"Aggregate then re-group"
(UC-B) and fan-out are the same grain-discipline problem.** Only **Malloy's `->` pipeline** does
arbitrary "aggregate → `having` → re-group at a new grain" at query time (= A2's linear re-grain);
LookML/Cube/MetricFlow require a **pre-modeled intermediate** (derived table / cube / semantic model)
— A1's `PlanRef`-of-an-inner-spec is this pre-modeled-intermediate style. **Multi-valued
categorization** (a row in many categories) is Kimball's **bridge table**: per-category subtotals
exceeding the grand total is correct **only** under an explicit "impact (unweighted)" choice;
reconciling needs a **weighting/allocation factor** — a named decision **both** designs omit (UC-C).

**Time spine (informs decision #7).** The ubiquitous term for A2's "calendar spine" is **time spine /
date spine**, descending from **Kimball's calendar date dimension**: a dense, one-row-per-period table
you LEFT JOIN to fill absent periods (MetricFlow "time spine"; LookML `allow_fill`; Cube
`fillMissingDates`). **A2's spine is exactly this, well-precedented.** Refinements to import: the spine
should carry **week-start convention / ISO week / fiscal columns** (needed for correct "per week"),
and **gap-fill interacts badly with measure filters** (Looker disables fill when measures are filtered)
— decide empty-period = 0 vs null and how it composes with filters.

**Persistence (informs the shared similarity #2 and decision #5).** **Every** layer persists a
**semantic declaration** (Malloy `.malloy`, LookML `.lkml`, Cube YAML/JS, MetricFlow YAML) and
compiles SQL at run time — **never a stored compiled plan** (Cube's pre-aggregations are a cache, not
the definition). This strongly endorses **both designs' "persist the declaration, derive/compile the
plan."** Two cautions: (a) **staleness must be validated** against the live schema (MetricFlow runs
validation queries before executing — = A1's "validate = derive schema on load") — not just
deserialize; (b) **the spec itself evolves** — MetricFlow *deprecated `measures`* into "simple metrics"
in dbt 1.12, renaming/relocating keys. **A persisted declaration needs a version field + migration
story from day one** — neither design states one.

**What semantic layers deliberately do NOT do:** own visualization/rendering, dashboards, or result
storage. They define dimensions/measures/joins and emit viz-ready tables; charting/goals/thresholds
sit above. **Lesson:** separate the *semantic* declaration (portable, validatable, evolvable) from the
*presentation/goal* declaration so each can evolve independently — relevant to whether the viz/encoding
spec (and the goal target) should travel *inside* the saved query (A1 "viz travels with the plan"; A2
folds goal into the query) or as a separate, separately-versioned artifact.

*(Sources: docs.malloydata.dev [source, fields, query, nesting, aggregates, filters, join];
docs.getdbt.com [measures, metrics-overview, ratio, derived, cumulative, semantic-models, dimensions,
entities, join-logic, metricflow-time-spine, latest-metrics-spec]; cloud.google.com/looker/docs
[lookml-terms, measure-types, symmetric-aggregates, dimension-group, allow-fill, filters];
docs.cube.dev [measures, dimensions, cube, view, joins, segments, pre-aggregations, custom-granularity];
en.wikipedia.org/wiki/OLAP_cube + Measure_(data_warehouse); kimballgroup.com [fact-table-structure,
calendar-date-dimension, multivalued-dimension-bridge-table, additive-semi-additive-non-additive].)*

### 8.4 Cross-cutting prior-art points that BOTH designs miss or under-name
1. **A scalar-expression sublanguage** (dataframe + semantic briefs) — the single highest-leverage
   omission risk; without it, derived metrics and filtered aggregates have no clean home and operators
   sprawl. (A1 disclaims it; A2 has only bounded arithmetic, no conditional.)
2. **Filtered / conditional aggregates** — table-stakes in every semantic layer; neither is
   first-class.
3. **The measure→metric two-tier** (a metric = expression over measures) — needed for ratios/ACWR as a
   *named* thing; neither names it.
4. **Sort order + the full category domain on the viz contract** — required to render empty buckets/
   absent categories (the spine's viz-side twin); both under-specify.
5. **Multi-valued category allocation policy** (weighted vs impact totals) — Kimball bridge-table
   decision; neither encodes it.
6. **A version field + migration story** for the persisted declaration — every real semantic layer
   evolved its spec and broke round-trips; neither design states one.
7. **Explicit null/absent-row + bag-vs-set + fan-out semantics** — the silent correctness traps;
   stated only partially (A2's spine addresses absence; neither fully addresses fan-out double-counting
   when a parent-grain measure is aggregated across a one-to-many).
8. **Determinism of row-selection/limit on ties** — A2 addresses it for `Extremum`; neither states it
   as a *global* invariant for every `Limit`/`top-k`/`max`-with-carry.

---

*End of evidence base. The maintainers will synthesize the final design from §7's decision points,
using §4's commitments, §6's strengths/failure modes, the §5 use-case runs, and §8's prior art. No
recommendation is made here by design.*
