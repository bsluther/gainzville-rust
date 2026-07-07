# Malloy's Core Semantic Model — Annotated Map

> Navigational map of the Malloy TypeScript monorepo, focused on its **modeling
> approach** and the **reified, serializable data structures** that represent the
> meaning of a model and a query — for the purpose of borrowing that design (sources,
> dimensions, measures, views, queries, nest, pipelines) into a smaller Rust
> reimplementation interpreted in-memory over one user's data.
>
> Repo studied (read-only): `/Users/brianluther/dev/clones/malloy`. All `file:line`
> pointers are against that checkout.

**Headline finding:** there are *two* serializable representations of the model/query,
not one — and the one closest to what we're building is **not** the compiler's internal
IR. The compiler IR (`packages/malloy/src/model/malloy_types.ts`) is shaped for SQL
generation; the **stable interface layer** (`packages/malloy-interfaces`) is the
deliberately-stable, persistable, grammar-independent form, with a mutable builder over
it in `packages/malloy-query-builder`. Port the latter, not the former.

---

## 1. Architecture Map

**Top-level `packages/`** (only the relevant ones; the rest are skip):

- **`malloy/`** — the compiler core. Contains both the translator (text→IR) and the
  compiler (IR→SQL). This is where the internal semantic model lives. **READ**
  (selectively).
- **`malloy-interfaces/`** — the *stable, serializable, language-independent* type layer
  (`@malloydata/malloy-interfaces`). Generated from the hand-written IDL
  `thrift/malloy.thrift` (the annotated source of truth) into `src/types.ts`; depends on
  nothing but a Thrift runtime. This is the public "model-as-data" contract.
  **READ — central to our goal (prefer the `.thrift` for intent, see §3a).**
- **`malloy-query-builder/`** — a programmatic query builder
  (`@malloydata/malloy-query-builder`). Manipulates queries as an object tree, *without*
  the `.malloy` text grammar. Depends on `malloy-interfaces`, **not** on the compiler.
  **READ — the closest analog to what we're building.**
- **`malloy-filter/`** — parser/model for filter expressions (the little filter
  sub-language). Skim only.
- **`malloy-tag/`** — the `#`/`##` annotation ("tag") model. Peripheral.
- `malloy-render*`, `malloy-db-*`, `malloy-connections`, `malloy-malloy-sql`,
  `malloy-syntax-highlight`, `malloy-db-publisher` — renderer, DB drivers, SQL-embedding,
  highlighting. **SKIP.**

**Within `packages/malloy/src/`:**

- **`model/`** — **the core: the internal IR + the IR→SQL compiler.** `malloy_types.ts`
  is the IR (read this). The rest (`query_query.ts`, `expression_compiler.ts`,
  `field_instance.ts`, `query_node.ts`, `stage_writer.ts`, dialect-facing code) is
  SQL-generation machinery — **SKIP for our purposes** (it's how IR becomes SQL strings;
  we do in-memory interpretation instead). Read the two `.md` files here first
  (`CONTEXT.md`, `REFACTOR_README.md`) — they're an excellent orientation.
- **`lang/`** — the translator: ANTLR grammar, lexer, AST-from-text, and the elaboration
  that produces the IR. **SKIP** (the text-grammar front end we're not reimplementing).
- **`dialect/`** — per-database SQL codegen. **SKIP.**
- **`api/`**, **`to_stable.ts`** — the boundary that converts the internal IR ↔ the
  stable `Malloy.*` interface types. **Skim** (the bridge between the two
  representations).
- **`connection/`, `run_sql_options.ts`, `sql_block.ts`** — execution plumbing. SKIP.

---

## 2. The Core Semantic Model / IR (internal compiler IR)

All in **`packages/malloy/src/model/malloy_types.ts`**. Per `model/CONTEXT.md:11`, this IR
is **plain serializable data, not class instances** — explicitly designed to be
cached/persisted/sent over the wire. The "there's a serializable IR" framing is
confirmed.

**Top-level container**

- `ModelDef` — `malloy_types.ts:2051` — the whole model: `contents` (a
  name→`NamedModelObject` map of all sources/queries/functions), `queryList`, `exports`,
  plus annotation/source registries.
- `NamedModelObject` — `malloy_types.ts:2038` — union of what a model can contain:
  `SourceDef | NamedQueryDef | FunctionDef | ConnectionDef | UserTypeDef | GivenEntry`.

**Source / Struct (a table + its attached fields/joins/views)**

- `StructDef` — `malloy_types.ts:1743` — `SourceDef | RecordDef | ArrayDef`. The umbrella
  "structured thing with fields."
- `SourceDef` — `malloy_types.ts:1713` — union of concrete source kinds
  (`TableSourceDef`, `SQLSourceDef`, `QuerySourceDef`, `CompositeSourceDef`,
  `VirtualSourceDef`, …).
- `SourceDefBase` — `malloy_types.ts:1532` — the shared shape: `fields: FieldDef[]`,
  `connection`, `dialect`, `primaryKey`, `parameters`/`arguments`, `filterList`.
  **This is the "source = table + attached fields" type.**
- `TableSourceDef` — `malloy_types.ts:1560` — `{type:'table', tablePath}` — the simplest
  base source.

**Field — how dimension/measure/join/view are encoded** (the subtle part)

- `FieldDef` — `malloy_types.ts:2003` — `BasicAtomicDef | JoinFieldDef | TurtleDef`. The
  first discrimination is by *shape*: scalar field vs join vs nested view.
  - **Join** → `JoinFieldDef` (`malloy_types.ts:1131`), a `Joinable` + `JoinBase`
    (`:1113`, with `JoinType = 'one'|'many'|'cross'` at `:1091`).
  - **Nested view ("turtle")** → `TurtleDef` (`malloy_types.ts:1510`) — `{type:'turtle'}`
    + a `Pipeline`. A view is literally a named pipeline attached to a source.
  - **Scalar field (dimension OR measure)** → `BasicAtomicDef`, built on `FieldBase`
    (`malloy_types.ts:880`).
- **Dimension vs measure is NOT a separate type in the internal IR.** `FieldBase` mixes in
  `Expression` (`malloy_types.ts:491`), which carries `expressionType?: ExpressionType`.
  `ExpressionType` (`malloy_types.ts:484`) = `'scalar' | 'aggregate' | 'scalar_analytic'
  | 'aggregate_analytic' | 'ungrouped_aggregate'`. **A dimension is a field whose
  `expressionType` is `scalar`; a measure is `aggregate`.** Helpers: `expressionIsScalar`
  (`:736`), `expressionIsAggregate` (`:740`). Each field also holds its computation as
  `e?: Expr` (the expression tree) plus `code?` (the source text).

**Query and its pipeline of segments**

- `Query` — `malloy_types.ts:1242` — `{ structRef, pipeline: PipeSegment[], filterList? }`.
  A query = a source reference + an ordered pipeline. `structRef`
  (`StructRef = string | SourceDef`, `:1219`) is the source it runs against.
- `Pipeline` — `malloy_types.ts:1239` — just `{ pipeline: PipeSegment[] }`.
- `PipeSegment` — `malloy_types.ts:1259` — `QuerySegment | IndexSegment | RawSegment`.
- `QuerySegment` — `malloy_types.ts:1494` — the workhorse: `{ type:'reduce'|'project'|
  'partial', queryFields: QueryFieldDef[], filterList?, orderBy?, limit?, extendSource? }`.
  - **`reduce`** (`ReduceSegment`, `:1283`) = group_by + aggregate (a grouping stage).
  - **`project`** (`ProjectSegment`, `:1297`) = select/projection (grain-preserving).
- `QueryFieldDef` — `malloy_types.ts:2014` — what lives in a segment:
  `AtomicFieldDef | TurtleDef | RefToField`. A **nest** is a `TurtleDef` appearing in a
  segment's `queryFields`; a `group_by:`/`aggregate:`/`select:` of an existing field is a
  `RefToField` (`:2008`, `{type:'fieldref', path:string[]}`); an inline computed column is
  an `AtomicFieldDef`.

So in the internal IR: **group_by/aggregate/project are not distinct node types** —
they're fields inside a `reduce`/`project` segment, distinguished by the field's
`expressionType`. (Contrast with the stable layer below, where they *are* distinct ops.)

**Expression IR**

- `Expr` — `malloy_types.ts:68` — a discriminated-union tree (~45 node kinds):
  `BinaryExpr`, `FunctionCallNode`, `AggregateExpr`, `FieldnameNode`, literals, time ops,
  casts, filters, etc. Node base interfaces `ExprLeaf`/`ExprE`/`ExprWithKids` at `:41–56`
  (kids are stored for generic tree-walking).
- `Expression` — `malloy_types.ts:491` — the *wrapper* a field uses to hold an
  expression: `{ e?: Expr, expressionType?, code? }`.

---

## 3. Programmatic Query Representation — the closest analog to our project

This is the part to study hardest. Malloy reifies model + query as **plain serializable
data in `malloy-interfaces`**, and exposes a **mutable builder over it in
`malloy-query-builder`** — both fully independent of the `.malloy` text grammar.

### (a) The serializable data model — `packages/malloy-interfaces/src/types.ts`

Consumed as `Malloy.*`. Unlike the internal IR, here dimension/measure/join/view/calculate
are **explicit first-class kinds**.

> **`types.ts` is generated — read the Thrift for intent.** The file is emitted by the
> `generate-types` npm script from the hand-written IDL
> **`packages/malloy-interfaces/thrift/malloy.thrift`** (793 lines) via a custom
> `scripts/hacky_gen_types.ts`. (A parallel `@creditkarma/thrift-typescript` pass writes a
> throwaway `generated-types/` dir purely to *validate* the thrift; the shipped file comes
> from the hacky generator.) The generated `.ts` **strips all comments**, so the `.thrift`
> is where the design reasoning lives — especially `TODO`s flagging open decisions. Every
> pointer below now gives **both**: `types.ts` for the concrete TS shape, `malloy.thrift`
> for the annotated source. Load-bearing comments worth reading first: the design-questions
> block above `ViewOperation` (`malloy.thrift:260`), `FieldInfo` second-guessing the
> dimension-vs-measure split (`:99`, `:104`), the `FilterOperation` union-nesting workaround
> (`:315`), the `Field`-vs-`GroupBy` shape asymmetry (`:608`), and the deliberately-omitted
> result metadata (`:568`).

- `ModelInfo` — `types.ts:2295` / `malloy.thrift:6` — `{ entries: (SourceInfo|QueryInfo)[],
  annotations?, anonymous_queries }`.
- `SourceInfo` — `types.ts:2554` / `malloy.thrift:17` — `{ name, schema: Schema,
  parameters? }`; `Schema` (`types.ts:2550` / `malloy.thrift:82`) = `{ fields: FieldInfo[] }`.
- `FieldInfo` — `types.ts:2101` / `malloy.thrift:90` — discriminated union on `kind`;
  `FieldInfoType` (`types.ts:2094`) = **`'dimension' | 'measure' | 'join' | 'view' |
  'calculate'`** (the thrift `union FieldInfo` variant tags at `malloy.thrift:90–96`).
  Variants: `DimensionInfo` (`types.ts:2035` / `malloy.thrift:100`), `MeasureInfo`
  (`types.ts:2279` / `malloy.thrift:112`), `JoinInfo` (`types.ts:2203` / `malloy.thrift:125`,
  with `relationship`; `Relationship` enum at `malloy.thrift:119`), `ViewInfo`
  (`types.ts:2649` / `malloy.thrift:132`), `CalculateInfo` (`types.ts:1895` /
  `malloy.thrift:142`).
- `Query` — `types.ts:2413` / `malloy.thrift:366`; `QueryDefinition` (`types.ts:2437` /
  `malloy.thrift:371`) = `arrow | query_reference | refinement`.
- `ViewDefinition` — `types.ts:2631` / `malloy.thrift:392` — `arrow | view_reference |
  refinement | segment`.
- `ViewSegment` — `types.ts:2703` / `malloy.thrift:409` — `{ operations: ViewOperation[] }`
  — **the linear pipeline.**
- `ViewOperation` — `types.ts:2667` / `malloy.thrift:272` — **the operation union, keyed on
  `kind`**: `group_by | aggregate | order_by | limit | where | nest | having | drill |
  calculate`. (e.g. `GroupBy` at `types.ts:2192` / `malloy.thrift:284` = `{name?, field:
  Field}`.)
- `Field` — `types.ts:2089` / `malloy.thrift:299` — `{ expression: Expression,
  annotations? }`. `Expression` (`types.ts:2055` / `malloy.thrift:470`) is a small
  structured union (`field_reference | time_truncation | filtered_field | literal_value |
  moving_average`) — **structured, not raw SQL strings.**

### (b) The mutable builder — `packages/malloy-query-builder/src/query-ast.ts`

A tree of wrapper classes over those `Malloy.*` types, with edit-tracking:

- `ASTNode<T>` — `query-ast.ts:42` — base; tracks `.edited`, has `.build(): T`,
  propagates edits up a parent chain.
- `ASTQuery` — `query-ast.ts:613` — **root/entry**; `new ASTQuery({query?,
  source/model})`. `getOrAddDefaultSegment()` gives you a segment to mutate.
- `ASTSegmentViewDefinition` — `query-ast.ts:2441` — the main mutation target; holds the
  operation list.
- Operation nodes: `ASTGroupByViewOperation` (`:3481`), `ASTAggregateViewOperation`
  (`:3662`), `ASTNestViewOperation` (`:4447`), `ASTOrderByViewOperation` (`:3425`),
  `ASTWhereViewOperation` (`:4608`).
- Mutation API (all return the created node): `addGroupBy(name, path?, rename?)` (`:2903`),
  `addAggregate(...)` (`:3139`), `addNest(name, rename?)` (`:3166`), `addOrderBy(...)`
  (`:2624`). Note these are thin: `addGroupBy` → `makeField(..., 'dimension')`;
  `addAggregate` → `'measure'`; `addNest` → `'view'`.
- Serialize: `.build(): Malloy.Query` (`:922`, reconstructs only dirty subtrees) and
  `.toMalloy(): string` (`:912`). Construct-from-data: the `ASTQuery` constructor wraps an
  existing `Malloy.Query`.

End-to-end shape (from the spec, `query-ast.spec.ts`):

```ts
const segment = q.getOrAddDefaultSegment();
segment.addGroupBy('d1');
segment.addGroupBy('d2');
segment.addAggregate('m1');
// → Malloy.Query:
// view: { kind:'segment', operations: [
//   {kind:'group_by', field:{expression:{kind:'field_reference', name:'d1'}}},
//   {kind:'group_by', field:{expression:{kind:'field_reference', name:'d2'}}},
//   {kind:'aggregate', field:{expression:{kind:'field_reference', name:'m1'}}},
// ]}
// equivalent malloy: run: s -> { group_by: d1 d2, aggregate: m1 }
```

**For the Rust design, this `malloy-interfaces` shape (explicit `kind`-tagged ops +
first-class dimension/measure/view fields) is a better template than the internal IR** —
it's the deliberately-stable, persistable, grammar-independent form, and it maps cleanly
to Rust enums (`enum ViewOperation { GroupBy{..}, Aggregate{..}, Nest{..}, ... }`).

---

## 4. The Conceptual Layering

Malloy keeps **model-as-data** strictly separate from **behavior**.

1. *Model definition* (sources, fields, joins, views) and
2. *queries* (a source ref + a pipeline of operations)

are both pure serializable data — twice over: the internal IR
(`model/malloy_types.ts`, optimized for SQL generation) and the stable public interface
(`malloy-interfaces/types.ts`, optimized for stability/tooling).

3. *Execution/compilation* is the only "behavior" layer: the translator (`src/lang/`)
   turns text into IR, and the compiler (`src/model/query_query.ts` +
   `expression_compiler.ts`) turns IR into SQL — but per `model/CONTEXT.md:376`, the
   compiler **never mutates the IR** and is stateless, so the model is data and the
   compiler is a pure function over it.

The boundary files between the two data representations are
**`packages/malloy/src/to_stable.ts`** and **`packages/malloy/src/api/`** (internal IR ↔
`Malloy.*`); the boundary between data and behavior is
**`model/query_model.ts`/`query_model_impl.ts`** (the entry point that takes IR and runs
compilation). Because we're interpreting in-memory over one user's data, we replace layer
(3) entirely and only need to borrow the two data layers — and really just one unified
version of them.

---

## 5. START HERE, IN THIS ORDER

1. **`packages/malloy/src/model/CONTEXT.md`** — the IR philosophy ("IR is plain
   serializable data") and the key-types index, in the authors' own words. The best
   ~380-line orientation in the repo.
2. **`packages/malloy/src/model/REFACTOR_README.md`** — why `model/` splits into IR vs
   compiler; tells you exactly what to skip.
3. **`packages/malloy-interfaces/src/types.ts:2667`** (`ViewOperation`; source
   **`thrift/malloy.thrift:272`**, with the design-questions block at `:260`) — the
   cleanest statement of "a query pipeline = a list of `kind`-tagged operations." The
   single most important shape to copy.
4. **`packages/malloy-interfaces/src/types.ts:2094–2116`** (`FieldInfoType` + `FieldInfo`;
   source **`thrift/malloy.thrift:90`**, with the dimension-vs-measure `TODO`s at
   `:99`/`:104`) — dimension/measure/join/view/calculate as first-class kinds; the field
   enum.
5. **`packages/malloy-interfaces/src/types.ts:2295` & `:2554`** (`ModelInfo`,
   `SourceInfo`/`Schema`; source **`thrift/malloy.thrift:6`, `:17`, `:82`**) — the model &
   source containers in their serializable form.
6. **`packages/malloy-query-builder/src/query-ast.ts:613`** (`ASTQuery`) +
   `addGroupBy`/`addAggregate`/`addNest` (`:2903/:3139/:3166`) — how a manipulable builder
   rides on top of the serializable data; the query-builder API ergonomics.
7. **`packages/malloy-query-builder/src/query-ast.spec.ts`** (the example in §3) — see the
   whole build→data→text round-trip concretely.
8. **`packages/malloy/src/model/malloy_types.ts:1242`** (`Query`) + `:1494`
   (`QuerySegment`) — the internal IR's pipeline, to contrast: note group_by/aggregate
   collapse into `reduce` segments distinguished by `expressionType` rather than explicit
   ops. Useful to understand *why* the interface layer re-explicitizes them.
9. **`packages/malloy/src/model/malloy_types.ts:880`** (`FieldBase`) + `:484`
   (`ExpressionType`) — how the internal IR derives dimension/measure from `expressionType`
   rather than a tag. Decide which approach to use (recommend the interface layer's
   explicit tags).
10. **`packages/malloy/src/model/malloy_types.ts:68`** (`Expr`) — only if you want a richer
    structured expression tree than `malloy-interfaces`' small `Expression` union; this is
    the full ~45-node version.

**One correction to a possible assumption:** don't model the persisted query on the
compiler IR (`malloy_types.ts` `Query`/`QuerySegment`) — it's shaped for SQL generation
(carries `outputStruct`, `refSummary`, `group_set` bookkeeping, `extendSource`, etc.). The
`malloy-interfaces` `Query`/`ViewOperation` is the intended persistable/manipulable form
and is dramatically smaller and cleaner — that's the one to port to Rust.
