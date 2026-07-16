# Analysis glossary

> **The GV system terms** — one entry per term: definition + references. Relationships to
> Malloy and friends appear as inline "cf." notes; external terms are never top-level
> entries. Entries marked *(open)* have an unsettled fork recorded inline.

#### Source

Anything a query stage reads from: the BaseSource, or the output of the previous segment
(a *derived* source at a new grain). GV has no general FROM — every pipeline is anchored
at the single BaseSource, and derived sources arise only by position in the pipeline.
Composition via saved-spec-as-source (`PlanRef`) is a foreseen extension, not v1.
*(cf. Malloy `source` / query-as-source.)*

#### BaseSource

The single root every analysis query starts from; there is exactly one. Two faces along
the schema/instance duality: the schema face is the **Catalog** (which fields exist —
needs no rows); the instance face is entry-grain rows produced from the user's data at
execution time. Scoping defaults are baked in, not parameters: owner-scoped, excludes
templates, completed entries only. See [base-source.md](./base-source.md).

##### EntryForest

The entry/forest model's contribution to the BaseSource — and deliberately the ambiguous
constituent, contributing to *both* faces: the entry **model** (Temporal, activity,
position — constant across users) yields the **built-in fields** on the schema face, and
the user's actual **entries** are the rows on the instance face. Pinned usage: **Forest**
(the core type) always means the instance side.

##### StdLib

System-defined catalog content: activities and attributes (later, metric definitions)
that ship with GV. Loaded exactly like user content and yielding fields by the same
rules — only the **provenance** tag differs.

##### UserLib

The user-created catalog content: their activities and attributes (later, their metric
definitions). Same mechanism as StdLib, `user` provenance.

#### Catalog

The BaseSource's schema: the merged namespace of every field a query may reference —
built-in fields ∪ attribute-derived fields ∪ named metrics. Derived fresh from live
domain data and never stored, which is what lets resolution double as the staleness gate.
*(Refs: [analysis-architecture-query-schema.md](../analysis-architecture-query-schema.md).)*

#### Field

A named, typed, per-row quantity of a source: built-in (`instant`, `duration`,
`activity`) or attribute-derived; after a reduce, the derived source's fields are the
prior segment's output columns. Persisted references carry identity only (IDs); type and
capability are resolved from the Catalog at use time, never stored. At query time a field
plays one of two roles: group key (the dimension role, possibly through a transform) or
metric input. *(cf. Malloy `dimension` — the per-row scalar half.)*

#### Provenance

Where a definition came from: `built-in | stdlib | user`. A tag the UX uses for picker
grouping and labeling; the engine treats all provenances identically.

#### Metric

A named aggregation expression: an aggregation function + a field (+ an optional
filter), or a ratio of two such. `count` is the one field-less metric. Metrics are the
unit of user-facing analytical vocabulary ("Total Reps", "Send Rate") — the picker offers
metrics, not aggregation functions. v1 metrics are system-derived + stdlib; user-defined
metrics arrive later via a form. *(cf. Malloy `measure`, MetricFlow `metric`.)*

#### Measure *(open)*

Currently unassigned — the fork is whether GV uses this word at all. "Measure" is the
documented three-way collision (A1: the aggregation · A2: a per-row numeric expression ·
OLAP: the cell value; [analysis-comparison.md §2](../analysis-comparison.md)). Option A
*(lean)*: GV never says measure; **Metric** covers the aggregation-expression concept and
the collision dies. Option B: adopt MetricFlow's two-tier split — *measure* = a bare
aggregation over a field, *metric* = the named, filterable, ratio-capable layer over
measures. B buys a principled home for ratios at the cost of two near-synonyms in the UI
and docs.

#### Dimension

Not a stored kind — the **role** a field plays when used as a group key: `row → key`,
optionally through a transform (time truncation, `bin(width)`, `band(thresholds,
labels)`). Discreteness comes from the resolved type or the transform. *(cf.
Looker/Malloy make dimension an intrinsic field kind; GV keeps the intrinsic split at
Field-vs-Metric — per-row vs aggregate — and lets any field group or feed a metric as its
type allows.)*

#### Relation (Carrier)

The value flowing between segments: `{ schema, rows }`. The **segment** is the unit of
closure — grouping's nested state never escapes a segment; its output is one flat row per
group. Strictly flat/tidy: no relation-valued cells (`nest` is dropped in favor of tidy
output + a top-per-group operation — README open decision #2). *(Refs:
[analysis-query-model-notes.md §4](../analysis-query-model-notes.md).)*

#### Segment

One reduce stage of a pipeline: filters, group-bys (field + transform), metrics, having,
order, limit, calculate. The pipeline is `Vec<Segment>` even at length one; re-graining
is just the next segment. *(cf. Malloy pipeline `->`.)*

#### Grain

What one row of the working relation means: entry-grain at the BaseSource; each reduce
re-grains to its group keys. After the first reduce the attribute vocabulary is gone —
later segments see only the prior segment's output columns.

#### QuerySpec

The persisted, synced declaration of one analysis: a pipeline of segments over the
implicit BaseSource. The only stored artifact — the result schema and the execution are
derived from it against the live Catalog (resolve = validate = derive schema), and it
carries a version field from day one.

#### ResultTable

The self-describing output: typed columns carrying measurement type
(nominal/ordinal/quantitative/temporal), role, sort order, and the full category domain
(so empty buckets render), plus rows of typed cells. Always tidy/long. *(Refs: POC
`viz/specs.ts` boundary findings;
[analysis-malloy-evaluation.md §4](../analysis-malloy-evaluation.md).)*

#### ChartSpec

The renderer-agnostic presentation binding: chart kind, column→channel assignment, and
the genuinely presentation-only options (step marks, zero-baseline, band thresholds).
A separate concern from the QuerySpec (README open decision #4: separate artifact, saved
alongside).

#### GoalSpec

A goal = a query + a hoped-for result; achievement and progress are derived by comparing
the query's current result to the target, never stored. The target lives in the GoalSpec,
not in result rows — a POC boundary finding. *(Refs: A2 D5,
[analysis-design-2.md §3](../analysis-design-2.md).)*
