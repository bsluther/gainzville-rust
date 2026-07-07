# Analysis design — evaluation rubric

Desirable qualities to **trade off** against one another. A good choice need not be optimal in every
category.

**How to use this:**
- **Per-criterion evidence, no aggregate score, no winner.** Record where each design is strong or
  weak per criterion; do not sum to a ranking or declare a winner.
- **Assess "how gracefully," not just "whether."** Both designs can likely *do* most things; what
  discriminates is whether they do them directly or via contortions.
- **Log distinct observations per criterion.** Several criteria are related (see Structure); don't
  restate one observation under four headings.
- These criteria are derived from the project (CLAUDE.md values, the boundary/sync architecture, and
  `docs/analysis-features.md`) — **not** from either design.

---

## Gates (closer to must-pass)

A design that fails a gate is disqualified, not merely lower-scoring.

- **Feature coverage** — Can it express every use case in `docs/analysis-features.md` (now, or via a
  clearly-foreseen extension)? Structurally unable ⇒ out. *Among passers, how gracefully it covers
  them lives in the Structure criteria below.*
- **Executability & boundary feasibility** — Can a user-defined query actually execute **in-memory**
  over one user's data? Can the user-facing + persisted model cross **FFI to Swift**, **persist**,
  and **sync** via the existing Action→Mutation→Delta path *at all*? A model that fundamentally can't
  reach Swift or be stored is out.

---

## Trade-off qualities

### Structure
- **Simplicity (parsimony)** — Fewest moving parts for the job; minimal incidental complexity; does
  every abstraction earn its keep? *(About total complexity — a design can be simple yet monolithic.)*
- **Modularity (separation of concerns)** — Are concerns cleanly separated, each isolated to one
  place, with clear boundaries? Can you point to where a given concern lives? *(About decomposition —
  distinct from simplicity: a design can be well-divided yet complex.)*
- **Composability & extensibility** — Are concepts built from small, isolated building blocks that
  recombine, so that **novel, unseen** use cases are reachable by *combination* rather than new code?
  *(Composability is the mechanism; extensibility is the test — probe both: are there real building
  blocks, and do they recombine to reach cases the design didn't anticipate?)*

### Fit with the project
> **Bias flag:** these three closely match dimensions A2 emphasizes. They're included because they
> fall straight out of CLAUDE.md and the stated scope — not to favor A2. Assess A1 **charitably**
> here, and reweight if they feel over-represented.

- **Fit with existing architecture & patterns** — Does it extend the existing query/executor and
  Action→Mutation→Delta patterns and the `gv-core`/`gv-sql`/`gv-ffi` boundaries, rather than
  introducing a parallel paradigm? How *cleanly* does it cross FFI/persistence/sync (the degree,
  beyond the gate's "whether")? *(Industry patterns are covered separately under Prior art; this is
  internal fit.)*
- **Testability & determinism** — Can its types be made `Arbitrary`-generatable and run
  deterministically (same data + same query ⇒ same result), fitting the simulation / property-testing
  approach the project relies on?
- **Visualization data contract** — How clean, stable, and general is the shape query output presents
  to visualizations? Is the query-output → chart-input mapping clear and adapter-friendly, without
  leaking query internals into the viz layer? *(This is the literal in-scope deliverable.)*

### Leverage & delivery
- **Prior art** — Does it leverage established **industry** patterns and ubiquitous language
  (grammar-of-graphics; semantic/metrics layers; dataframe/relational verbs) rather than reinventing,
  and avoid known pitfalls?
- **Incremental path** — What's the smallest first slice that delivers a real feature, and does the
  design make it reachable **without** building the hardest machinery (windows, joins, the full
  algebra) first?
- **UX enablement** — Does the user-facing model let a user express intent in **domain terms**
  (activities, attributes, categories) without being exposed to the underlying machinery — i.e.
  without turning users into programmers?
