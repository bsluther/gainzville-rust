# Analysis architecture — Catalog / QuerySpec / QueryResultSchema (sketch)

> Draft supporting the high-level analysis architecture diagram. Covers one section:
> how the result schema is derived and validated. Two conventions used throughout:
> **rectangles = functions/modules (code)**, **rounded = data (types)**; persisted vs
> derived is color-coded.

## Main form: two-input derivation

The persisted `QuerySpec` stays **catalog-free** (ID references only), so the schema is
`f(QuerySpec, Catalog_now)` — the catalog is a *fresh* input on every derivation, which is
what makes derivation double as the **staleness gate**. Derivation runs at three trigger
points: **build** (feeds VizBuilder), **load** (validate a saved artifact), and
**catalog change** (subscription-driven revalidation). GV's additive-only attribute
configs make most catalog changes *compatible* (domains grow; charts gain empty slots);
true staleness is confined to deletions and structural changes.

```mermaid
flowchart TB
    classDef data fill:#1f2937,stroke:#94a3b8,rx:12,ry:12,color:#e5e7eb
    classDef persisted fill:#1e3a5f,stroke:#60a5fa,rx:12,ry:12,color:#e5e7eb
    classDef func fill:#111827,stroke:#e5e7eb,color:#e5e7eb
    classDef err fill:#3f1d1d,stroke:#f87171,rx:12,ry:12,color:#e5e7eb

    gv(["GV activities + attributes"]):::data
    catalog(["Catalog — the source schema"]):::data
    qb["QueryBuilder (UI)"]:::func
    spec(["QuerySpec — ID-refs only, catalog-free"]):::persisted
    derive["derive_schema = validate<br/>runs at: build · load · catalog change"]:::func
    schema(["QueryResultSchema — derived, never stored"]):::data
    stale(["Stale(reason) — names the broken leaf"]):::err

    gv -- "derive (schema-side twin of SourceAdapter)" --> catalog
    catalog -- "browse / pick" --> qb
    qb -- "emit" --> spec
    spec --> derive
    catalog --> derive
    derive -- "ok" --> schema
    derive -- "err" --> stale
```

- `QuerySpec` (blue) is the persisted declaration; everything else is derived or code.
- If the spec *embedded* catalog knowledge (types/domains), it would be a snapshot: no
  staleness detection, and compatible growth (a select gaining an option) wouldn't reach
  saved charts.

## Variant: reify the resolved query

"Everything after the catalog is a function of the query" is true of the **resolved**
form, not the persisted one — the classic unresolved-AST vs bound/typed-AST split. If the
diagram should foreshadow the implementation (the resolved/lowered IR will likely exist as
a concrete type), draw it this way:

```mermaid
flowchart TB
    classDef data fill:#1f2937,stroke:#94a3b8,rx:12,ry:12,color:#e5e7eb
    classDef persisted fill:#1e3a5f,stroke:#60a5fa,rx:12,ry:12,color:#e5e7eb
    classDef func fill:#111827,stroke:#e5e7eb,color:#e5e7eb

    catalog(["Catalog"]):::data
    spec(["QuerySpec (persisted)"]):::persisted
    resolve["resolve = validate"]:::func
    resolved(["ResolvedQuery — bound to the live catalog"]):::data
    schema(["QueryResultSchema"]):::data
    runner["QueryRunner"]:::func

    catalog --> resolve
    spec --> resolve
    resolve --> resolved
    resolved -- "pure" --> schema
    resolved --> runner
```

Same semantics, one more type: the catalog feeds **resolution**, and both the schema and
the runnable plan follow purely from `ResolvedQuery`.
