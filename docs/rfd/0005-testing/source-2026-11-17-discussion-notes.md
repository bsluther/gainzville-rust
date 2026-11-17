# Source: discussion notes, 2026-11-13 to 2026-11-17

Notes from a discussion between the author and an agent, mostly voice-narrated. Kept in the order it
happened. The author's points are marked; the rest is the agent's contribution as it stood after the
author's pushback. The Turso and TigerBeetle sections are reports from research agents that read a
shallow clone of each repository. Counts are grep-based and approximate, and nothing was built or
run. antithesis.com was outside the session's network allowlist, so that part rests on search
snippets.

---

## 1. Objective

- **Author:** the goal is DST and PBT, with a lot of groundwork already in place. Along the way,
  agents added many unit tests. They give some coverage but feel like technical debt: an
  amalgamation, some of it checking things simulation covers or should cover. Tests should verify
  *and express* correctness: properties written down where you can read them. Duplicating a rule
  between DST and unit tests is not defense in depth; it is interest to pay. Unit tests aren't
  banned, but doing both is a mistake, and the codebase has drifted that way.
- **Agent:** agreed, with one refinement: "covered by DST" has to be demonstrated. A property can
  exist while the generator never reaches the state where it matters. Deletion test: break the code
  a unit test guards and see whether the simulation fails. Unit tests keep a narrow role for what the
  simulation can't reach by design.
- **Author:** don't compare against the simulation as it is now. Ask whether the rule a unit test
  checks is easy to enforce by adding an assertion, a property, or a workload.

## 2. Turso (research agent, clone at 072456f, 2026-11-13)

Verdict: "lots of ordinary tests alongside DST", not "unit tests only where the simulator can't
reach". Hand-written tests dominate by count and are the required form for regression pinning.

- **Layers:** a sequential simulator (`testing/simulator`, 18 variants of one `Property` enum, with
  property, differential-vs-SQLite and doublecheck oracles); a concurrent simulator "Whopper" (8
  `Property` impls); a differential fuzzer against SQLite; other fuzzing (`tests/fuzz`, cargo-fuzz
  targets, SQLancer); Antithesis (13 templates, about 650 `turso_assert*` macros in `core`); SQL
  suites (about 16,000 `.sqltest` cases and about 41,700 TCL test calls).
- **Counts:** about 6,770 test attributes. `core` has 2,661 inline tests (mvcc 600, storage 379,
  vdbe 358). `tests/` has 1,396.
- **What unit tests cover:** some pure components, but the densest (MVCC, WAL, btree) are scenario
  tests of checkpoint, recovery and rollback, which is what Whopper and Antithesis target. 44 of 119
  `core` files with tests open a real database.
- **Stated philosophy:** "Every functional change needs a test"; a reproducer "must serve as a
  regression test". Regression pinning is by hand-written test ("Regression test for simulator seed
  867"), not saved seeds.
- **Admitted limits:** `testing/simulator/COVERAGE.md` marks most of SQL "No" or "Partial", and says
  a shadow model of all SQL is "virtually impossible".
- **Not checked:** blog posts and talks; whether the overlap is policy or accretion.

Agent's reading: Turso's overlap is partly forced by a domain too big to model. GV's domain is small
enough that a stricter stance is achievable.

## 3. Model-based versus assertion-based

- **Author:** added the in-memory model partly to mimic Turso, with doubts about maintaining a fake
  world. Impression that TigerBeetle leans on a very large number of assertions exercised by the
  simulator.
- **Agent:** reframed as three kinds of check: assertions in code (local facts), state invariants (a
  predicate over one state, no model needed), and a model or oracle (only needed for "did this
  action do the right thing").

## 4. TigerBeetle (research agent, clone at a2a3c70, 2026-11-10)

Verdict: the assertion picture is right, with two corrections. Ledger business rules are result
codes, not assertions. And it is a hybrid: test-side code is about a third the size of the core.

- **Assertion practice:** `docs/TIGER_STYLE.md` requires an average of at least two assertions per
  function, asserting positive and negative space, with pair assertions. A core subset (95k lines)
  has 6,468 asserts over about 2,843 functions: about 2.3 per function. Assertions stay on in
  production (ReleaseSafe). Costly ones sit behind `constants.verify`.
- **What they express:** preconditions, postconditions, local invariants. A few small whole-structure
  validators (`Replica.invariants`, run after every tick). Business rules are plain returns such as
  `.exceeds_credits`. The balance invariant is asserted in the auditor, not production code.
- **Checkers and models:** about 32k test-side lines against 95k core. `src/testing/cluster/` has
  state, storage, grid, manifest and journal checkers. `workload.zig` (2,281 lines) and `auditor.zig`
  (1,009) sit against 5,399 lines of state machine. The workload plans each transfer as success,
  failure or unknown and gives the auditor a set of allowed result codes; a transfer's id is a
  reversible permutation of its index, so its outcome can be re-derived. No reference implementation
  of the state machine. LSM fuzzers do use models.
- **Unit tests:** 562 `test` blocks. 47 are table-driven state-machine tests asserting exact result
  codes; 60 are scripted cluster scenarios; 16 standalone fuzzers.
- **Pinning:** no saved-seed corpus; seed plus git commit, and hard cases become scripted scenarios.

Agent's disagreement with the report: it said to keep GV's delta-replay model because it catches
wrong-but-consistent results. It doesn't, since it replays whatever the mutator produced.

- **Author:** does TigerBeetle actually prevent an over-credit transfer, or does only the auditor
  care?
- **Agent:** it prevents it. The distinction is who is at fault. An assertion means "our code is
  wrong", and theirs crash the replica. A user asking for an overdraft is a legitimate request whose
  answer is no, so it is a return value. Same split as GV's `Rejected` versus `InvariantViolation`.
  Each rule appears three times: a guard in production, the resulting invariant in the auditor, and
  the exact setup-to-code mapping in the example tables. The last lives in examples because a
  property stating it would re-implement the guard.

## 5. Pre- and postcondition checks

- **Author:** from another conversation (RFD 4): pull checks out of the mutators into logic that
  only uses a query executor; cache query results for one mutation; possibly run the action, check
  conditions against the resulting world, and roll back in the same transaction.

## 6. Generating invalid actions, and the two directions

- **Author:** generate an action known to be invalid and check for the expected rejection. The hard
  part is generating one reliably.
- **Agent:** start from a valid action and break exactly one thing, using the model for ingredients.
  Complications: breaking one rule can break a second (expect a set of reasons); a recipe may be
  impossible in the current state (needs counters); the valid side is harder, since promising
  success means knowing every precondition.
- **Agent:** the two directions leave different evidence. A loose guard corrupts state, which can be
  inspected afterwards. A strict guard only withholds a result, which can't be seen unless predicted.
  Caveat: a guard whose rule leaves nothing in stored state needs prediction both ways.
- **Author:** this is Antithesis's "sometimes" property. Probably unreasonable to implement soon.
- **Agent:** the hard half is their guided exploration. The easy half is a named counter per
  condition and a check that none is zero at the end of a run. That would have caught the two actions
  that are never generated.

## 7. Walkthrough

### `test_move_entry_disallows_cycles`

Creates two template sequences and tries to move A under A, asserting a `Precondition` rejection. A
is a root template, so "root template entry may not change position" fires before the cycle check;
the test never reaches it. Belongs as a "no cycles" state invariant plus a generator recipe.

- **Author:** agreed, the ideal fuzzing case: build chaotic trees and look for cycles. But how do we
  know the checker is right? Perhaps a property test on the checker in isolation.
- **Agent:** yes, with inputs built by construction. Clean: each entry picks its parent from earlier
  entries. Planted: point an entry at its own descendant. Separately, sabotage the guard once to show
  the simulation reaches the case.

### `test_convert_to_sets_rejections`

Four rejections, each asserting only `is_err()`, and one success. Template root and
sets-member-activity fold into existing invariants. The id collision would surface as a database
error but is never generated. The off-timeline root case looked like a transition rule, because
`create_entry` accepts such a root.

- **Author:** envisioned as an invariant for log entries; templates must have no start or end.
  Considered relaxing it but never found a good UX. Probably a gap.
- **Agent:** `create_entry` validates the template side but lacks the log-root check that the other
  three actions have. Writing the rule down as a property is what exposed it.

### `test_duplicate_entry_exact_copy_after_source`

Hand-built three-level tree with one value; duplicates a child and checks placement, fields, and the
re-keyed value. A "did the action do the right thing" test: invariants would pass if duplicate
dropped the values.

- **Agent (first take):** a per-action postcondition, compared ignoring ids. Checking is simpler
  than doing.
- **Author:** the check needs the pre-mutation tree to say the forest wasn't otherwise modified.
  That questions whether a predicate-style function fits every condition: the more the arguments
  leak the check, the weaker the abstraction. The check is coupled to the action, so extraction may
  be artificial, though helpers like Forest methods can hold common logic.
- **Agent:** a state invariant is a predicate over one state; a postcondition is a relation over
  before, action, after. And a GV mutation is an explicit delta list, so "nothing else changed" can
  be checked on the mutation: only inserts, all in the new subtree.

The author asked for three versions in code: the mutator as is; the check in a separate function
called by the mutator; the check inline with generic helpers. Sketches (uncompiled) used a
`Forest::shape` (a subtree with identity erased), `Forest::next_sibling`, a `Mutation::only_inserts`,
and an `ensure` helper that returns `InvariantViolation`.

Extracted version, about 35 lines:

```rust
pub async fn check_duplicate_entry(
    executor: &mut impl DomainQueryExecutor,
    action: &DuplicateEntry,
    mx: &Mutation,
) -> Result<()> {
    // Re-read the source (free with a caching executor).
    // 1. mx.only_inserts() is Some: nothing existing was touched.
    // 2. The inserted entries form one tree whose shape equals the source's.
    // 3. No inserted id appears in the source subtree.
    // 4. Same parent as the source, frac_index after the source and before
    //    the source's next sibling; or both at the root.
}
```

Inline version, about 20 lines after `duplicate_subtree`: the shape comparison and the placement
comparison, using the mutator's own locals.

- **Author:** `only_inserts` is suspicious, specific to this case. How is "the same signature" better
  than inline? "Catches a mutator that loaded the wrong subtree" is a reach, since the check could
  load the wrong one too. And is a vacuous check any better for being in another function?
- **Agent:** conceded all three. What matters is whether the check computes its answer differently
  from the code it checks. Only two parts have substance, and both point below the mutator: the shape
  comparison tests `duplicate_subtree`, and adjacency tests `position_between`. The mutator is mostly
  glue, which is why a mutator-level postcondition felt awkward. Revised lean: property tests on the
  two pure functions, plus a small check in the harness after the deltas are applied (read the
  source's next sibling back from the database and compare shapes).
- **Author:** the delta check could still be a good assertion, as a stated contract. And a pattern:
  PBT looks best for domain logic like `duplicate_subtree`, where the state space can be crafted for
  the function's properties. DST excels at distributed systems, and sync is one; outside sync it
  still gives confidence by exploring the whole system and catches related bugs, such as ids not
  being replaced showing up as a duplicate key.
- **Agent:** agreed. A pure function should hold for any input, so its generator needn't know the
  domain rules. The simulation's value outside sync is sequences of actions.
- **Author:** careful to build a principled system and not accumulate special cases like the
  after-apply check. To sort out later.

### `numeric_validation`

Example table over `validate_value`. Not for the simulation, because the invariant "stored values
conform to their attribute" would call the same function. Converts to property tests beside the
existing mass ones (valid by construction passes; break one thing and it fails). Drawing values as
whole hundredths produces the 0.29 case naturally.

### Row round-trips

17 hand-built `core → Row → core` examples and 6 column round-trips through in-memory SQLite. The
same rule is checked four ways: those examples, the existing Hegel `entry_round_trip`, a seeded test
in `sqlite_tests.rs`, and the simulation's model-versus-snapshot comparison (Postgres only). Belongs
as one property per model plus a simulation run on SQLite.

- **Agent:** the property is only as good as the generator; the examples list all seven `Temporal`
  variants explicitly. Confirm reach with sometimes counters before deleting. Round-trips say nothing
  about rows written by older versions.
- **Author:** add it, and note the sometimes-assertion use case.
