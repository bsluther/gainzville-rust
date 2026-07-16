# ChartSpec

> **Status: draft for discussion (2026-07-07).** Terms in the [glossary](./glossary.md);
> evidence base is the POC's viz layer (`malloy-poc/viz/specs.ts` boundary findings).

## 1. Role — and the dual of resolve

ChartSpec is the persisted, declarative presentation binding for one saved analysis: it
maps a QuerySpec's output columns onto the slots of one chart kind. Persisted **together**
with the QuerySpec (`SavedAnalysis { query, chart }`) but as separately-versioned fields —
semantics and presentation evolve independently.

Its validation is the viz-side twin of the query-side resolve:

| | persisted | binds against | derived (never stored) |
|---|---|---|---|
| query side | QuerySpec | the Catalog | result schema, execution |
| chart side | ChartSpec | the query's **result schema** | the concrete render: domains, ticks, reshaped series |

`validate(ChartSpec, result_schema)` is a static pass, no rows: every binding must name an
existing output column whose measurement type is legal for its slot. Staleness propagates
the same way — editing the query can orphan a chart binding, and the same
build/load/edit gate catches it. Bindings reference output columns by **stable id** (the
id of the dimension/measure use that produced the column), not display name, so renames
don't dangle charts.

## 2. Shape — closed kinds over a shared vocabulary

A closed enum of chart kinds, one typed record each, composed from a small shared
vocabulary (`ColumnRef`, `AxisOpts`, series bindings). The grammar-of-graphics
inspiration lives in that shared vocabulary — channels, measurement-type requirements —
**not** in a universal mark/encoding spec: ~8 kinds don't justify a grammar compiler, and
the non-axis terminals fit bespoke records better than encoding channels (POC: those were
the easiest components).

```
ChartSpec = Bar | Line | Scatter | BigValue | Progress | Calendar | RecordCard | Table

BarSpec  { x: ColumnRef, y: ColumnRef, series: Option<ColumnRef>, stacked: bool, y_axis: AxisOpts }
LineSpec { x: ColumnRef, series: Vec<{ y: ColumnRef, step: bool }>, y_axis: AxisOpts }
AxisOpts { zero: Option<bool>, label_override: Option<String>, format_override: Option<Format> }
```

Each kind declares **slot requirements** — e.g. `Bar: x discrete
(nominal/ordinal/time-bucket), y quantitative, series discrete & low-cardinality`. Like
the catalog's capability descriptors, they serve three consumers: validation, the
builder's "legal kinds for this result shape" suggestions, renderer dispatch.

## 3. What it stores — only the viz-only residue

The sharpest POC boundary finding: most of what a chart needs (category domains in config
order, sort order, units, time grain, measurement types, default formats) already exists
in the catalog or the query. The ChartSpec **references** the self-describing result
schema and never re-declares it. It stores only: kind, slot bindings, per-series mark
options (step), zero-baseline / format / label overrides, band thresholds.

`default_chart(result_schema) → ChartSpec` derives a complete spec from a result shape
(time dimension → x, measure → y, second dimension → series; running-max → step line;
ratio → percent format). One function, three jobs: Tier-0 auto-charts, the builder's
chart suggestion, and spec minimalism — a saved spec is the user's overrides over
derivable defaults.

Boundary rule: **no chart step inside the query pipeline** — rendering maps *from* the
final table; it is never an operator *in* it.

## 4. Open questions

- **Chart-component API** — do components consume the ChartSpec directly, or a
  fully-resolved per-kind model produced by a translation layer? (Next discussion.)
- Exact v1 kind list (candidate first slice: bar, line, big value, table).
- ColumnRef identity mechanics — stable output-column ids in the QuerySpec.
- ~~Where tooltip/selection payloads live~~ — resolved by the Line scrub spike
  (chart-model.md, hardening round): the ChartModel already carries everything a
  tooltip needs; no payload field exists in the contract.
