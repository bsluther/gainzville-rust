# Integrating `generation/` `Arbitrary` impls with hegel-rust (PBT)

Research note. Question: can the existing `Arbitrary`/`ArbitraryFrom` impls in
the `generation` crate be reused to drive **hegel-rust** property-based tests, or
would PBT force us to duplicate all of them? And — separately — how much
duplication is actually at stake if reuse turns out to be infeasible?

Sources consulted:
- hegel skill overview: <https://github.com/hegeldev/hegel-skill/blob/main/skills/hegel/SKILL.md>
- Rust crate (`hegeltest`): <https://docs.rs/hegeltest/latest/hegel/>
- hegel-rust source: <https://github.com/hegeldev/hegel-rust>
- Local clone read directly: `~/dev/clones/hegel-rust` (crate `v0.14.17`). The
  claims below about the RNG bridge, the per-draw transport, and the stateful
  API are verified against that source, not just docs.

## TL;DR

- **Reuse is feasible and is the right call.** Hegel already ships the exact
  bridge we need: `hegel::extras::rand::randoms()` yields a `HegelRandom` RNG
  that is *backed by test-case data and shrinkable*. You `tc.draw(randoms())` to
  get an RNG and then call our existing `T::arbitrary(&mut rng, &ctx)` verbatim.
- **The one real obstacle is a `rand` version skew, not a design mismatch.**
  `generation` is on `rand 0.9.2`; hegel's rand bridge is on `rand 0.10`. The two
  `Rng` traits are different types, so hegel's RNG won't satisfy our `R: rand::Rng`
  bound directly. Resolve with either a ~20-line version-shim newtype (keep 0.9)
  or a workspace `rand` 0.9→0.10 bump.
- **The rand path must stay regardless of hegel.** DST and dev-seeds need fast,
  offline, seed-reproducible generation. Hegel's engine (Hypothesis-backed,
  shrinking-oriented, `uv`/server-assisted) is not built for that. So "rewrite
  natively for hegel" is not *reuse vs rewrite* — it's *reuse vs maintain two
  generator hierarchies in lockstep forever*. That reframes the duplication cost.
- **Two integration tracks, not one.** Stateless `Arbitrary` impls reuse for free
  through the RNG bridge. The context-dependent `ArbitraryFrom<(&[Entry], …)>`
  impls and the `Action` generator are a *model-based command generator* — those
  map to hegel's stateful testing (`stateful.rs`) and reuse only *partially*
  (keep the per-command bodies, rewrap the orchestration).
- **Per-draw cost is the one perf wrinkle, and it has a built-in fix.** On the
  default **server backend**, every draw is a synchronous CBOR round-trip to a
  Python subprocess over a pipe — and our impls fan one value into dozens of
  draws. The **`native` backend** (`--features native`) makes `generate`
  in-process (no IPC), trading shrink quality we don't need for the round-trip
  suite. So use native for high-volume reuse, server for the lower-volume Track B
  stateful tests. A ~30-line spike just picks the backend per suite; it no longer
  threatens the architecture.

## How hegel works, and how it relates to `Arbitrary`

Your mental model ("generators are the analogue of `arbitrary`, but they draw
from the hypothesis server") is correct, with one inversion of control worth
making explicit.

Our trait pulls entropy from an RNG **we own**:

```rust
pub trait Arbitrary {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, context: &C) -> Self;
}
```

Hegel inverts that. A test receives a `TestCase` and *draws* typed values:

```rust
#[hegel::test]
fn test_matches_builtin(tc: TestCase) {
    let mut v = tc.draw(gs::vecs(gs::integers::<i32>()));
    // ...
}
```

`TestCase::draw` has signature `fn draw<T: Debug>(&self, g: impl Generator<T>) -> T`
(note: `&self`, which matters below). A `Generator<T>` does not receive a
`rand::Rng`; it implements `as_basic`/`do_draw` against the `TestCase`, whose
bytes come from the Hypothesis engine so that failing cases can be **shrunk**
server-side to a minimal counterexample. That shrinking — plus coverage feedback
and a persisted failing-example corpus — is the entire reason to route generation
through hegel instead of a plain `rand::Rng`.

So `Generator<T>` ≈ `Arbitrary for T`, but the control is inverted: hegel pushes
bytes in (for shrinkability) rather than us pulling them from an RNG we seeded.

## The reuse mechanism: hegel already ships the RNG bridge

The naive worry is that inverted control forces us to rewrite every generator in
hegel's `tc.draw`-per-field style. It does not. The skill's guidance is explicit:

> When code under test requires an RNG, do not seed it with a Hegel-generated
> integer — Hegel cannot shrink individual random decisions that way. Instead,
> use Hegel's random generator, which routes all random calls through the
> shrinking engine for fine-grained control.

That "Hegel random generator" is real and shipped in `src/extras/rand/`:

```rust
pub fn randoms() -> RandomsGenerator;

pub enum HegelRandom {
    ArtificialRandom(TestCase), // "Backed by test case data. Shrinkable."
    TrueRandom(Box<StdRng>),
}

impl TryRng for HegelRandom {
    type Error = Infallible;
    fn try_next_u32(&mut self) -> Result<u32, Self::Error>;
    fn try_next_u64(&mut self) -> Result<u64, Self::Error>;
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error>;
}
```

This is exactly the adapter the integration needs, already written. Verified in
`src/extras/rand/generators.rs`: `try_next_u32`/`try_next_u64` each call
`integers().do_draw(tc)`, and `try_fill_bytes` calls one sized `binary().do_draw(tc)`.
So every random
decision in our `arbitrary` bodies is an individually-tracked, shrinkable draw —
*not* an opaque PRNG seeded from a single integer (the anti-pattern the skill
warns against). The intended shape:

```rust
#[hegel::test]
fn entry_round_trips(tc: TestCase) {
    let mut rng = tc.draw(gs::randoms());        // HegelRandom — shrinkable
    let ctx = SimulationContext::default();
    let entry = Entry::arbitrary(&mut rng, &ctx); // existing impl, reused as-is
    // round-trip assertion ...
}
```

The bridge is behind hegel's optional `rand` feature:
`cargo add --dev hegeltest --features rand`.

## The one real obstacle: `rand` 0.9 vs 0.10

`HegelRandom` implements `TryRng` (fallible, `Error = Infallible`). Our trait
bound is `R: rand::Rng` (infallible). Within a single `rand` version that gap is
a one-call adapter — `rand_core` provides `TryRngCore::unwrap_mut()` /
`unwrap_err()` to view an infallible `TryRng` as an `RngCore` (and `Rng` is a
blanket impl over `RngCore`). So *conceptually* it's free.

The catch is the version skew:

| Crate | `rand` | `rand_core` |
|-------|--------|-------------|
| `generation` (this repo) | **0.9.2** | 0.9.3 |
| hegel-rust rand bridge | **0.10** (optional) | (re-exported) |

`rand` 0.9 → 0.10 is a breaking release: the `Rng`/`RngCore`/`TryRngCore` traits
in 0.10 are a *different type* from the 0.9 ones. So `HegelRandom` satisfies
`rand 0.10`'s `Rng`, while `Entry::arbitrary` wants `rand 0.9`'s `Rng`. They do
not unify, and the `unwrap_mut()` adapter only bridges fallible→infallible
*within the same major*, not across 0.9↔0.10.

Two ways out:

1. **Version-shim newtype (~20 LOC, keeps `generation` on 0.9).** Wrap hegel's
   0.10 RNG and re-expose it as a `rand_core` 0.9 `RngCore` by forwarding
   `next_u32`/`next_u64`/`fill_bytes` to the 0.10 `TryRng` methods (all
   `Infallible`, so `.unwrap()` is total). Multiple `rand` majors already coexist
   in our lock file (0.8.5 *and* 0.9.2 are both present today), so pulling in
   hegel's 0.10 on the dev/test path is not a new kind of problem. This is the
   lowest-friction option and isolates the skew to one file.

   ```rust
   // dev/test-only shim. hr: hegel's rand 0.10 RNG; exposes rand 0.9 RngCore.
   struct Rng09<'a>(&'a mut hegel::extras::rand::HegelRandom);
   impl rand_core::RngCore for Rng09<'_> {
       fn next_u32(&mut self) -> u32 { self.0.try_next_u32().unwrap() }
       fn next_u64(&mut self) -> u64 { self.0.try_next_u64().unwrap() }
       fn fill_bytes(&mut self, d: &mut [u8]) { self.0.try_fill_bytes(d).unwrap() }
   }
   ```
   (Exact method set/signatures to match `rand_core` 0.9.)

2. **Bump the workspace `rand` to 0.10.** Then the trait types align and hegel's
   RNG (via `unwrap_mut`) drives our impls with no shim. Cleaner end state, but a
   0.9→0.10 migration touches every `rng.random_range(...)`/`random_bool(...)`
   call site in `generation` and requires a `rand_distr` bump (currently
   `rand_distr 0.5.1`, which pairs with `rand` 0.9 and is used for the Normal
   sampling in `DateTime`/duration generation). Only worth it if we want the 0.10
   migration for its own sake.

Recommendation: start with the shim (option 1). It unblocks PBT today without a
workspace-wide churn, and option 2 remains available later.

## Two integration tracks

Not all 35 impls reuse the same way. They split cleanly:

### Track A — stateless `Arbitrary` (reuse is free via the bridge)

These generate a value from entropy alone, no world state:

- `Email`, `Uuid`, `Username`, `DateTime<Utc>` (`lib.rs`)
- `FractionalIndex`, `Temporal` (`entry.rs`)
- `ActivityName` (`activity.rs`)
- `AttributeConfig`, `NumericConfig`, `SelectConfig`, `MassConfig` (`attribute.rs`)
- `CreateUser` (`actions.rs`)

For these, `tc.draw(randoms())` + shim + existing impl = done. No logic copied.

### Track B — context-dependent `ArbitraryFrom<…>` (partial reuse, stateful)

These take *existing world data* and produce a value that references it —
e.g. `ArbitraryFrom<(&[Entry], &[Attribute])> for Value` picks an
(entry, attribute) pair with matching owners; `ArbitraryFrom<(&[Uuid],
&[Activity], &[Entry], &[Attribute])> for Action` gates which action variants are
even legal on what currently exists. This is precisely a **model-based command
generator**: given the current model, draw a valid next command.

Hegel supports this directly via `src/stateful.rs`, and the API (read from the
clone) maps onto our domain almost one-to-one:

```rust
// hegel::stateful
pub struct Variables<T> { /* a pool of generated handles */ }
impl<T> Variables<T> {
    fn add(&mut self, v: T);   // register a newly-created entity
    fn draw(&self) -> &T;      // pick an existing one (engine-driven choice)
    fn consume(&mut self) -> T;
    fn is_empty(&self) -> bool;
    fn len(&self) -> usize;
}
pub trait StateMachine { /* with check_invariants */ }
pub fn run(m: impl StateMachine, tc: TestCase);

#[hegel::state_machine]
impl LedgerTest {
    #[rule] fn create_account(&mut self, tc: TestCase) { /* draws, mutates self */ }
    #[rule] fn transfer(&mut self, tc: TestCase) { let from = self.accounts.draw(); … }
}
```

The mapping to our `Action` system:

- `Variables<Entry>`, `Variables<Attribute>`, `Variables<Activity>`,
  `Variables<Uuid>` (actors) **replace the `&[Entry]`/`&[Attribute]` slices**
  threaded through every `ArbitraryFrom<(…)>`. `add`/`draw` replace our `pick(...)`.
- Each `Action` variant becomes a `#[rule]`. The rule body **reuses the existing
  value-construction logic** (owner-matching via `pick_owned_pair`,
  config-consistent value generation, fractional-index placement) — that domain
  knowledge ports over directly.
- The **legal-action gating** in `Action::arbitrary_from` (which variants are
  available given non-empty entries / owner overlap) is replaced by hegel's rule
  selection plus `Variables::is_empty()` guards. The engine chooses the command;
  we no longer hand-maintain the `choices: Vec<u8>` table.
- `generation::model::Model` (already an `AnyDeltaExecutor` with insert/update/
  delete invariants) **is the reference model**: a rule applies its action through
  the real mutator *and* against `Model`, and `check_invariants` asserts they
  agree. That's the classic model-based property, and `Model` already exists.

So reuse here is *partial but substantial*: **keep** the per-command bodies and
the reference `Model`; **rewrap** the orchestration — replace slice-threading with
`Variables<T>` and the manual variant gating with `#[rule]`/preconditions.

Do not promise "all 35 impls reuse for free." Track A is free; Track B keeps the
bodies and the model and re-expresses the command loop. Still far less than a
rewrite — and it upgrades the ad-hoc `Action::arbitrary_from` driver into a
proper shrinkable, model-checked command sequence.

## Shrinking quality: a gradient, and it favors our actual use case

Routing a `rand`-style generator through `HegelRandom` preserves shrinking, but
quality varies by how each draw maps to output:

- **Shrinks well (toward minimal):** enum-tag selection via
  `rng.random_range(0..=N)` (e.g. `Temporal`, `AttributeConfig`, `NumericValue`
  Exact-vs-Range), `Option` presence via `maybe(...)`/`random_bool`, small counts
  and indices, `sampled_from`-style picks. These shrink toward variant 0 / `None`
  / smaller — the structural minimization we want.
- **Shrinks to noise (acceptable):** `Uuid` (16 raw bytes), `FractionalIndex`
  (random byte vector), and especially the **Normal-distribution sampling** in
  `DateTime<Utc>` and `gen_random_exercise_duration_ms` — the map from underlying
  bytes to output is nonlinear, so shrinking the bytes won't cleanly drive the
  datetime toward `time_mean`. You get a valid-but-arbitrary value, not a tidy
  minimal one.

Crucially, for the **round-trip tests** that motivated this (see below), the
high-value counterexamples are *structural* — which `Temporal` variant, which
`AttributeConfig`, which `Option`s are `Some` — and those are exactly the draws
that shrink well. A noisy UUID in the minimal example is irrelevant to debugging a
`Row` decode failure. So the shrinking degradation is real but lands on the axes
we don't care about. Scope the claim to this use case rather than selling it as a
general win.

## Per-draw cost — answered from source, with a real mitigation

The earlier draft left per-draw cost as an open question. Reading the source
settles the *mechanism*; only the absolute numbers still want a measurement.

`draw`/`do_draw` for a primitive generator routes through
`generate_raw(tc, schema)` → `tc.with_data_source(|ds| ds.generate(schema))`
(`src/test_case.rs`). So every `integers()`/`binary()` draw — i.e. every
`try_next_u32`/`try_fill_bytes` in the RNG bridge — is one `DataSource::generate`
call. There are two backends, and they differ enormously:

- **Server backend (default).** `src/server/session.rs` spawns the hegel-core
  Python process with **piped stdin/stdout**; `src/server/data_source.rs::generate`
  does `send_request("generate", …)` — a **synchronous CBOR request/response
  round-trip over the pipe, per draw.** So one `Entry` (several UUIDs via
  `fill_bytes` + Normal-sampled datetimes + assorted `random_bool`/`random_range`)
  is ~10–20 blocking IPC round-trips to Python. Best shrinking and feature
  completeness; slowest per draw.
- **Native backend (`--features native`).** `src/native/data_source.rs::generate`
  is `self.with_ntc(|ntc| schema::interpret_schema(ntc, schema))` — a **mutex lock
  + in-process schema interpretation, no IPC, no Python.** Per the README, native
  is "strictly worse than the Python-based implementation, across every axis
  *except performance*," and is still incomplete. So native is the fast,
  IPC-free path at the cost of shrink quality/features.

This is the real escape hatch, and it lands well for our use case. Our
`arbitrary` impls fan a single value out into *dozens* of fine-grained draws — the
exact workload that punishes per-draw IPC. For the high-volume **round-trip
tests**, we already established (see Shrinking section) that the shrink targets we
care about are *structural* and that UUID/datetime draws shrink to noise either
way — so the native backend's weaker shrinking costs us little there. Pairing
reuse with `--features native` for the round-trip suite is therefore attractive;
reserve the server backend for the lower-volume, structurally-rich **Track B**
stateful tests where shrink quality earns its IPC.

Still worth a quick measurement before converting en masse (numbers, not
mechanism):

> Spike (~30 lines): generate ~1000 `Entry`s through the `HegelRandom` bridge on
> (a) the server backend and (b) `--features native`, vs `ChaCha8Rng`.
> - If native is within a small factor of `ChaCha8Rng`: use native for the
>   round-trip suite broadly.
> - If even native is too slow for some hot type: hand-write a native
>   `Generator` for just that type (UUID/datetime), keep the bridge elsewhere.

The architecture doesn't hinge on the result anymore — the native backend already
removes the IPC cliff. The spike just picks the backend per suite.

## Duplication analysis — are we overestimating the repetition?

No — if anything the *raw LOC* understates the cost. Current surface:

- **35 `impl` blocks**, ~480 lines of generation logic across `generation/`
  (`actions.rs` ~290, `attribute.rs` ~240, `entry.rs` ~200, `lib.rs` ~90 of
  generation, `activity.rs` ~35).
- Several encode **non-trivial domain invariants**, not boilerplate:
  owner-matching for `Value`/`AttachValue`/`DeleteAttributeValue`, the
  legal-action gating in `Action::arbitrary_from`, fractional-index insertion
  positions, config-consistent value generation in `UpdateAttribute`.

If we could not reuse and rewrote everything natively for hegel:

- We'd reproduce all ~480 lines *and their invariants* in hegel's idiom, then
- **keep the `rand` versions anyway**, because DST and dev-seeds can't run on a
  Hypothesis-server-backed engine (they need millions of deterministic, offline,
  seed-reproducible steps with no `uv` subprocess in the loop).

So no-reuse means maintaining **two parallel generator hierarchies in lockstep** —
every future invariant change (a new `Action` variant, a new `AttributeConfig`)
has to be made twice and kept consistent. That's the true cost, and it's much
worse than "write it once more."

The bridge collapses that to: **one source of truth (`Arbitrary<R: Rng>`) + a
~20-line shim + a partial rewrap of the stateful command loop.** That is the
headline answer to "am I overestimating the duplication": the duplication is real
and compounding, and the bridge avoids essentially all of it.

## Concrete migration targets

Two existing tests are ideal first conversions:

1. **`gv-sql/tests/rows_round_trip.rs`** — today this hand-writes `sample_user()`,
   `sample_activity()`, `sample_entry_with_position()`, etc. with fixed field
   values, plus a manual `cases = [...]` array enumerating `Temporal` variants.
   This is the textbook "would be more robust as PBT" case: replace the hand-rolled
   samples with `tc.draw(randoms())` + `Entry::arbitrary` / `Attribute::arbitrary`
   / `Value::arbitrary_from`, asserting `core → Row → core` identity. PBT explores
   the variant/Option cross-product the hard-coded cases miss.

2. **`gv-sql/tests/entry_round_trip_sqlite.rs`** — already does
   `Entry::arbitrary(&mut rng, &ctx)` with a seeded `ChaCha8Rng` and a manual
   `for _ in 0..N` loop. Converting it is nearly mechanical: swap the seeded
   `ChaCha8Rng` for `tc.draw(randoms())` (via the shim) and drop the manual loop —
   `#[hegel::test]` provides iteration, shrinking, and a failing-case corpus the
   fixed seed `0xb0c0_dabad7e5` does not. This one is the cleanest proof that the
   bridge reuses an existing `arbitrary` impl with zero body changes.

Both are pure round-trip identity properties (`decode(encode(x)) == x`), the
highest-confidence PBT shape.

## Recommended sequence

1. **Write the version shim** (`rand` 0.10 `HegelRandom` → `rand` 0.9 `RngCore`),
   dev/test-only. ~20 lines, one file. This is the whole bridge.
2. **Convert `entry_round_trip_sqlite.rs`** as the reference Track-A migration
   (reuses `Entry::arbitrary` unchanged). Run it both with the default server
   backend and with `--features native` — that *is* the per-draw-cost spike, now
   doing real work instead of a throwaway.
3. **Convert `rows_round_trip.rs`** to replace hand-written samples with reused
   `arbitrary`/`arbitrary_from` impls.
4. **Pick the backend per suite** from step 2's timing: native for the
   high-volume round-trip suite (weak shrinking is fine there), server where
   shrink quality matters.
5. **Prototype one Track-B stateful test**: turn `Action::arbitrary_from` into a
   `#[hegel::state_machine]` with `Variables<Entry>`/`Variables<Attribute>` pools
   and `generation::model::Model` as the reference model, to validate the
   partial-reuse pattern before converting the rest.
6. Decide on the workspace `rand` 0.10 bump separately, on its own merits; the
   shim makes it non-blocking.

Cross-reference: `docs/properties.md` (PBT strategy).
