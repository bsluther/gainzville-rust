`# Analysis subsystem — evaluating Malloy as inspiration

> **Status: design exploration, not a settled decision.** This records how well
> [Malloy](https://github.com/malloydata/malloy)'s modeling approach fits Gainzville's
> analysis needs, what a runnable spike found, and what to carry forward.
>
> Companions: [`analysis-features.md`](./analysis-features.md) (the feature bar),
> [`analysis-comparison.md`](./analysis-comparison.md) (the A1/A2 design evidence base and
> prior-art survey), and a runnable proof-of-concept in the **sibling `malloy-poc/` repo**
> (outside this workspace) — the queries and data live there; this doc is the gist.

---

## 1. Motivating question

GV needs user-defined **visualizations** and **goals** over training data;
`analysis-features.md` is the bar any design must clear. Malloy is a promising source of
inspiration — its model of **sources / dimensions / measures / views / queries / nest**.

We do **not** want to use the Malloy library (it's JavaScript, general-purpose, and
compiles to SQL). The idea is to present a *simplified* version of Malloy's interface as
**both** (a) the user-facing way to build queries **and** (b) the internal persistent
representation. So:

1. How far does Malloy's **model** go in covering `analysis-features.md`?
2. Can the GV analysis subsystem take **heavy** inspiration from it?

**Scope.** We assume the upfront work of shaping GV's data into Malloy-friendly sources is
feasible (the EAV→relational pivot, the joins, etc.). The question is the *expressiveness
of the model*, not the plumbing — and not the library's suitability for embedding, FFI, or
persistence (those are GV concerns covered in `analysis-comparison.md`).

---

## 2. Conceptual overview — how Malloy's abstractions map to GV

**Malloy in one paragraph.** Malloy is a semantic-modeling + query layer that compiles to
SQL. A **source** wraps a table with reusable **dimensions** (per-row scalar expressions),
**measures** (aggregation expressions — the semantic-layer sense, matching A1's vocabulary),
**joins**, and **views** (named queries). A query is a chain of operators — `group_by` /
`aggregate` / `where` / `having` / `order_by` / `limit` / `calculate` (window functions) /
`nest` (subqueries at their own grain) — composed with pipelines (`->`), refinement
(`+ { … }`), and query-as-source.

### The bridge: GV's EAV-forest → a Malloy source

GV stores entities as an EAV model over an ordered forest; Malloy wants flat relational
sources. The bridge (the "assume upfront work" part) is one pivot plus a couple of joins:

- a wide **`entries`** table — one row per log entry; each attribute pivoted to its own
  SI-normalized column; the forest collapsed to scalar `instant` (the canonical instant)
  and `session_id` (the nearest ancestor carrying a Session Type); Reps as a `reps` column.
- a precomputed **activity→category closure** (the category DAG flattened) for taxonomy.
- a generated **calendar spine** for absence and time-window queries.

Once that exists, Malloy's measure/dimension/view vocabulary maps onto GV's domain-term
queries directly.

### The fit, at a glance

| Feature family (`analysis-features.md`) | Malloy construct | Fit |
|---|---|---|
| Counting / frequency / aggregation | `group_by` + `aggregate` | ✅ native |
| Nested aggregation (outcome-by-grade; avg-per-session) | `nest` / pipeline `->` | ✅ native |
| Filtered aggregates (send rate) | `count() { where: … }` | ✅ native *(A1/A2 both lacked this)* |
| Derived measures (tonnage, Epley 1RM, pace) | `dimension` + `measure` | ✅ native |
| Filter on a computed value, then re-grain | pipeline + `having` | ✅ native |
| Extremes **with context** (PR + date / route) | `order_by` + `limit` / nested `limit 1` | ✅ native |
| Sequential (deltas, PR detection, time-since) | `calculate` (`lag`, `max_cumulative`) | ✅ native |
| Taxonomy rollup (multi-valued) | `join_many` on the closure | ✅ needs closure |
| Absence (rest days, gaps) | calendar spine + join | ⚠️ needs spine |
| Moving averages / ACWR | `calculate` (`avg_moving`) | ⚠️ ROWS-only frame |
| Statistics (median, percentile) | — | ❌ not native (`stddev` is) |
| Streaks (consecutive periods) | gaps-and-islands | ⚠️ awkward, needs spine |
| Forecasting / projection | — | ❌ outside the paradigm |

### Verdict

Malloy's **query model covers nearly every *shape* of computation** the feature bar needs —
including several that GV's own A1/A2 designs struggled with (filtered aggregates,
aggregate-of-aggregate, argmax-with-context, derived measures). The things that strain it
are **not** the query algebra; they sort into four kinds:

1. **Data-prep prerequisites** (spine, closure, the pivot) — within the "assume upfront
   work" stipulation;
2. **Function-library gaps** (median/percentile) — a bespoke interpreter just adds them;
3. **Fixed conventions** (Sunday week-start, ROWS-only frames) — parameterize / replace;
4. **Two genuine paradigm gaps** — time-range windows + stateful-scan streaks, and
   forecasting.

**So: yes, take heavy inspiration.** The source / dimension / measure / view / query / nest
/ calculate / pipeline vocabulary is a strong template for both the GV user-facing builder
and the persisted declaration — *provided* we add time-range frames, a date spine,
configurable week-start, statistical aggregates, and a stateful-scan primitive, and design
the EAV→relational pivot + category closure as the upfront data-prep layer beneath it.

---

## 3. The POC spike

To pressure-test the above against a real compiler and real data, we built a runnable spike
in the sibling **`malloy-poc/`** repo — actual Malloy (the JS library, pinned `0.0.405`)
over DuckDB / parquet.

- **Data** (`data/seed.sql`) — a deterministic DuckDB generator emits GV-*flattened*
  parquet: ~790 entries across **7 activities** (4 barbell lifts, bouldering, running,
  bodyweight) over ~12 weeks, plus the category-closure and calendar-spine tables. Shaped to
  stress the hard cases: **sparse bodyweight** (real day gaps), **multi-valued taxonomy**
  (Deadlift ∈ {Legs, Pull}; Glutes/Triceps shared across lifts), varying session sizes, and
  an upward load trend.
- **Model** (`models/gv.malloy`) — the `entries` bridge source with reusable
  measures/dimensions and the category join.
- **Queries** (`models/features.malloy`) — ~20 representative queries, each annotated with
  the `analysis-features.md` claim it proves and grouped by the four expressiveness buckets;
  deliberate gap demos live in `models/gaps.malloy`.
- **Runner** (`run.mjs`) — a small Node harness that loads a model, runs named queries, and
  prints results (and, with `--sql`, the compiled SQL).

**Findings.** Every representative query compiles and returns **correct** numbers — and the
"bridge" assumption held: once the data is pivoted/flattened, Malloy's vocabulary expresses
the domain-term queries cleanly. Concretely:

- send rate falls monotonically **V0→V8 (1.00 → 0.07)**;
- per-muscle volume *lift counts* sum to **1080 against 480 actual lifting entries** — the
  multi-valued fan-out is real, with per-category totals exceeding the grand total *by
  design*;
- running-max **PR detection** tracks correctly; **rest-day** counting via the spine works
  (90 calendar days − 72 trained = **18 rest**).

We also attached Malloy's built-in **charts** (the renderer offers bar / line / big_value /
scatter / maps). Clean 1-dimension(+series)/measure queries took a chart tag directly
(stacked-bar outcome-by-grade, per-lift tonnage lines, a PR-progression step line, taxonomy
bars, bodyweight lines, a big-value KPI). Nested / argmax / scalar-shaped queries need
reshaping to chart.

**D3 phase.** A second phase (`viz/` in the POC) evaluated **D3 in a webview** as the GV
chart renderer, with two goals: try D3 on this sample of use cases, and empirically
discover **what must cross the query→viz boundary**. A small typed component library
(`viz/gv-charts.ts` — bar/stacked, line incl. step + dual-axis, scatter, big-value,
progress bar, calendar heatmap, record card) renders **all 20 queries** from per-chart
specs (`viz/specs.ts`); a jsdom harness writes SVGs + a browsable index. D3 verdict:
every result shape rendered in ~500 lines of typed components; the *non-standard* shapes
(calendar heatmap, progress bar, record card) were easier than the axis charts and are
exactly where Malloy's renderer had nothing to offer; headless rendering suggests a clean
testing story. Record/KPI results unified fine in D3, though text-heavy terminals (cards)
are also what native SwiftUI does best — notably the **spec contract is renderer-agnostic**,
so the API needn't split even if implementations do (webview for marks-heavy charts,
native for text-heavy terminals). The boundary findings are in §4.

---

## 4. Gaps & lessons learned

### Expressiveness gaps — the four buckets

1. **Native & idiomatic** — the majority (see the fit table). Counting, frequency, derived
   measures, filtered aggregates, nested + aggregate-of-aggregate, windows
   (delta / PR / time-since), taxonomy rollup, extremes-with-context, training monotony
   (`stddev` is native), goal progress.
2. **Needs a companion data-prep mechanism** — within "assume upfront work." Absence and
   recursive taxonomy need a **date spine** and a **category closure**; per-session needs the
   **forest→`session_id`** resolution. These aren't query-algebra holes — they're source prep.
3. **Outside the portable function library** — `median` / `percentile` aren't native (only
   `stddev` is). The POC's workaround drops to a raw-SQL source (DuckDB `quantile_cont`); a
   bespoke GV interpreter would just implement them as aggregates. Not a model gap.
4. **Genuine paradigm gaps:**
   - **Time-range windows.** Malloy's `avg_moving` is a **ROWS** frame (N preceding *rows*),
     not a **RANGE** frame (N preceding *days*). On a sparse series this silently averages the
     wrong time span. The POC's flagship demo (`f_bodyweight_rows_vs_range`) overlays a
     ROWS-frame "7-day" average against the correct RANGE-frame one (computed in raw SQL,
     since Malloy can't express it): **identical on dense stretches, diverging +0.29 kg right
     after a 4-day gap.** **Lesson: for sparse fitness data, time-range frames are table
     stakes** — every rolling/periodic metric (moving averages, ACWR, weekly volume) is
     subtly wrong without them, with no error to flag it.
   - **Streaks / stateful scans.** "Consecutive weeks hitting a target" has no
     stateful-scan-with-reset primitive (Malloy, like SQL, lacks one); it needs a 4-stage
     gaps-and-islands pipeline *plus* a weekly spine for correctness. Expressible but awkward
     — a place a *simplified* GV model could be **more** expressive by borrowing a KQL-style
     `scan` (≈ A2's `WindowSpec::Scan`).
   - **Forecasting / projection.** No query form — it's a model fit above the
     view/measure/dimension layer. Goal *progress* (how close) works; *projection* (when
     you'll hit it) doesn't.

### Malloy gotchas — design intel for a GV surface

The spike surfaced foot-guns worth knowing before copying the surface:

- **Week-start is hardcoded to Sunday** — it materially changes weekly buckets (it *flipped*
  a streak result vs. DuckDB's Monday default). A GV surface must make week-start
  configurable.
- **Two silent row caps.** A query with no `limit:` gets a default SQL `LIMIT ~10`, and the
  JS `.run()` applies its *own* result cap — so a chart or query silently truncates categories
  with no error (a stacked bar showed only V0–V5 until we set an explicit limit). Charting
  queries must set `limit:` explicitly.
- **Schema staleness.** Malloy caches table schemas; a saved query breaks when the underlying
  schema changes underneath it — exactly the "validate the saved declaration against the live
  catalog on load" concern from the A1/A2 evidence base, observed live.
- Smaller ones: reserved timeframe keywords (`day`/`week`/`month`) can't be field names; null
  checks are `is null` / `is not null`; no bare scalar constants in `aggregate:`; per-group
  argmax via nested `select` + `order` + `limit` hits a SQL-gen bug (use `group_by` instead);
  strict date/timestamp comparison; **one uncompilable query fails the whole model**.

### Visualization-contract findings

Both charting exercises confirmed the grammar-of-graphics boundary the prior-art survey
flagged: charts consume **tidy/long** data where the **series is its own top-level column**
(an `outcome` buried in a `nest` can't be a series), and **sort order must travel in the
data** (you can't keep an ordering helper like `grade_ord` off to the side — it either
becomes a junk series or trips a "too many dimensions" error).

The D3 phase turned this into a concrete metadata list — and the key structural insight is
that **most of it is not new information**. It already exists in the catalog or the query;
the failure mode is *throwing it away* at the result-table boundary (a bare rows+columns
table is lossy). Sorting the findings by where each item originates:

**Carried — from the source/catalog** (the query knows which attribute/unit a column came
from; the contract must forward it):
- **Full category domain, in display order** — the grade axis comes from the attribute
  config's options list (V0 rendered as an *empty slot* despite having no rows; result rows
  can never supply absent categories). Ordinal order comes from the same config — grade
  sorts by option index, not lexically.
- **Units + normalization** — mass/length columns carry their unit; SI normalization
  happened at the leaf.
- **Measurement type** (nominal / ordinal / quantitative / temporal) and default display
  names — derivable per column from the catalog.

**Carried — from the query** (facts about how the result was produced):
- **Time grain and range** — `instant.week` implies the tick format and, with the spine,
  the dense weekly domain (weeks with no rows must still get slots; two sparse-weeks charts
  silently compressed without it).
- **Bin edges** — the histogram's contiguous bin domain falls out of the binning transform.
- **Result sort order** (`order_by`) — value-sorted bars.
- **Column roles** — which columns are dimensions vs measures, and the result's *shape*
  (series / record / scalar), which selects the terminal (chart / card / big-value).
- **Derived-measure semantics** — send-rate being a ratio implies percent formatting.

**Carried — from the saved declaration, outside the rows:**
- **Goal targets** (the 500-mile goal is in the goal declaration, not the result) — goal
  parameters must travel alongside rows.
- **Band thresholds** (calendar heatmap's 0 / 1–5 / 6–10 / … steps) are user-owned domain
  knowledge. They *can* live viz-side as configuration (the POC does this), but banding is
  also a query construct (`pick` / grade tiers), and query-side banding would make the band
  domain + labels carried like any other category domain. Either way: user config, not a
  component default.

**Pure visualization configuration** (independent of the query result — the genuinely
viz-only surface turned out to be small):
- **Chart kind** (bar / line / scatter / …) and **channel assignment** — which column maps
  to x / y / series / size / color.
- **Axis assignment** for dual-axis (which series scales to the second unit's axis) — the
  *presence* of two units is carried; the layout choice is config.
- **Per-series mark options** — e.g. the running-max PR renders as a *step* line (a smart
  default is derivable from the window type, but it's a presentation choice).
- **Zero-baseline** per axis — bodyweight must not start at 0; counts should. A default is
  derivable from measure semantics (additive vs point-in-scale), the override is config.
- Format-precision overrides, titles/labels when they diverge from catalog names.
- Everything else (palette, margins, tick density, legends) stayed **encapsulated in the
  component library** — set once, never crossing the boundary.

**Boundary mechanics** (infrastructure, not metadata):
- **One canonical wire format per type** — time columns arrived as `Date` objects while
  supplied domains held ISO strings; every bar silently collapsed to x=0. Typed cells, one
  serialization.
- **No implicit row caps** — Malloy applies a default SQL `LIMIT ~10` *and* a separate
  result-fetch cap; both silently truncated charts (a stacked bar showed V0–V5 only).
  Limits must be explicit and truncation surfaced.
- **Reshaping transforms** live at the boundary: long→wide series grouping for multi-line
  charts (declare the series column; the component groups), nest→record flattening for
  argmax results.
- Chart layers **fail silently** (wrong data renders as *a* chart); the contract wants
  types, not conventions.

---

## 5. Suggested follow-up

- **Design the GV surface from Malloy's vocabulary.** Carry `source` / `dimension` /
  `measure` / `view` / `query` / `nest` / `calculate` / `pipeline` into both the user-facing
  builder and the persisted declaration — but:
  - add **time-range (RANGE) window frames** (MetricFlow / Polars semantics), not only
    row-count frames;
  - make **week-start configurable**;
  - treat a **date spine** as first-class machinery (absence + correct time-windows);
  - add **statistical aggregates** (median / percentile / …) to the interpreter;
  - add a **stateful-scan** primitive (streaks / sessionization).
- **Make the EAV→relational pivot + category closure the data-prep layer** the surface sits
  on — that's where the forest/EAV complexity is paid down once.
- **Specify the viz contract from the §4 findings**: a self-describing result table
  (typed cells; per-column measurement type / role / sort order / domain — i.e. *forward*
  the catalog/query knowledge rather than re-declare it) plus a thin, renderer-agnostic viz
  spec holding only the genuinely-viz-only choices (kind, channel/axis assignment, mark
  options, presentation overrides). Explicit limits; no silent truncation.
- **Feed the A1/A2 synthesis.** This evaluation directly informs the open decision points in
  `analysis-comparison.md` — especially the scalar-expression sublanguage (#3), filtered
  aggregates (#4), the spine (#7), windows incl. time-range frames (#8), and the viz contract
  (#6). Malloy is a strong reference precisely because it ships clean answers to several of
  them.
- *(Optional, in the POC)* add a **plan/actual** split; try the D3 components in a real
  **webview with interactivity** (tooltips/hover — where argmax context like route names
  naturally wants to live).
