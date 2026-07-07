# Analysis subsystem — working design

> **Status: design in progress.** This directory is the synthesis target: one doc per
> component, plus a [glossary](./glossary.md) that pins the language. The flat
> `docs/analysis-*.md` files are the research record feeding this work — they stay as
> evidence and are superseded here component by component.

## The shape

```
authoring UI · templates · LLM assist
        │ emit / edit
   QuerySpec ──────────────── persisted + synced; ID-refs only, catalog-free, versioned
        │
   resolve  ◄──── Catalog ◄── built-in fields ∪ attribute-derived fields ∪ named metrics
        │         (derived fresh from live domain data; never stored)
        │  resolve = validate = derive result schema; runs at build / load / catalog change
        ▼
   interpret (in-memory, gv-core) ◄── SourceAdapter (forest + values → entry-grain rows)
        │
   ResultTable ─────────────── self-describing: column type / role / domain / sort
        │
   ChartSpec → renderer ────── renderer-agnostic; Swift Charts first
```

## Documents

| Doc | Covers | Status |
|---|---|---|
| [glossary.md](./glossary.md) | the GV terms; cf-notes to Malloy & friends inline | seeded |
| [base-source.md](./base-source.md) | BaseSource, Catalog, defaults, the field taxonomy | draft |
| query-spec.md | Segment/Reduce shape, filters, metrics-in-queries, calculate | planned |
| metrics.md | named metrics: stdlib set, activation, user definition form | planned |
| result-table.md | the query→viz contract (from the POC boundary findings) | planned |
| chart-spec.md | chart kinds, channel binding, presentation options | planned |
| execution.md | SourceAdapter, interpreter, spine densification, determinism | planned |
| persistence-ffi.md | SavedQuery entity, serde/versioning, uniffi crossing | planned |

## Standing commitments

Carried in from the research round; each is argued in the record below.

- Persist only the QuerySpec; schema and execution are derived, never stored.
- Expressions exist only at author time; query time is selection + parameterization.
- In-memory interpreter first; SQL lowering is a deferred server-side option.
- Output is tidy/long, self-describing; presentation binds to it, never reaches into it.
- Deterministic tiebreaks anywhere a row is selected or limited.
- Units normalize at leaves; derived arithmetic yields labeled scalars.

## Open decisions

1. **Metric tier timing** — system-derived + stdlib in v1, user-defined form in v1.5? (lean: yes)
2. **Kill `nest`** — tidy output + a top-per-group operation instead? (lean: yes)
3. **Predicate depth cap** — all/any + one group level, non-recursive across FFI? (lean: yes)
4. **Viz spec placement** — separate artifact vs embedded in the QuerySpec? (lean: separate, saved together)
5. **Correlation sequencing** — overlay-first vs join-first? (lean: overlay)

## Research record

`../analysis-features.md` (the feature bar) · `../analysis-rubric.md` ·
`../analysis-design-1.md` / `../analysis-design-2.md` (A1/A2) ·
`../analysis-comparison.md` (evidence base: codebase grounding, 13 decision points, prior art) ·
`../analysis-malloy-evaluation.md` (the leading direction; POC verdict) ·
`../analysis-query-model-notes.md` (Malloy-source structural takeaways) ·
`../analysis-architecture-query-schema.md` (resolve = validate diagram) ·
`../malloy-core-model-map.md` · sibling repo **`malloy-poc/`** (runnable queries + D3 boundary findings).
