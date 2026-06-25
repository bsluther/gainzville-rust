# Analysis & Visualization — Design (alt)

> **Status.** Candidate architecture for the analysis/visualization subsystem, written to be
> self-contained so it can be compared and synthesized against the other design. It defines the
> data structures, boundaries, responsibilities, the Swift-facing domain language, and the
> internal representation. Specific chart/visualization components are out of scope by intent —
> this design stops at the data structure visualizations consume (`ResultTable`).
>
> Nothing in the existing core model is redefined here; see [`model.md`](./model.md),
> [`attributes-design.md`](./attributes-design.md), [`actions_and_queries.md`](./actions_and_queries.md),
> [`boundary-transformations.md`](./boundary-transformations.md). Terms that could be ambiguous
> against the core model or another design are pinned in the [Glossary](#3-glossary).

---

## 1. The problem

Today Gainzville structures, views, and edits training data. This subsystem adds **analysis**:
user-defined queries over a user's activities, attributes, and entries, producing data that drives
visualizations (charts, tables, single values) and goals.

Surveying the target use cases — counting, frequency/consistency, (nested) aggregation,
post-aggregation filtering and re-graining, arithmetic measures (tonnage, 1RM, pace, load),
ordered/windowed metrics (moving averages, ACWR, streaks, PR detection), argmax-with-context,
statistics (histograms, percentiles, monotony), cross-source correlation, taxonomy/volume rollups,
absence analysis, and goals — the shape that subsumes them is a **relational + grouped-aggregation
+ windowed + statistical query** over the entry forest. That is the thing this design builds.

A key non-goal clarified early: **users must never assemble that query language directly.** Exposing
a general algebra as UI turns users into programmers. So the design is two-layered.

---

## 2. Architecture at a glance

```
Swift UI
  └─ Curated KIND          (a viz/analytic objective: input form  ↔  fixed result schema)
        └─ expands to →  PIPELINE          (the operator algebra / IR — never user-facing)
              └─ run by →  IN-MEMORY INTERPRETER   (pure gv-core, over a loaded Forest)
                    └─ →  ResultTable  →  visualization
  Shared FOUNDATION: value coercion · category resolver · forest resolvers · calendar · ResultTable
```

**Central commitment:** a **curated set of kinds** is the entire user surface; a **composable
operator algebra** is the execution IR; execution is **in-memory** (the DB is a data source, not a
compute engine). In-DB execution is a deferred, additive future, kept open precisely because the
algebra is a compilation target.

This is deliberately a hybrid of the two extremes we considered ("open algebra exposed to users"
vs. "closed set of hand-written analysis templates"): the algebra gives one execution semantics and
the long tail; the kinds give a guided UX, a clean FFI surface, and a 1:1 mapping to visualizations.

---

## 3. Glossary

Short pins, not sections. "(vs core)" flags a term that means something specific here.

- **Analysis query / saved query** — the persisted, synced, domain-level *declaration* of one
  analysis. The canonical artifact (the IR is derived from it, never the reverse).
- **Kind** — a curated analysis template tied to an analytic/viz objective (e.g. Aggregate,
  Distribution, Correlation, Goal). The UX surface. The user fills domain slots; they never see the
  algebra.
- **Pipeline** — the internal representation a kind expands to: an ordered `Vec<Step>`. Also "the IR."
- **Step / operator** — one transformation in a pipeline (filter, group, aggregate, window, join …).
- **Row** *(vs core)* — the unit flowing through a pipeline. A row begins as an **entry-in-context**:
  an `Entry` plus cheap access to its values, its activity (and the activity's categories), its
  ancestors, and its *effective instant*. Later steps reshape rows (grouped rows, joined rows).
- **Measure** *(vs core)* — a **numeric-valued expression evaluated over a row** at analysis time:
  an attribute read, a constant, or arithmetic over measures (`reps·weight`). Yields a `Quantity`.
  A measure is **not** an `Attribute` and is not stored; it is computed.
- **Comparable** — a value that can be *ordered*: a `Quantity` (numeric/mass/length/duration) **or**
  an `Ordinal` (an ordered `Select` reduced to rank + label). Min/Max/Extremum operate on these.
- **Quantity** — a coerced numeric value carrying a **dimension**, unit-normalized to a base
  (mass→kg, length→m, duration→ms).
- **Dimension (`Dim`)** — the unit family of a *leaf* quantity: `Mass | Length | Duration | Count |
  Dimensionless`. Tracked at leaves (for unit-correct aggregation); **not** propagated through
  arithmetic (see §6.4 / Decision D11).
- **Coercion** — turning an `AttributeValue` into a `Comparable`: pick the source (plan/actual),
  reduce a range to a scalar, normalize units, apply the 2-decimal cap.
- **Range policy** — how a `Range` value reduces to a scalar: `Min | Max | Mean | Omit`
  (`Omit` ⇒ treated as absent). Default `Mean`.
- **Filter** *(vs the dropped term "classifier")* — any **entry-returning query reused in a
  selecting role**. There is no separate "classifier" concept; reuse is query-references-query.
- **Predicate** — a boolean test over a row: attribute comparison, attribute presence,
  category membership, `has-an-ancestor-where`, `matches-query`, and boolean combinators.
- **Category / `CategoryResolver`** — activity-determined membership via an is-a graph over
  activities. The graph (edges, editing, sync) is deferred; analysis depends only on a resolver
  `entry → set of category keys`, which is trivially `{activity_id}` today.
- **Grouping key** — a value used to partition rows. May be **multi-valued** (one row fans into
  several groups — categories).
- **Aggregate** — a **reducing** function over a group (`count/sum/min/max/mean/median/percentile/
  stddev/histogram`). Collapses a group to scalar column(s); discards row identity.
- **Extremum** — a **row-selecting** operator (top-k by a comparable). Unlike an aggregate it
  *retains* the chosen row so its fields can be projected (argmax-with-carry).
- **Window** — an **order-dependent** operator. Two sub-shapes: a **frame-aggregate** (moving
  sum/mean over a trailing time frame) and a **stateful scan** (streak, record-detection, delta,
  time-since-last — depend on all prior rows, not a fixed frame).
- **Spine** — a **generated dense axis** (e.g. every calendar day in a range) outer-joined to data
  to materialize otherwise-absent periods. See §7.
- **`ResultTable`** — the tidy/long-form, visualization-facing output: typed columns + rows of
  typed cells. The one shape every kind produces.
- **Goal** — a **composition of a query + a hoped-for result**. Achievement and progress are
  derived (actual vs hoped-for). Queries themselves carry no goal field.
- **Effective instant** — the time a row is bucketed by: its own `canonical_instant`, or, if absent
  (e.g. a duration-only set member), the nearest ancestor's. No effective instant ⇒ excluded from
  time-bucketed analysis.
- **Session** *(note)* — **not** a primitive. A user-level pattern expressed with
  `group-by-nearest-ancestor-where(P)` over whatever attribute/category the user chose.

---

## 4. Decisions & assumptions

These were settled deliberately; a comparison against another design should diff against this list.

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | **No "session" primitive.** Sessions are expressed via generic `has-an-ancestor-where(P)` / `group-by-nearest-ancestor-where(P)`. Time-gap binning **deferred**. | Keeps the forest as source of truth; "session" lives in user intent, not the model. |
| D2 | **Reps is an ordinary numeric attribute**, no implicit count multiplier. `count()` counts entries; `sum(reps)` counts reps. | Preserves two distinct questions; keeps the engine free of a privileged attribute; matches the arithmetic measures (tonnage, 1RM). |
| D3 | **Calendar bucketing on *effective instant*, in an explicit timezone captured in the query.** Week starts Monday (configurable later). Travel/DST history deferred. | Determinism/re-runnability; correct local-day grouping; no new synced tz data (UI supplies it). |
| D4 | **Categories are activities; membership is a transitive is-a DAG, behind a `CategoryResolver` seam.** The edge table + its mutations/sync/UI are **deferred**; the resolver is trivially `{activity}` now. Grouping keys are **multi-valued**; category-grouped totals deliberately exceed the grand total. | Pulls categorization complexity out of analysis; the seam means analysis is unchanged when the DAG lands. |
| D5 | **No "classifier" concept** — a filter is any entry-returning query reused in a selecting role. **A goal = (query + hoped-for result)**; achievement/progress derived. | One fewer concept; reuse via query-references-query; goals keep the *measured value* (progress), which a "non-empty set" definition discards. |
| D6 | **Saved queries are first-class synced domain entities**; the **declaration** persists as one external-tagged JSON column; the IR is derived, never persisted. **Results are never persisted** — always derived. | Matches the Action→Mutation→Delta→HLC path; declaration is canonical and portable; results depend on current DB state. |
| D7 | **Results recompute in full, on demand**, over the existing change-broadcast / `QueryStore`. **IVM deferred.** | One user's data is small; reuses existing reactive plumbing; avoids committing to the unproven `ivm` crate. |
| D8 | **Measures read a selectable source — `actual` (default) or `plan`.** Absent source ⇒ absent measure ⇒ excluded. Plan/actual presence is predicate-expressible (missed-planned = plan ∧ ¬actual). | Common case trivial; planning/goal use cases reachable through one selector. |
| D9 | **Ranges reduce to scalars by a per-measure/per-predicate policy `Min/Max/Mean/Omit`** (default `Mean`; Mean-of-ordered-select = option nearest the rounded midpoint), applied uniformly to measures and comparisons (**reduce-then-operate**). | Keeps the whole engine scalar after coercion; avoids range-aware overlap semantics. |
| D10 | **In-memory execution** (interpreter in `gv-core`); the DB is a loader. **In-DB execution (SQL compilation) deferred**, trigger = server-side scale. | Simpler upfront, more general (the windowed family is far easier in Rust than SQL), good enough at single-user scale; the IR keeps the in-DB door open. |
| D11 | **No full dimensional analysis.** Units are normalized at *leaves* (for correct aggregation); arithmetic does **not** propagate composite dimensions — derived/ratio results are **labeled scalars** (the kind supplies the label). | Real dim-tracking is heavy machinery whose only payoff is cosmetic axis labels the kind already knows. |
| D12 | **Joins are bounded and binary**: a `Join` step combines the running stream with one **linear** side-input on a shared key. No join-of-joins; N-way and lagged joins deferred. | Covers every correlation/spine use case; avoids hairy recursion until needed. |

Assumptions worth stating: a single user's history fits comfortably in memory; the primary target
is the offline-first SQLite client; new types must be deterministic, `serde` (external-tagged), and
`Arbitrary`-generatable for the property/simulation tests.

---

## 5. Shared foundation

All of this lives in **`gv-core`** (no `sqlx`, no `uniffi`), so it is pure, deterministic, and
generatable.

### 5.1 Value coercion
```rust
struct Quantity { value: f64, dim: Dim }                 // unit-normalized to base
enum Dim { Dimensionless, Mass /*kg*/, Length /*m*/, Duration /*ms*/, Count }
enum Ordinal { /* rank: i64, label: String */ }          // ordered Select reduced to rank
enum Comparable { Quantity(Quantity), Ordinal(Ordinal) }

enum Source { Actual, Plan }                              // D8
enum RangePolicy { Min, Max, Mean, Omit }                // D9, default Mean

fn coerce(v: &AttributeValue, src: Source, rp: RangePolicy) -> Option<Comparable>;
//   Exact -> normalized; Range -> Min/Max/Mean/Omit(None); absent source -> None
//   Text / Multiselect / unordered Select are categorical, not Comparable (used as keys)
```

### 5.2 Category resolver (D4)
```rust
trait CategoryResolver { fn categories_of(&self, e: &Entry) -> SmallVec<[Uuid; 4]>; }
// today: {entry.activity_id}.  later: transitive is-a closure over the (synced) edge graph.
// closure over an empty edge set is the identity, so "group by category" == "group by activity" now.
```

### 5.3 Forest resolvers (D1, D3)
```rust
fn has_ancestor_where(f: &Forest, e: &Entry, p: &Predicate) -> bool;          // structural predicate
fn nearest_ancestor_where<'a>(f: &'a Forest, e: &Entry, p: &Predicate) -> Option<&'a Entry>; // grouping key
fn effective_instant(f: &Forest, e: &Entry) -> Option<DateTime<Utc>>;         // own, else ancestor's
```

### 5.4 Calendar bucketing (D3)
```rust
struct CalendarSpec { period: Period, tz: TzId, week_start: Weekday }   // Period: Day|Week|Month|Year
fn bucket(instant: DateTime<Utc>, c: &CalendarSpec) -> BucketKey;        // keys off effective_instant
```

### 5.5 `ResultTable` — the visualization contract
The single output shape. **Long/tidy form**: nested results like `{v4:{send:7,attempt:2}}` are
emitted as long rows `(grade, outcome, count)` and pivoted by the UI. A scalar ("max load") is a
1-row, 1-measure table.
```rust
struct ResultTable { columns: Vec<ColumnSpec>, rows: Vec<Vec<Cell>> }
struct ColumnSpec { name: String, role: ColumnRole, kind: CellKind }     // Dimension | Measure
enum Cell {
    Quantity { value: f64, dim: Dim },
    Ordinal  { rank: i64, label: String },
    Category { id: Uuid, label: String },          // multi-valued grouping fans into rows
    TimeBucket { start: DateTime<Utc>, end: DateTime<Utc>, period: Period },
    Instant(DateTime<Utc>),                         // a point in time (e.g. a carried PR date)
    Text(String), Count(i64), Null,
}
```
`ColumnSpec.kind` is **load-bearing**, not decoration: it is how a viz renders heterogeneous
carried fields (a date vs a route name vs a number). Bivariate axis assignment (which measure is x
vs y) lives in the **kind ↔ viz contract**, not in a global `ColumnRole`.

### 5.6 Persistence, sync, run (D6, D7)
A `SavedQuery` domain entity (declaration as one external-tagged JSON column) flows the existing
`Action → Mutation → Vec<AnyDelta> → DeltaExecutor` path and syncs by HLC/seq-num like any other
entity. Running is an ordinary `Query` variant — `RunSavedQuery { id }` / `RunQuery { decl }` →
`ResultTable` — so it plugs into the existing `QueryStore` change-broadcast for on-demand full
recompute. Results are never stored.

---

## 6. The operator algebra (the IR)

A query is a **`Pipeline = Vec<Step>`**. Rows start as entries-in-context and are reshaped by steps.

### 6.1 Sources, steps
```rust
type Pipeline = Vec<Step>;

enum SourceSpec {
    Entries { prefilter: Option<Predicate> },       // entry-in-context stream (pushdown target)
    Spine   { range: TimeRange, cal: CalendarSpec }, // generated dense calendar axis (see §7)
}

enum Step {
    Source(SourceSpec),
    Filter(Predicate),
    Derive   { name: String, expr: Measure },        // computed column (per-row); evaluated before grouping
    Group(Vec<KeySpec>),                             // partition; keys may be multi-valued
    Aggregate(Vec<AggSpec>),                         // reduce groups -> scalar column(s)
    Extremum(ExtremumSpec),                          // SELECT extremal row(s) per group; keep + project
    Having(Predicate),                               // post-aggregation filter
    Window(WindowSpec),                              // order-dependent; appends a column
    Fill   { column: String, default: f64 },         // coalesce nulls (post outer-join)
    Join   { other: SideInput, on: GroupKey, how: JoinKind },  // running stream (left) ⋈ side input
    Sort(Vec<SortKey>),
    Limit(u32),
}

enum SideInput { Pipeline(Box<Pipeline>), Saved(QueryRef) }   // LINEAR side input — no Join inside (D12)
enum JoinKind  { Inner, LeftOuter, FullOuter }
```

Note how re-graining needs no nesting: "sessions with ≥10 climbs, counted per week" is simply
`Group(session) → Aggregate(count) → Having(≥10) → Group(week) → Aggregate(count)` — a **linear**
sequence. The only branching in the whole model is the binary `Join` step.

### 6.2 Predicates & measures (the recursive parts)
```rust
enum Predicate {
    ActivityIs(Uuid), ActivityInCategory(Uuid),     // category via CategoryResolver (D4)
    Cmp { lhs: Comparable_, op: CmpOp, rhs: ScalarLit },   // reduce-then-compare (D9)
    Present { attr: Uuid, src: Source },
    HasAncestorWhere(Box<Predicate>),                // D1
    MatchesQuery(QueryRef),                          // reuse an entry-returning query (D5)
    And(Vec<Predicate>), Or(Vec<Predicate>), Not(Box<Predicate>),
}

enum Measure { Attr { attr: Uuid, src: Source, range: RangePolicy }, Const(f64),
               Bin(Box<Measure>, ArithOp, Box<Measure>) }       // tonnage = reps·weight; yields Quantity
// `Comparable_` for ordering = a Measure (-> Quantity) OR an ordered attribute (-> Ordinal).
```
`Predicate`/`Measure` are recursive; they live in `gv-core` and persist as JSON fine, but they
**cannot cross uniffi as enums** — hence the builder Object in §10.

### 6.3 Grouping, aggregation, statistics
```rust
enum KeySpec {
    Activity, Category,                              // Category is MULTI-VALUED (D4)
    AncestorWhere(Box<Predicate>),                   // session-style grouping (D1)
    Time(CalendarSpec),                              // calendar bucket (D3)
    AttrValue { attr: Uuid },                        // exact value of an attribute
    Band { attr: Uuid, thresholds: Vec<f64>, labels: Vec<String> },  // HR zones, RPE bands, grade tiers
}
enum AggSpec { Count, Sum(Measure), Min(Comparable_), Max(Comparable_), Mean(Measure),
               Median(Measure), Percentile(Measure, f64), StdDev(Measure),
               Histogram { measure: Measure, bins: Binning } }
```
The **"statistics interface"** is exactly this aggregate family (`Median/Percentile/StdDev/
Histogram`, and `Mean/StdDev` together give Foster monotony). Users get it through a `Distribution`
kind, not by hand-assembling it.

**Rows with no grouping key are excluded** (anonymous entry under `Category`; no matching ancestor
under `AncestorWhere`; no effective instant under `Time`). The deliberate inverse — materializing
absent groups — is the spine (§7).

### 6.4 Extremum (row selection ≠ aggregation)
Argmax-with-carry is a *selection*, not a reduction: it keeps the winning row to project its fields.
```rust
struct ExtremumSpec { by: Comparable_, dir: MinMax, k: u32,
                      project: Vec<FieldRef>, tiebreak: Tiebreak }
enum FieldRef { Value(Measure), Ordered { attr, src, range }, Raw { attr, src }, // categorical
                Instant, Activity, EntryId }         // structural fields, not attributes
enum Tiebreak { EarliestInstant, LatestInstant }     // then entry-id, always — determinism
```

### 6.5 Windows (two sub-shapes; order-dependent)
```rust
enum WindowSpec {
    FrameAggregate { of: Measure, frame: Frame, agg: WinAgg, out: String },  // moving sum/mean
    Scan(ScanOp),                                                            // stateful
}
enum Frame   { Trailing(Duration), Centered(Duration), TrailingRows(u32) }   // TIME-ranged is primary
enum WinAgg  { Sum, Mean, Min, Max, Count }
enum ScanOp  { Delta(Measure), Streak(Predicate), TimeSinceLast(Predicate), IsRecordMax(Measure) }
```
Time-ranged frames are primary because row-count frames are *wrong* on a sparse series; they are
correct only once a spine has densified the axis (see ACWR, §11.3).

### 6.6 Derived measures
Named, reusable arithmetic (`Tonnage = reps·weight`, `Epley1RM = weight·(1+reps/30)`,
`Pace = distance/duration`, `Load = RPE·duration`) are their own small saved type referenced by a
`Measure::Attr`-like handle. Reps being an ordinary attribute (D2) is what lets these compose.

---

## 7. The calendar spine (why / what / how)

**Why.** A locked rule (D-foundation) says *no grouping key ⇒ excluded*. That is right for normal
analysis — don't invent phantom rows — but it means an aggregation only emits rows for periods that
*have* data. For **absence analysis** that is fatal: rest days, training gaps, and the longest
layoff are exactly the empty periods, which vanish.

**What.** A spine is a **generated, dense axis of every period in a range** (every day/week/month,
data or not), produced by `SourceSpec::Spine`. You **outer-join** your aggregation onto it so empty
periods appear (as null, then filled to 0) instead of disappearing.

**How.** It is *not* a new subsystem — it is the spine source plus the bounded join (§6.1, D12):
```
Source(Spine{range, Day})                              // dense daily axis is the preserved (left) stream
 → Join{ other: <your daily aggregation>, on: Time(Day), how: LeftOuter }
 → Fill{ count, default: 0 }                            // empty days become 0
 → ...                                                  // then read absences / scan for runs
```
- **Rest-day count** = rows where the measure is 0.
- **Longest layoff / gaps** = the longest run of consecutive 0-rows (a `Scan(Streak)` over the dense
  series — and a streak operator literally cannot see periods that don't exist, which is *why* the
  spine is a prerequisite, not just a convenience).
- **Calendar-windowed metrics** (moving averages, ACWR) require it too, so trailing time frames
  include zero days.

It **generalizes past time** (a "category spine" makes an untrained muscle group show volume 0), and
"missed planned workouts" splits cleanly: an entry that exists but is planned-not-done is the
plan/actual predicate (D8, no spine); "I planned Tuesday and logged nothing" is a spine case.

---

## 8. Curated kinds — the UX layer

**Principle: curate the UX, never expose the algebra.** The user picks a viz/analytic *objective*
and fills domain slots (activities, attributes, categories, periods) in domain language. The
objective anchors **both ends** — it constrains the input form *and* fixes the output schema.

A kind is therefore a **three-way contract**: `input form ↔ pipeline template ↔ result schema (and
its viz)`. The algebra is purely interior; there is no general pipeline-builder UI to make
approachable. The algebra is **grown kind-driven** — operators are implemented as the kinds that
need them ship, not speculatively.

| Kind (objective) | Fills | Expands to (sketch) | Result schema → viz |
|---|---|---|---|
| **Aggregate** | selection, measure, agg, group-by[] | `Filter → [Derive] → Group → Aggregate` | dims + measure → bar/table/value |
| **Frequency** | selection, calendar, fill-empty? | `Group(Time) → Count` (+ spine if fill) | time + count → line/bar |
| **Distribution / Statistics** | selection, measure, stat | `… → Aggregate(Histogram/Percentile/Median/StdDev)` | bins/quantiles → histogram/box |
| **Record / Extremum** | selection, measure, dir, carry[], group-by? | `… → [Group] → Extremum{project}` | value + carried fields → stat card / line |
| **Sequential** | selection, measure, calendar, op | `… → Group(Time) → [spine+Fill] → Window` | time + metric → line |
| **Correlation** | x-series, y-series, on-key | `Join{ left: X, right: Y, on }` | key + x + y → scatter |
| **Goal** | a query (ref/inline) + hoped-for result | run query; compare to target | progress/achievement → gauge |

Adding a genuinely new objective is a new kind (a code change) — an accepted, bounded cost, since
the kinds map 1:1 to the visualizations we will build anyway.

---

## 9. Execution & the two axes

Two independent axes, kept distinct (their conflation caused confusion early):

- **Axis X — data source:** load `[Entry]`/`[Value]` from the DB, or synthesize in tests.
- **Axis Y — execution location:** compute **in-memory** (Rust) vs **in-DB** (push to SQL).

This design commits **Axis Y = in-memory** (D10): one interpreter in `gv-core`; `RunQuery`'s executor
**loads** the slice the pipeline needs (with coarse `Source`/`Filter` **pushdown** to bound memory),
builds a `Forest`, and runs the engine. Because the engine only ever sees in-memory domain objects,
DB-backed data and synthesized test data run identical code (Axis X is free).

**In-DB execution is deferred**, not foreclosed: the pipeline IR is a clean compilation target
(operators → relational algebra → SQL). The concrete trigger to build it is **server-side scale** —
`gv-server` running analyses across many users / large histories, where pushing compute into
Postgres beats per-user in-memory loads. Until then, in-memory wins on simplicity and on the
windowed family (streaks/PR/time-since/ACWR are clean folds in Rust and poorly/unevenly supported in
SQLite).

---

## 10. Boundaries (crate placement)

- **`gv-core`** — all declarations (`SavedQuery`, `Pipeline`/`Step`/`Predicate`/`Measure`, the kinds),
  the foundation (§5), and the **in-memory interpreter**. Pure: no `sqlx`, no `uniffi`. Deterministic
  and `Arbitrary`-generatable, so the whole engine is property/simulation-testable.
- **`gv-sql`** — a `SavedQuery` `*Row` storing the declaration as one external-tagged JSON column
  (mirroring `AttributeConfig`); `DeltaExecutor` impls for both backends; the **loader** + the
  `RunQuery`/`RunSavedQuery` `QueryExecutor` (load → build forest → call engine).
- **`gv-ffi`** — exposes `SavedQuery` and `ResultTable` (flat → cross directly). Kinds, being flat
  records/enums, cross directly. The **recursive** `Predicate`/`Measure` cannot cross as enums, so
  construction goes through an **opaque `PipelineBuilder` uniffi Object** that holds the AST
  Rust-side and serializes to the JSON declaration on `build()`. `gv-ffi` depends on
  `gv-core`/`gv-client`, not `gv-sql`; since the declaration is plain `gv-core` data, this is fine.
- **`gv-client`** — wires `RunQuery` into the `QueryStore` change-broadcast for on-demand recompute.

Serde: external tagging throughout (the `arbitrary_precision` gotcha), as for `AttributeConfig`.

---

## 11. Stress tests (re-runnable)

Stated as use case + what's structurally hard + how this model fits + what it exercises — so the
same bar can be applied to another design.

### 11.1 PR-with-carry
**Use case.** "My all-time max bench load, and the date (and route) I hit it." Also per-period
("max load each month") and "hardest send this season + which route."

**Hard because.** It is *argmax*, not a reduction: you need the extremal value **plus context from
the winning row** (a date — which is not an attribute — and possibly an ordinal grade).

**How this fits.** A `Record/Extremum` kind → `Filter → [Group(Time)] → Extremum{ by, dir:Max,
project:[value, Instant, route], tiebreak: EarliestInstant }`. Carry is a heterogeneous `FieldRef`
(attributes, the `Instant`, the `Activity`); the result schema carries one typed column per field.
"Hardest send" uses an **ordinal** `by` (grade). The separate "is this a new PR" question is a
different operator — `Scan(IsRecordMax)` then filter.

**Exercises.** Row-selection vs reduction; heterogeneous projection incl. structural fields;
`Cell::Instant`; deterministic tiebreak; ordinal comparables; the kind↔schema contract for single vs
grouped output.

### 11.2 Scatter-join (cross-source correlation)
**Use case.** "Bodyweight vs estimated-1RM over the same period" (scatter). Also sleep vs
performance, fingerboard tonnage vs max send, session tonnage vs session RPE.

**Hard because.** Two carriers from **disjoint** entry sets, logged at different times/frequencies,
must be **aligned on a shared key** before they can be paired.

**How this fits.** A `Correlation` kind → two linear series pipelines (each `Filter → [Derive] →
Group(on) → Aggregate(agg)`, with possibly different aggs — mean weight vs max 1RM) combined by one
binary `Join` on a shared `on` key (default `Inner` — a scatter point needs both sides). The key is
owned by the combinator and forced identical on both sides. `on` is **any `GroupKey`**, so the same
mechanism does per-session and per-category correlation, not just per-time.

**Exercises.** The bounded binary join; combinator-owned alignment keys; inline-or-referenced series;
bivariate result schema (key + two measures) with axis roles in the kind↔viz contract; per-series
aggregation. (Lagged alignment — sleep night *t* → performance day *t+1* — is a flagged, deferred
knob: an offset on the join key.)

### 11.3 ACWR (rolling workload ratio)
**Use case.** Acute:Chronic Workload Ratio = trailing-7-day load ÷ trailing-28-day load, plotted
over time (with a "sweet spot" band).

**Hard because.** It needs a **dense daily series** (a rest day is a 0 inside the window, not a
skipped row), **two overlapping windows of different widths**, and a **ratio** of them — and it is
exactly the family SQL handles poorly.

**How this fits.** A `Sequential` kind (preset) → `Source(Spine{Day}) → Join{ daily-load-agg,
LeftOuter } → Fill(0) → Sort → Window(Trailing 7d, Sum) → Window(Trailing 28d, Sum) → Derive(ratio)`.
A "rolling ratio" is **not** a primitive — it decomposes to two frame-aggregate windows + a ratio
`Derive`; the kind is the preset over that composition. The ratio is a labeled-dimensionless scalar
(D11). In Rust it's sliding sums (O(n)).

**Exercises.** The spine as a **correctness prerequisite** (not just absence analysis); `Fill`;
time-ranged window frames; the frame-aggregate vs stateful-scan window taxonomy; join as a
**mid-pipeline step** (this case is what established that joins compose mid-pipeline rather than
being terminal); the curated-kind → composable-pipeline layering at its clearest; and the strongest
case for in-memory-first.

---

## 12. Deferrals & scoping (justified)

- **In-DB (SQL) execution** — trigger: server-side scale. The IR is the compilation target, so this
  is additive. (D10)
- **Activity is-a DAG table + its mutations/sync/UI** — analysis depends only on the
  `CategoryResolver` seam; trivially `{activity}` today, grows without touching analysis. (D4)
- **Time-gap–binned sessions** — explicit forest sessions cover the use cases; gap-binning would be
  one stateful operator added only if a real flat-log case can't be served. (D1)
- **Incremental view maintenance (IVM)** — full recompute is cheap at single-user scale; IVM stays a
  per-query optimization for later. (D7)
- **Full dimensional analysis** — leaf unit normalization gives correct aggregation; arithmetic uses
  labeled scalars. (D11)
- **N-way / joins-of-joins / lagged joins** — binary same-key joins cover the cases. (D12)
- **Regression/forecasting math** — modeled as a `Sequential`/aggregate slot; the math is deferred.
- **Travel/DST tz history & per-entry offsets** — one captured tz per query for now. (D3)

---

## 13. Open questions / risks

- **Builder-Object UX surface** — the recursive `Predicate`/`Measure` builder is the one piece of FFI
  ceremony; its method surface needs design (kept entirely behind the curated kinds, so it is not
  user-facing, but the kinds' forms drive it).
- **Algebra build cost** — real, and worst for trivial scope (filter+count alone). Mitigated by
  growing it kind-driven and by the shared foundation carrying the hard logic once.
- **Memory/pushdown** — `Source`/`Filter` pushdown to the loader must bound how much a pipeline pulls
  into memory; needs concrete pushdown rules.
- **Kind catalog coverage** — the long tail beyond the catalog requires new kinds; the catalog above
  is the starting set, validated against §11 but not exhaustive.
