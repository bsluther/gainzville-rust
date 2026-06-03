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

## Decision 4 — `pick_sorted` (the one concrete picker requirement)

Picking from `HashMap<Uuid, V>` requires imposing an order to index into. **Pick from a
Uuid-sorted view, not raw hash-iteration order:**

```rust
fn pick_sorted<'a, V, R: Rng>(map: &'a HashMap<Uuid, V>, rng: &mut R) -> Option<&'a V>;
```

- **Primary reason — reproducibility across builds.** `FxHashMap` uses a fixed-seed hasher,
  so iteration order is deterministic *within a binary* (seeds reproduce today). But hash
  order is an implementation detail; a `rustc-hash`/`hashbrown`/compiler bump can reshuffle it,
  breaking reproduction from old logged seeds. Uuids are seed-determined, so a sorted view
  makes `index → entity` a pure function of the key set — durable across builds. (Sorting does
  not bias the distribution; Uuids are random, so it's a stable relabeling.) This matches the
  existing "log the seed, reproduce later" workflow (`test_arbitrary_actions` logs `seed=`).
- **Secondary — shrink stability.** Under `HegelRandom`, the pick index is a shrinkable draw
  minimized toward 0; a stable ordering makes index 0 a consistent target. (Stronger *semantic*
  shrinking would use insertion/creation order — index 0 = earliest entity — which needs an
  ordered structure; a future upgrade if shrink quality demands it.)
- **Cost** is negligible at test-gen scale; cache a sorted `Vec<Uuid>` only if a picker gets hot.

Other model helpers (`pick_actor_id`, `pick_entry`, `pick_activity`, `pick_attribute`,
`pick_sequence_entry`, owner-matched `pick_owned_pair` with a value-generatable filter,
forest-children lookup for positions) are written **as needed**, all routed through
`pick_sorted`.

## Files & migration

- **`generation/lib.rs`** — make `Arbitrary` model-aware (it already takes `&C`); delete
  `ArbitraryFromMaybe`. Add a way for a harness to set/mutate the model between steps
  (`SimulationContext::model_mut` / constructor).
- **`generation/model.rs`** — add `pick_sorted` + helpers as needed; add `Model::from_world(...)`
  to build the maps from query results. Actors arrive as bare `Uuid`s (`AllActorIds`), so
  `from_world` fabricates minimal placeholder `Actor` records (generators read keys only); add
  an `AllActors` query instead only if/when a real need appears.
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