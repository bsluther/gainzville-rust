You are assembling a neutral EVIDENCE BASE comparing two independently-produced designs for a new ANALYSIS subsystem in Gainzville (a Rust workspace; see CLAUDE.md). You will NOT pick a winner, recommend one, or author a synthesized/final design — that synthesis happens in a follow-on collaborative phase. Your job is to make that synthesis well-informed. Stay neutral; you have no stake in either design.

## What "analysis" is

Features that let a user learn from the data they've entered: primarily VISUALIZATIONS (charts, tables, single values like "max load across all bench presses"), and closely related, GOALS (e.g. run 5 miles/week, climb V8 by August), which reuse most or all of the same machinery.

## Scope

- In scope: the user-facing DATA MODEL (the language and constructs a user creates/views/edits — not Swift code); a persistent model for USER-DEFINED QUERIES; a persistent model for BINDING A VISUALIZATION to a query (this may be the primary entry point — "define a scatter plot" — or a layer over a query); IN-MEMORY EXECUTION of a query; and the API/data SHAPE visualizations consume (possibly via adapters).
- Out of scope: building visualization components. In-database execution (SQLite/Postgres) is a POSSIBLE FUTURE, not this phase — note where a design enables or forecloses it.

The motivating use cases are in `docs/analysis-features.md` — treat this as the NEUTRAL BAR: it says what users want, not how. Any design must support these.

## The two designs

- A1 = `docs/analysis-design.md`
- A2 = `docs/analysis-design-alt.md`

They came from two separate phases that used different framing and terminology on purpose (to cover more of the design space). The same word may mean different things across A1, A2, and the eventual final design — be rigorous about this.

## Stay neutral

- Judge on SUBSTANCE, not polish. The docs differ in length, detail, and framing; do not reward verbosity or a more complete writeup.
- Ground feasibility FIRST-HAND in the codebase (core model, persistence, FFI/uniffi, sync, the existing query/executor pattern). Do not take either design's claims about the repo on faith.
- Validate both designs against a COMMON, NEUTRAL set of use cases from `docs/analysis-features.md` PLUS 2–3 you devise. Do NOT rely on either design's own worked examples/stress tests — each may be tuned to pass its own.
- Use the rubric at `docs/analysis-rubric.md` as the per-criterion assessment basis if present; if absent, derive explicit criteria first (from the features doc + project values: composability, FFI fit, fit with existing patterns, determinism/testability, in-memory-now/DB-later, build cost, UX, extensibility).
- Report per-criterion findings, strengths, and failure modes as EVIDENCE. Do NOT produce an overall ranking, declare a winner, or propose a merged design.

## Steps

1. GROUND. Research the codebase: core domain model, persistence/data models, read/query and write paths, boundary transformations (DB + FFI), sync, relevant patterns. Read `docs/analysis-features.md`, A1, A2.
2. RECONCILE TERMINOLOGY. A table mapping each load-bearing term across A1 ↔ A2, flagging words used differently. Use it consistently afterward.
3. SIMILARITY. What do A1 and A2 share — explicitly or in substance (same concept, different name)?
4. CONTRAST. Where do they diverge — and what is the COMMITMENT behind each divergence, not just the surface difference?
5. EVALUATE. Assess both against the rubric/criteria and run both through the common use-case set. For each design, state distinctive strengths and FAILURE MODES (where it breaks, gets awkward, or forecloses a future need). Then surface the OPEN DECISION POINTS a synthesis must resolve — framed as forks with their tradeoffs, NOT resolved.
6. EXPLORE PRIOR ART. Spawn sub-agent(s) to survey prior art for patterns, pitfalls, abstraction boundaries, and ubiquitous language. Cover at least:
   - Grammar-of-graphics / viz data models: Vega-Lite, D3, Observable Plot — the query↔viz data boundary and what shape charts consume.
   - Semantic / metrics layers: Malloy, dbt MetricFlow, LookML, Cube — the closest analogs to "domain-term queries (dimensions/measures) producing viz-ready results"; strong source of ubiquitous language.
   - Query / dataframe models: relational algebra, Polars/pandas/dplyr verbs, LINQ, PRQL.
   Report established terminology, proven boundaries, pitfalls — and call out anything BOTH A1 and A2 missed.

## Deliverable

Write `docs/analysis-comparison.md`: the terminology table, similarities, contrasts (with the commitment behind each), per-design evaluation (strengths + failure modes vs. rubric + use cases), the open decision points, and the prior-art findings. Evidence only — no winner, no final design. Make it self-contained.

## Follow-on
The maintainers will use this evidence to synthesize the final design collaboratively, decision by decision. Optimize your output to support that — make the decision points and tradeoffs crisp; do not pre-empt them with a recommendation.
