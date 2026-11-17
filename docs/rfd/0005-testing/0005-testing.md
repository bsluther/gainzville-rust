# RFD 0005: Testing strategy

- State: discussion
- Linear: [GV-78](https://linear.app/gainzville/issue/GV-78)

## Question

How should correctness checking be divided between deterministic simulation (DST), property-based
tests (PBT), assertions in production code, and example-based unit tests, so that each rule of the
system is stated once, readably, and enforced where that is cheapest?

Inputs: the [discussion notes](./source-2026-11-17-discussion-notes.md), which hold the prior-art
research on Turso and TigerBeetle, the test-by-test walkthrough, and the code sketches.

Related: [RFD 4](../0004-actions/0004-actions.md) (how mutators organize their checks),
[properties](../../properties.md) (the working list of properties),
[generation architecture](../../generation-architecture.md).

## Objective

The author's, stated at the start of the discussion:

- Tests exist to **verify and express** the correctness of the system. A reader should be able to
  find the rules written down and see how they are enforced.
- Checking the same rule in both DST and a unit test is not defense in depth. It is duplication that
  has to be maintained, and example tests are a worse statement of the rule.
- Push into DST and PBT whatever can be checked there, and do not repeat it in unit tests. Unit
  tests are allowed, but "do both" is not the default.
- The comparison is not against the simulation as it exists today. Part of the work is growing it:
  an assertion here, a property there, a new workload.

## Where things stand (2026-11-13)

About 115 test functions. The largest groups: SQLite integration tests (23, one 2,270-line file),
live queries in `gv-client` (19), row and column round-trips in `gv-sql` (23), the attribute model
in `gv-core` (12).

The simulation is one loop in `integration-tests/tests/postgres_tests.rs`: 1,000 random actions,
and after each one the in-memory model is compared with a full snapshot of the database. It fails on
a database error or an `InvariantViolation`.

- **The model is not an independent oracle.** It replays the mutation the server returned, so the
  comparison shows that the database applied the deltas faithfully. It cannot show that the mutator
  produced the right deltas.
- **No property from `properties.md` is asserted in the loop.** One is written on the model
  (`all_activities_have_one_template_root`) and nothing calls it.
- **It runs against Postgres only.** SQLite, the offline-first target, has no simulation run.
- **Two of sixteen actions are never generated:** `DeleteEntryRecursive` and `UpdateAttributeValue`.

Separately there are four real Hegel property tests (three on mass conversion, one entry row
round-trip) and one empty stub.

So there is little true duplication today. The unit tests carry nearly all of the rule checking, and
this is a migration more than a deletion.

In `mutators.rs` (about 1,500 lines) there are roughly 70 rejection sites and only 4
`InvariantViolation` sites. The guards are well populated; "this should be impossible" checks are
almost absent.

## Prior art

Details and counts are in the [discussion notes](./source-2026-11-17-discussion-notes.md). Both
reports came from an agent reading a shallow clone; nothing was built or run.

- **Turso** does a lot of both. About 6,800 test functions against 18 simulator properties and 8 in
  the concurrent simulator. The densest unit tests are scenario tests of the same behaviour the
  simulators target. Bugs the simulator finds are pinned as hand-written tests, not saved seeds.
  Their own coverage doc says most of SQL is unmodelled, so part of the overlap is forced by the size
  of the domain. GV's domain is small enough that this pressure is much weaker.
- **TigerBeetle** is a hybrid, not "assertions instead of models". Assertion density is real (about
  2.3 per function in the core, on in production). But the assertions cover preconditions,
  postconditions and local consistency. **Business rules are result codes**, checked from outside by
  an auditor in the simulator and by about 47 table-driven example tests. The auditor stays small
  because the workload decides in advance whether each request should succeed, and passes along the
  set of allowed result codes.
- **Antithesis** "sometimes" assertions: a condition must hold at least once across the whole test,
  or the property fails. Mainly used to show code is reached. The site was outside the session's
  network allowlist, so this rests on search snippets.

## Lean (2026-11-17)

Not decided. This is where the discussion had got to.

### Three kinds of check

- **Assertions in the code:** local facts the function knows at that moment. In GV this is
  `InvariantViolation`. An assertion can also be worth having as a stated contract ("this mutation
  only inserts") even where it would be vacuous as a test.
- **State invariants:** a predicate over one state, checked after every simulated action. No model
  needed. Most of `properties.md` is this kind.
- **Postconditions:** a relation between the state before, the action, and the state after. These
  are inherently per action.

### Where a rule's check lives depends on where its logic lives

- **Logic in a pure function:** property-test that function directly. Examples: `duplicate_subtree`,
  attribute validation, `Forest::position_between`, a cycle checker. Such a function should hold for
  any input, legal or not, so its generator doesn't need the domain rules.
- **A rule about stored state:** an invariant checked in the simulation.
- **Wiring between the two:** the simulation catches much of it as a side effect (a duplicate key
  becomes a database error). For actions like duplicate, a small check in the harness after the
  deltas are applied reads the result back through real queries.

A check is worth writing when it computes its answer differently from the code it checks. Where it
is written doesn't change that. Extracting a tautology into its own function only hides it.

### Guards fail in two directions

- **Too loose** (a bad action is accepted): the state is now illegal, and the invariant check finds
  it. The generator needs no prediction, only to reach the case.
- **Too strict** (a good action is rejected): nothing changes and every invariant passes. Only a
  prediction made beforehand ("this is valid, so it must succeed") reveals it.

So deliberately invalid actions mostly serve reach, and prediction earns its keep on the valid side.
Recipes for invalid actions start from a valid action and break exactly one thing. Exception: a
guard whose rule leaves nothing in stored state (permissions, transition rules) needs prediction in
both directions.

### Roots of trust

A checker the simulation relies on cannot be tested by the simulation. The cycle checker and
`validate_value` need their own property tests, with inputs built so the answer is known by
construction (a forest where parents are always earlier entries is clean; point an entry at its own
descendant and it is not).

### Reach has to be shown

- **Sabotage:** when adding a rule, remove its guard and confirm the simulation fails. If it stays
  green, the generator isn't reaching that case.
- **Sometimes counters:** a named counter per condition, bumped when it holds, with a check at the
  end of a run that none is zero. Candidates: every action type succeeded at least once, every
  rejection reason appeared at least once, every enum variant was generated (all seven `Temporal`
  variants, every attribute and value kind). This is the cheap half of what Antithesis does. Random
  seeds can miss a rare condition by luck, so thresholds need to be loose or judged over several
  seeds.

### What stays as examples

- Exact "which rejection reason" tables, if the reason matters to callers.
- Rows written by older versions of the app (data versioning, [RFD 2](../0002-dogfooding/0002-dogfooding.md)).
- The deletion test for any unit test: break the code it guards and see whether DST or a property
  fails. If so, it is a duplicate.

## Walkthrough

Five of a planned seven sample tests. Reasoning for each is in the discussion notes.

| Test | What it is | Where it belongs |
|------|-----------|------------------|
| `test_move_entry_disallows_cycles` | Guard with a matching state invariant | "No cycles" invariant in the simulation, a generator recipe that picks a descendant as the new parent, and a property test on the cycle checker. Unit test goes. |
| `test_convert_to_sets_rejections` | Four rejections and one success in one test | Two fold into existing invariants. The id collision needs a recipe and a counter. The root-time rule turned out to be an intended invariant with a gap. The success case needs valid-by-construction generation. |
| `test_duplicate_entry_exact_copy_after_source` | "Did the action do the right thing" | Property tests on `duplicate_subtree` and `position_between`, plus a small after-apply check in the harness. |
| `numeric_validation`, `numeric_precision_validation` | Example table over a pure function | Property tests beside the existing mass ones. Not in the simulation, because the simulation's invariant would call the same function. |
| Row round-trips (`gv-sql`, 17 pure and 6 through SQLite) | `decode(encode(x)) == x` | One property per model using the existing generators, plus a simulation run on SQLite. About two dozen tests collapse into five. Sometimes counters confirm the generators reach every variant before the examples are deleted. |

Still to do: an "only this field changed" test (`test_update_attribute_value_clears_only_target_field`)
and a live-query timing test from `client/live.rs`.

## Findings

Things noticed in the code along the way. All from reading; none confirmed by running.

- `test_move_entry_disallows_cycles` never reaches the cycle check. Its entry is a root template, so
  the earlier guard "root template entry may not change position" fires first, and the test accepts
  any `Precondition`. Deleting the cycle check would likely leave it green. It also only tries an
  entry as its own parent.
- `create_entry` does not enforce "a log entry at the root has a start or an end". `move_entry`,
  `create_entry_from_activity` and `convert_to_sets` do. The author intends this as an invariant for
  log entries (templates must have neither), so this is probably a gap. The model doc doesn't state
  the rule.
- `create_entry` skips its parent checks when the parent id matches no entry. Whether that yields a
  dangling parent or a database error depends on the foreign key. Not checked.
- The rejection tests in `sqlite_tests.rs` mostly assert only `is_err()`, so a different guard or a
  database failure would pass them.
- The duplicate rule looks tested twice among the unit tests: the SQLite integration test and pure
  tests in `core/src/instantiation.rs` (judged from names only).
- Generators: `ConvertToSets` mints a fresh sequence id every time, so a collision is never tried.
  `MoveEntry` picks a random position, so a cycle is attempted only by luck.
- `property_tests.rs` has an empty `find_descendants` stub.

## Open

- **A principled form for per-action checks.** The after-apply check for duplicate should not become
  one of an accumulating pile of special cases. What is the general shape, and how does it relate to
  the checks design in RFD 4 (checks as functions of a query executor, a per-run cache, invariants
  against the post-change state in the write transaction)? Note that a mutator returns deltas without
  applying them, so a check inside a mutator sees only what was read plus the deltas produced.
- **Valid-by-construction generation.** How much precondition knowledge the generator needs before
  it is duplicating the guards.
- **Transition rules** that leave no trace in stored state: predicted outcome, a before-and-after
  check, or promote them to state invariants.
- **Deltas as the "nothing else changed" check.** A mutation is an explicit list of row deltas, so
  the footprint of an action can be checked on the mutation itself, with the existing model
  comparison confirming the database applied it.
- **Assertions:** always on, or only under simulation? An `InvariantViolation` in production aborts
  the action.
- **Pinning failures:** saved seeds, seed plus commit, or hand-written regression tests.
- **Live queries.** Their tests are about timing and ordering, which the action simulation doesn't
  touch. Possibly a separate simulation with a deterministic scheduler.
- **Sync** is where DST matters most, and isn't built yet.
