# BaseSource & the field taxonomy

> **Status: draft for discussion (2026-07-06).** First component doc; terms pinned in the
> [glossary](./glossary.md). Supersedes the Source sections of
> [analysis-design-1 §3](../analysis-design-1.md) where they differ.

## 1. One root, two faces

The working formula — `BaseSource = EntryForest + StdLib + UserLib` — holds, recast along
the schema/instance duality so each constituent lands on the face it actually feeds:

| | Schema face — the **Catalog** | Instance face — the rows |
|---|---|---|
| What it is | every field a query may reference | entry-grain rows |
| Built from | built-in fields (entry model) ∪ attribute-derived fields (StdLib + UserLib attributes) ∪ named metrics | the user's Forest + Values, flattened by the **SourceAdapter** (demand-driven attribute pivot, canonical-instant derivation) |
| Needed for | authoring, validation, schema derivation — no rows required | execution only |

EntryForest is the one constituent contributing to *both* faces: the entry **model**
yields the built-in fields; the user's **entries** are the rows. StdLib and UserLib are
catalog-side only — their instances (entries logged against stdlib activities) are just
entries in the forest like any other.

**There is exactly one BaseSource.** Consequences:

- No general FROM. Saved-spec-as-source (`PlanRef`) remains a foreseen extension.
- The spine is **not** a source: absence/densification happens at execution, surfaced as
  "show empty periods" on a time dimension.
- Mid-pipeline "sources" are just the previous segment's output — a derived grain, not a
  new root.

## 2. Defaults, not parameters

Baked into the BaseSource (repeating them per query would be noise; changing them is a
design change, not a query option):

- **owner-scoped** — like every query in the system;
- **`is_template = false`** — template entries never appear in analysis;
- **`is_complete = true`** — planned-vs-actual analysis arrives later via the value-level
  **aspect** selector (plan/actual), not by flipping this default.

Activity scoping is deliberately *not* a source parameter — it changes neither grain nor
vocabulary; "climbing only" is a filter.

## 3. Built-in fields — the EntryForest schema contribution (v0 proposal)

| Field | Type | Dimension role | Metric input | Transforms | Notes |
|---|---|---|---|---|---|
| `instant` | temporal | **only via truncation** (raw-timestamp grouping is useless at entry grain) | — (context column for extremes) | truncate: day / week / month / year; tz + week start captured in the spec | Total: the entry's own temporal, else derived from the nearest ancestor (sequences carry time downward) — so every entry lands in a time bucket. Exact derivation is **Q1**. |
| `duration` | quantitative (duration) | later, via bin | sum / avg / min / max | bin/band later | Absent duration ⇒ excluded from duration metrics. Own-vs-subtree semantics is **Q2**. |
| `activity` | nominal | identity | — | — | Domain = catalog activities, both provenances. Domain display order is **Q3**. |
| `session` *(gated)* | nominal key | identity | — | — | Nearest-ancestor-matching-predicate. Gated on pinning what a session is (structural sequence vs windowed); both reduce to "assign each entry a session key." |

Built-in metric: **`count`** — the one field-less metric ("number of entries"; reps are an
ordinary attribute, so "total reps" is `sum(Reps)`, never a count).

**Deliberately omitted, additive later:** forest-structural fields (parent, is-root,
depth), `is_sequence`, `category` (waits on the activity-category closure/DAG),
time-of-day / day-of-week transforms.

That this list is so small is the point: nearly all analytical vocabulary is
attribute-derived (§4), which is why the Catalog can be machine-derived rather than
authored — the "system is the modeler" move.

## 4. Attribute-derived fields

Each catalog attribute yields **exactly one field**, named by the attribute, typed by its
kind. Every read of a value-derived field resolves in two stages before any use:

1. **aspect** — Actual (default) | Plan;
2. **range fold** — Mean (default) | Min | Max (Omit variant is **Q6**) — collapses a
   Range value to a scalar. The within-value counterpart of a metric's across-rows
   aggregation: same names, two levels.

Both are per-use selectors on the field reference, surfaced only in advanced UI.

| Attribute kind | Field type | Dimension role | Metric aggregations | Notes |
|---|---|---|---|---|
| Numeric | quantitative | via `bin(width)` / `band(thresholds, labels)` | sum / avg / min / max (median / percentile / stddev later) | |
| Mass | quantitative, unit-normalized (kg) | bin / band | same | normalize at the leaf; display unit is presentation-only |
| Length | quantitative, unit-normalized (m) | bin / band | same | mechanical clone of Mass |
| Select (ordered) | ordinal — rank = option index | identity; domain = options in config order | min / max (ordinal — the extremum carries its label) | comparison legality is read from config at resolve time, never stored on the ref |
| Select (unordered) | nominal | identity; domain = options | — | |
| Multiselect | nominal, **multi-valued** | identity, fans out — an entry lands in every selected group | — | per-group totals exceed the grand total by design; allocation policy gets named alongside the category DAG |
| Text | nominal, open domain | identity — high cardinality, possibly filter-first (**Q4**) | — | the autocomplete query already exists for filter values |

**Absence rule:** a missing value (or an Omit fold) excludes the row from that metric and
assigns it to no group for that dimension — phantom rows are never invented. The
deliberate inverse is the spine ("show empty periods"), which densifies the *result*, not
the source.

## 5. Named metrics (pointer)

Stdlib metrics (Tonnage, e1RM, Pace, Send Rate, …) and later user-defined metrics sit in
the Catalog as named, provenance-tagged definitions over fields — same namespace, same
resolve pass. Timing is README open decision #1; the activation question (**Q5**) and the
definition form go to `metrics.md`.

## 6. Open questions

- **Q1 — canonical instant.** Pin the derivation (own `infer_start().or(end())`, else the
  nearest ancestor's?) and the field's public name (`instant` vs `effective instant`).
- **Q2 — duration on sequences.** Own temporal only, or roll up from children when absent?
- **Q3 — activity domain order.** Creation order, alphabetical, or manual — charts need a
  deterministic answer.
- **Q4 — Text grouping.** Group-by allowed by default, or filter-only with opt-in grouping?
- **Q5 — stdlib metric activation.** Bind to fixed stdlib attribute IDs, or ship as
  templates with field slots ("make Tonnage from [reps] × [weight]")?
- **Q6 — range fold variants.** Keep A2's `Omit`, or just Min/Max/Mean?
