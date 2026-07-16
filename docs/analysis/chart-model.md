# ChartModel

> **Status: co-design in progress (2026-07-07)** — recording decisions as they settle.
> The ChartModel is the fully-resolved, per-kind input a chart component consumes:
> `render_model(ResultTable, result_schema, ChartSpec) → ChartModel`, computed in
> `gv-core`, crossing FFI as flat records. Components are decoupled from
> QuerySpec/ChartSpec/ResultTable entirely — which is what lets Swift prototyping start
> before the QuerySpec is finished.

## Conventions

- **Totality.** A ChartModel is always completely renderable — no missing slots, no
  partial states. Partiality is resolved upstream by spec completion (defaults merged
  against the result schema); a slot that can't be defaulted makes the kind illegal for
  that result shape, so the builder never offers it. An empty result yields a model with
  empty series — the component renders an empty state, never an error.
- **Instants are epoch-ms `i64`** (the existing gv-ffi DateTime convention).
- **Discrete axes are dense, domain-aligned arrays** (D2): the model carries the full
  category domain (ordered, including empty categories) and each series' values as
  `Vec<Option<f64>>` aligned by index. Kills the string-matching wire-bug class the POC
  hit (Date-vs-ISO silently collapsed a chart); makes empty buckets and stacking
  structural rather than conventional.
- **Time axes are sparse, real timestamps**: points carry actual instants so gaps stay
  visible (the POC bodyweight lesson). If the user chose "show empty periods," the query
  already densified — the points then include the zeros.
- **Colors are palette slots** (D3): series carry `color_slot`, resolved to concrete
  colors by the Swift theme (design tokens, dark mode). The model never contains colors.
- **Numbers are raw `f64` + a format descriptor** (D1): Swift maps descriptors to
  Foundation formatters (locale-correct), core stays authoritative on semantics and
  precision.

## Shared vocabulary

```rust
Format = Count                                        // integer count of things
       | Number   { precision: u8 }
       | Percent  { precision: u8 }
       | Quantity { unit: String, precision: u8 }     // "kg", "km", "days"
       | Date     { grain: Day | Week | Month | Year }

CategoryDomain = Vec<CategorySlot>   // full domain, display order, incl. empty categories
CategorySlot   = { label: String }   // + stable id later (selection/drill)

SeriesMeta = { label: String, color_slot: u8 }
```

Units: **the ChartSpec picks the display unit** (defaulted from the attribute config's
`default_unit`, user-overridable); core converts values into that unit at `render_model`
time, so `Quantity.unit` is always the chosen unit and conversion/rounding happens in
exactly one place.

## Decisions log

| # | Decision | Status |
|---|---|---|
| D1 | Format descriptors in the model; Swift renders them. Display unit is a ChartSpec choice; core converts. | **accepted** |
| D2 | Dense domain-aligned arrays for discrete axes; sparse real timestamps for time axes | **accepted** |
| D3 | Color slots; the theme resolves actual colors | **accepted** |
| D4 | Title is a ChartSpec field (user-chosen, defaulted to the auto-generated sentence); travels resolved in the model | **accepted** |
| D5 | One Bar kind: single-series is the degenerate case; stacked/grouped is a parameter (prior art unanimous: Vega-Lite, ggplot2, Swift Charts, Highcharts) | **accepted** |

## Prototype findings (2026-07-07)

All five v1 kinds are prototyped as SwiftUI components against hand-written fixtures
(`swift-app/Gainzville/Features/Analysis/`, gallery in the new Analysis tab; two
fixtures per kind: canonical + stretch), verified by screenshot on an iOS 26 sim.

**Verified.** Swift Charts maps the models ~1:1, validating D2/D5 in real rendering:
`chartXScale(domain:)` renders empty categories as empty slots; multi-series BarMarks
stack automatically and group via `.position(by:)`; `.interpolationMethod(.stepEnd)`
gives the PR step line; `includesZero: false` handles the bodyweight baseline; SectorMark
covers pie/donut with `angularInset` as the surface gap; format descriptors map cleanly
to Foundation formatters, including unit-formatted axis ticks ("120 kg").

**Caught by the stretch fixtures.** The legend must wrap (a capped pie = 7 entries);
fixed with an adaptive grid. Hand-rolling the legend was the right call regardless —
the built-in one is too rigid to theme.

**Palette.** Eight categorical slots minted as `Gv<Hue><Step>` tokens
(GvAzure500 … GvOrange500), validator-passed against gvSurface #111111. The slot
*order* is the colorblind-safety mechanism (max-min adjacent ΔE) — never reorder,
never cycle; the warning lives on `Color.gvChartSeries`.

**Hardening round (2026-07-07, later):**
- ~~Empty states~~ — implemented: a third fixture per kind (complete model, zero rows)
  plus component handling. The designed state keeps the card title ("the chart still
  says what it is") over a neutral "No data" mark; Table keeps its headers ("the shape
  is still information"); BigValue shows a muted "—". Forced one contract fix:
  **`BigValueModel.value` is `Option<f64>`** — max/avg over zero rows has no honest
  substitute (a "0 kg PR" would be a lie); `count` still emits a real 0.
- ~~macOS render pass~~ — verified by hand, looks good.
- ~~Tooltip payloads~~ — **resolved by the scrub spike: no payload field needed.**
  Line now ships `chartXSelection` scrubbing (drag on iOS, hover on macOS): snap the
  continuous date to the nearest data instant, rule + bubble rendered entirely from
  what the model already carries (points, `Format`, grain, series meta). Interactive
  feel is user-verified territory; static render confirmed.
- **Still open — the uniffi seam**: components reference hand-written Swift types;
  uniffi will generate replacements. Field names should carry over (uniffi camelCases),
  but plan a typealias seam so the swap doesn't touch component bodies.
- ~~Long series / horizontal scrolling~~ (2026-07-08) — **a display concern, not a
  model field**: the window is derivable from grain + data span, so the component
  decides (per-grain windows: day → 90d, week → 26w, month → 24mo; scroll engages only
  when the span exceeds it, opening at the most recent data, Health-style). Swift
  Charts natively: `chartScrollableAxes` + `chartXVisibleDomain` + `chartScrollPosition
  (initialX:)`; selection composes with scrolling. Verified with a 3-year weekly
  fixture. A user-chosen window can later arrive as an additive ChartSpec override.
  Caveats: the y-scale stays fixed to the full domain while scrolling (correct for long
  series; no per-window rescale), and time-bucketed *bars* will want the same treatment
  (categorical `chartXVisibleDomain` takes a category count) when that case ships.
- **Axis year rule** (display convention): a bare "Jun 14" means the current year;
  other years stack beneath the tick ("Jun 14" / "2024"). The scrub bubble applies the
  same rule inline ("Jun 12, 2024"). Year grain is exempt (the label already is the year).

## v1 kinds

BigValue · Bar · Line · Pie · Table — then Scatter, Progress, Calendar, RecordCard in
later phases.

### BigValue — prototyped (the pattern-setter)

```rust
BigValueModel {
  title: Option<String>,
  label: String,          // "longest gap between training days"
  value: Option<f64>,     // None = empty result: max over zero rows has no value (renders "—")
  format: Format,         // Quantity { unit: "days", precision: 0 } → "18 days"
}
```

### Bar — prototyped

```rust
BarModel {
  title: Option<String>,
  x: BarAxis { label: Option<String>, domain: CategoryDomain },
  y: ValueAxis { label: Option<String>, format: Format },   // bars are always zero-based
  series: Vec<BarSeries>,          // exactly 1 series = a plain bar chart
  stacking: Stacked | Grouped,     // ignored when series.len() == 1
}
BarSeries { meta: SeriesMeta, values: Vec<Option<f64>> }    // aligned to x.domain
```

Notes (D5): one component either way — Swift Charts stacks automatically via
`.foregroundStyle(by:)` and groups via `.position(by:)`, so single-vs-multi is three
conditionals (hide legend at one series, ignore `stacking`, simpler tooltip), not two
components. `None` vs `Some(0.0)` distinguishes *no data* from *measured zero* — both
render as an empty/zero bar, but tooltips and accessibility differ.

### Line — prototyped

```rust
LineModel {
  title: Option<String>,
  x: TimeAxis { grain: Day | Week | Month | Year },   // v1: time-only x
  y: ValueAxis { label: Option<String>, format: Format, zero: bool },  // zero resolved upstream
  series: Vec<LineSeries>,
}
LineSeries { meta: SeriesMeta, step: bool, points: Vec<(i64, f64)> }  // sparse, epoch-ms
```

Notes:
- **Both series origins produce the same model**: "one measure split by a dimension"
  (tonnage per month *by lift*) and "several measures as lines" (volume + max grade) are
  indistinguishable by model time — just labeled series. The distinction dies at the
  spec/`render_model` layer (pivot by dimension values vs one series per measure).
- **Consequence — the shared-axis constraint**: one y-axis means all series must share a
  compatible `Format` (same unit family). This is a slot rule at the ChartSpec layer:
  multi-measure lines are legal only when the measures' formats agree. **No dual y-axis
  in v1** (Swift Charts doesn't support it; the POC flagged dual-axis as dubious anyway) —
  the escape is two charts or a normalized index, later.
- `zero` arrives resolved (`bool`, not `Option`): the default is derived upstream
  (counts/volumes → true; point-in-scale values like bodyweight → false), spec can
  override — the model carries only the outcome (totality convention).
- Lines connect across time gaps; the time scale makes the gap visible spatially. If the
  user chose "show empty periods," the query densified and zeros plot as data.

### Pie — prototyped

```rust
PieModel {
  title: Option<String>,
  slices: Vec<PieSlice>,                 // value-descending; ties broken by domain order
  other: Option<OtherSlice>,             // remainder bundle when the cap binds
  format: Format,
  donut: bool,
}
PieSlice   { label: String, value: f64, color_slot: u8 }
OtherSlice { value: f64, bundled_count: u32 }   // renders last, theme-neutral color
```

Slot requirements: parts-of-whole ⇒ a **single-valued** discrete dimension (multi-valued
dimensions — multiselect, future categories — are illegal: fan-out sums past the whole)
+ a **non-negative additive** measure (sum/count; never avg). Additivity is also what
makes the Other bundle meaningful — it's a legitimate sum of the remainder.

Notes:
- **The slice cap is presentation, applied in `render_model`** — never in the query. The
  ResultTable keeps all groups (switching pie → bar must show every category); ChartSpec
  carries `max_slices` (default 15, user-configurable lower, e.g. top-5), and the model
  bundles the remainder into `other`.
- Order: value-descending with a deterministic tiebreak (then domain order) — the global
  row-selection determinism invariant applies at the cut boundary.
- `other` is a distinguished field, not a flagged slice: it renders last regardless of
  its value (it can exceed the smallest kept slice), takes the theme's neutral color (no
  color_slot), and its tooltip differs ("12 others").
- Zero-value and empty categories are omitted (unlike bars). Percent-of-whole labels are
  derived by the component (value / total).
- `donut` is a ChartSpec presentation flag, carried resolved.
- **Open:** the default `max_slices` (15) exceeds the 8-slot categorical palette —
  either the default caps at the palette size, or the palette grows, before pie ships
  beyond fixtures. (Series colors are never cycled.)

### Table — prototyped (deliberately minimal)

```rust
TableModel {
  title: Option<String>,
  columns: Vec<TableColumn>,     // dimensions first, then measures (result-schema order)
  rows: Vec<Vec<TableCell>>,     // query's order; the component scrolls, no cap
}
TableColumn { label: String, format: Format }
TableCell   = Number(f64) | Text(String) | Instant(i64) | Empty
```

Notes: not a primary chart kind — a grounding view ("what data am I actually charting?").
Its model is a thin projection of the ResultTable (labels + formats applied), so its slot
requirements are satisfied by **every** result shape — which makes Table the universal
fallback: always legal in the kind picker, and the safe default when `default_chart`
can't pick anything smarter. Alignment is derived by the component from the cell type
(numbers right, text left); no table-side sorting or interaction in v1.
