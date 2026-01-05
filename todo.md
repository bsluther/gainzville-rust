- [ ] Refactor `Entry` to use `Position`.

- [ ] Refactor `ArbitraryFrom` impls to take slices rather an vec refs.

- [ ] Log mutations and implement undo/redo.

- [ ] Use seeded rng for determinism (e.g. for generating Uuid's).

- [ ] Consider using a SortedFracitionalIndices type to avoid having to defensively copy/sort
lists of fractional indices.

- [ ] There is going to be an issue when I want to run a test which runs multiple actions. The
execute_action function on the controllers returns a tx but doesn't take one as an argument. Will
need to pass the tx as an argument to be able to rollback the transaction, which allows for
parallel tests because it isolated each test from each other through the transaction boundary and
never commits.
    - Is it as easy creating an alternate constructor which takes tx as an arg instead of pool?
- [ ] Use `garde` to validate types like Email, Username, etc.

- [ ] Maybe: write macros to do one or both of
    - [ ] Create model updater.
    - [ ] Create model apply_delta functions.