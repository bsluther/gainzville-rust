# Generation architecture: model-based generation & hegel-readiness

Design record for the `generation/` crate refactor. Captures the decisions reached
while preparing the crate for (a) less boilerplate in world-state-dependent
generators and (b) eventual integration with hegel-rust PBT.

Cross-references: [`hegel-pbt-integration-research.md`](./hegel-pbt-integration-research.md)
(PBT integration analysis), [`properties.md`](./properties.md) (PBT strategy).

## Context — the problem

`generation/` produces arbitrary domain data for deterministic-simulation (DST) and
integration tests. World-state-dependent generators currently thread the current state
in as tuples of slices — the worst case being
`ArbitraryFrom<(&[Uuid], &[Activity], &[Entry], &[Attribute])> for Action`. These tuples
grow as the domain grows and must be reconstructed and passed at every call site
(inspired by Turso's generators, but Turso avoids the strain by putting a model of system
state *inside* the generation context). `generation/model.rs` was added with that intent:
a `Model` of `HashMap`s, already an `AnyDeltaExecutor`, now reachable via
`GenerationContext::model()`.

Two problems to fix, plus one forward-looking constraint:

1. **Slice-threading strain** — migrate world-state-dependent generators to read
   `context.model()` instead of taking threaded slice tuples.
2. **Fallible generators that panic** — many generators `pick(...).expect("must be non-empty")`
   (no actors, no entries, no owned pair, no activities). This forces callers to reason
   about "can I even call this right now?".
3. **Don't over-constrain for hegel** — the `Model` should be adaptable as a hegel
   state-machine substrate later, without bending the generation API around hegel today.

## Decision 1 — two infallible traits; generators never fail

We considered a third, fallible trait (`TryArbitrary -> Result<_, GenerationError>`) for the
model-dependent generators. **Rejected:** the "missing source" condition is pervasive (it
affects nearly every entity generator), so `Result` would tax the entire API to encode
something that isn't really an error. Instead:

| Trait | Fallible? | Reads model? | Role |
|-------|-----------|--------------|------|
| `Arbitrary` (existing) | no | **yes (new)** | produce a value, reading world state from `context.model()` |
| `ArbitraryFrom<T>` (existing) | no | no | construct a value from a *logical input* (a `*Config`, sibling indices) |

- **Generators are infallible and *capable*.** Contract: *produce a well-formed value,
  preferring to reference existing model entities; where the model can't supply, fall back
  to a **valid degenerate**; only where there is no valid degenerate, **fabricate** an
  (invalid) reference.* Never panic, never `Option`, never `Result`.
- **Valid degenerates** exist for most reference fields: `activity_id: None` (anonymous
  entry), `position: None` (root entry), `default: None`. These deliver the real goal:
  *empty model → limited but **valid** data.*
- **Fabrication** is needed only for fields with no valid empty answer — essentially just
  `owner_id` (every entity needs an owner that ought to exist). On an empty model,
  `Activity::arbitrary` therefore yields a well-formed `Activity` with a fabricated owner —
  reference-invalid, but a legitimate test input.
- **Delete** the dead `ArbitraryFromMaybe` trait (defined, never used). Keep the four
  `ArbitraryFrom<&{Attribute,Numeric,Select,Mass}Config>` impls (`generation/attribute.rs`)
  unchanged — they are construction-from-logical-input and don't touch world state.

### Two flavors of "invalid", kept distinct
- **Bad references** — fabricated ids when the model can't supply. The fabrication capability.
- **Bad combinations** — real refs in a shape the mutator rejects (e.g. today's `MoveEntry`
  position/temporal combos). Happens naturally because generators don't self-validate; kept.

## Decision 2 — the empty-model guarantee lives at the selection layer

"No actors → we can't do anything" is *not* quite true: **`CreateUser` needs no
pre-existing state and bootstraps everything** (it mints a fresh actor). So the
"always *validly* succeeds" guarantee lives at the **`Action`** level, with `CreateUser`
as the universal valid floor. At the **individual-entity** level (`Activity::arbitrary`,
`Attribute::arbitrary`), an empty model can only yield reference-invalid data — fine, and
infallible.

### Validity levels (the lens for `p_valid`)
1. **Struct** — guaranteed by the type system; free.
2. **Reference** — do referenced ids exist? Model-read vs fabricate.
3. **Constraint** — domain rules (no cycles, template-temporal rules, owner-matched values,
   child only under a sequence). `p_valid` *aims* to follow all rules so that a valid-by-
   construction action the system *rejects* is a signal (a bug), not noise — best-effort,
   because constraint-level validity is the expensive one to encode. The reference `Model`
   (`AnyDeltaExecutor`) is the eventual oracle: apply an action to both the real system and
   `Model` and assert agreement. (`Model` currently checks only PK consistency, not
   constraints — strengthening it is future work.)

### This pass vs. later (scope line)
- **This pass:** `Action::arbitrary` is a baseline selector that offers **only variants whose
  *reference-level* preconditions the model satisfies** (gating table below), picks
  **uniformly** among them. This reproduces today's behavior plus the empty-model→`CreateUser`
  guarantee, with **no** panics.
- **Deferred:** deliberate invalid-injection, explicit `p_valid` distribution control, and a
  pluggable `Workload` layer (Turso/Antithesis/TigerBeetle style: *generators enable
  behavior, workloads utilize it*; Turso expresses "not applicable now" as
  `Workload::generate -> Option<Operation>`). **Accept that valid/invalid ratios are
  approximate near an anemic model** — want accurate distributions, grow the model or run
  longer; don't engineer the bootstrap to make `p_valid` exact when state is thin.

### Gating table (reference-level preconditions)
| Variant(s) | Offered when |
|-----------|--------------|
| `CreateUser` | always (valid floor) |
| `CreateActivity`, `CreateAttribute`, `CreateEntry` | `!model.actors.is_empty()` |
| `MoveEntry`, `UpdateEntryCompletion`, `UpdateEntry` | `!model.entries.is_empty()` |
| `CreateValue`, `AttachValue`, `DeleteAttributeValue` | an owner-matched, value-generatable `(entry, attribute)` pair exists |
| `CreateEntryFromActivity` | `!model.activities.is_empty()` |

(`DeleteEntryRecursive` and `UpdateAttributeValue` are `Action` variants but are not
generated today; left out of scope, can be added later.)

## Decision 3 — RNG/model location and the hegel seam

- **Mutation is owned above the generator.** `Arbitrary` only mutates the RNG; it reads
  `context.model()` read-only (`&C`). Evolving the model between simulation steps
  (apply the action's deltas) is the harness/workload's job, not the generator's. This
  matches Turso and hegel both (`#[rule] fn(&mut self, tc)` — the state machine owns its
  pools, the test case is separate).
- **The abstraction seam is `GenerationContext`, and the thing that swaps is the RNG**, not
  the model:
  - **DST:** `ChaCha8Rng` + concrete `Model` (HashMaps). `Action::arbitrary(&mut rng, &ctx)`.
  - **Hegel:** the state machine's `self` holds the concrete `Model`; each `#[rule]` does
    `let mut rng = <shim>(tc.draw(randoms()))` and calls **the same `Action::arbitrary`**.
    `HegelRandom` routes every `random_range`/`random_bool` through the shrinking engine, so
    our rng-index picks over the materialized `Model` are shrinkable draws — the query-rich
    generators reuse **wholesale**.
- **`Variables<T>` is *not* the seam.** Verified against the hegel clone (`src/stateful.rs`):
  `Variables<T>` is an opaque, engine-driven, draw-only pool (`is_empty`/`len`/`add`/`draw`/
  `consume`; keyed by an engine-assigned `i64`; `draw()` picks via the test case and calls
  `assume(false)` to skip a rule when empty). It exposes **no iteration/filter/lookup**, so it
  cannot back a query-rich `Model` trait, and our constraint-aware generation needs queries
  and *relations* (owner-matching, sequence parents, non-cyclic targets) it can't express.
  Forcing `Model` down to the `Variables` interface would gut the capability that makes the
  model worth having. `Variables` stays **optional** — reach for it only where hegel's
  stronger provenance-based shrinking on one hot handle is worth a native rewrap. The
  `Model`'s other hegel role — reference **oracle** — is unaffected.

## Decision 4 — the `Model` is RNG-free; picking lives in the generation layer

Generators never touch the `Model`'s storage (`model.entries`, …) directly, and the `Model`
never sees an RNG. The split:

- **`Model`: RNG-free read accessors.** `actors()`, `entries()`, `activities()`, … return
  `impl Iterator<Item = &T>`; domain-meaningful *queries* (e.g. `sequence_entries()`,
  `attributes_owned_by(owner)`) are also RNG-free `.filter(...)` accessors, added as needed.
  No `rng`, no sampling, no distribution. This keeps the `Model` a pure, deterministic object
  — exactly what its *other* role needs (the reference **oracle**, an `AnyDeltaExecutor`).
  Putting picking on the `Model` would conflate "what the state is" with "which one we chose,
  how often" and contaminate the oracle.
- **Generation owns all randomness.** The existing `pick`/`maybe`/`random_range`, or
  `rand::seq::IteratorRandom::choose`, consume the `Model`'s accessors. The *distribution*
  lives here, never on the `Model`.

**There is no `pick_sorted`.** An earlier draft proposed picking from a Uuid-sorted view for
reproducibility; that reasoning doesn't hold:

- **Cross-build reproducibility was the weak part.** A `rustc-hash`/`hashbrown` bump lands in
  `Cargo.lock`, which is committed — so "reproduce across a hashmap-order change" *is*
  "reproduce across a commit," never a reasonable expectation (any source change can alter the
  code path). When actually debugging you sit on a pinned commit + lockfile.
- **Within-build reproducibility — the only reasonable kind — is already free.** `FxHashMap`
  uses a fixed-seed hasher (no per-process randomization), so for a given seed → insertion
  sequence, iteration order is deterministic across runs of the same binary, and picks consume
  the RNG identically regardless of order. So raw hash-order picking reproduces fine — and is
  cheaper than sorting.

**Ordering is a `Model` storage detail, swappable behind the accessors.** Today it's a
`HashMap` (hash order). The one residual reason to ever impose an order is *future, hegel-only*:
shrink **convergence** — as hegel minimizes a command sequence it removes entities, and a stable
order keeps `index → entity` from reshuffling as the pool shrinks. If that bites, swap the
backing to insertion order (`IndexMap`/seq-counter) behind the accessor — **no generator
changes** — and insertion order (index 0 = earliest entity) is the meaningful shrink target,
strictly better than the Uuid-sort once floated.

**Picking mechanics (generation side).** `IteratorRandom::choose(rng)` avoids a `collect()`:
for an exact-size iterator (an *unfiltered* accessor over `HashMap::values()`) it does a single
`random_range + nth` — one RNG draw, no allocation. For a *filtered* accessor the `size_hint`
isn't exact, so `choose` uses reservoir sampling (one draw per element): still uniform and
allocation-free, but more draws — marginally worse for hegel shrink granularity and
server-backend IPC. A `collect()` + single `random_range` is the one-draw alternative there; at
test-gen volumes the difference is negligible, so it's a defer-able call. Domain helpers
(`sequence_entries`, owner-matched pairs with a value-generatable filter, forest-children for
positions) are written **as needed** — as RNG-free `Model` queries, with the pick done in
generation.

## Files & migration

- **`generation/lib.rs`** — make `Arbitrary` model-aware (it already takes `&C`); delete
  `ArbitraryFromMaybe`. Add a way for a harness to set/mutate the model between steps
  (`SimulationContext::model_mut` / constructor).
- **`generation/model.rs`** — add RNG-free read accessors (`entries()`, `activities()`, …, plus
  domain queries like `sequence_entries()`) as needed; **no picking/RNG on `Model`** (Decision 4).
  Add `Model::from_world(...)` to build the maps from query results. Actors arrive as bare
  `Uuid`s (`AllActorIds`), so `from_world` fabricates minimal placeholder `Actor` records
  (generators read keys only); add an `AllActors` query instead only if/when a real need appears.
- **`generation/actions.rs`** — convert the ~11 world-state impls from `ArbitraryFrom<slices>`
  to model-reading `Arbitrary`; rewrite `Action`'s `choices` gating per the table; remove the
  `.expect(...)` panics (replace with model reads + valid-degenerate/fabricate).
- **`generation/entry.rs`** — single model-aware `Arbitrary for Entry` (collapsed; no separate
  leaf impl — round-trip tests overwrite `owner_id`/`activity_id`/`position` anyway).
  `Option<Position>` becomes model-aware `Arbitrary`. `FractionalIndex` from
  `&[FractionalIndex]` stays `ArbitraryFrom`.
- **`generation/attribute.rs`** — `Value` becomes model-aware `Arbitrary`; the `*Value`-from-
  `*Config` impls stay `ArbitraryFrom`.
- **`generation/activity.rs`** — `Activity` becomes model-aware `Arbitrary`.

### Consumers (in scope — workspace must compile)
- **`integration-tests/tests/postgres_tests.rs`** — `test_arbitrary_create_entry` (~114–120)
  and `test_arbitrary_actions` (~146–195): seed/rebuild a `Model` per iteration via
  `Model::from_world(...)` and call the new model-reading `Arbitrary` impls; drop the manual
  std_lib-attribute workaround (value actions are simply not offered until an owner-matched
  attribute exists).
- **`gv-sql/tests/entry_round_trip_sqlite.rs`** and `postgres_tests::test_move_entry_disallows_cycles`
  — use `Entry::arbitrary` then overwrite reference fields; unaffected by the collapse.

## Verification

1. `cargo test -p generation` — existing no-panic/distribution tests pass; add a test that
   `Action::arbitrary` against an **empty** model returns `CreateUser` (never panics/errors)
   and against a `seed_basic()` model returns a spread of variants.
2. **From the workspace root** `cargo test` (per `CLAUDE.md` — feature unification; needs the
   postgres docker container up for `gv-server`/sqlx compile-time verification). Confirms the
   consumer migrations compile and the arbitrary-action tests pass, seed reproducible.
3. `cargo test -p gv-sql --features sqlite entry_round_trip` — confirms the collapsed
   `Arbitrary for Entry` still round-trips.