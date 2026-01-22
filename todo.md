Decisions:
- Do users duplicate activites added from another library?
    - Always consider the sequence case first, it's more complcated.
    - Should profiles be separate from activites? Again, consider the sequence case...

Features to add:
- Time
    - Attribute or built-in?
- Sets
- Attributes
- Categories
- Permissions

Properties to test:
- Forest (acyclic)
- No dangling parent pointers
- Undo/redo roundtrip

Actions to add:
    CreateEntryFromTemplate
    - Or should the client do the look-up, and just CreateEntry?
    CreateActivityTemplate
    - Or should each activity automatically have a template?
    CreateAttribute
    AddValueToEntry
    UpdateValue

- [ ] Add initializers to model types, migrate to using those.

- [ ] Consider refactoring repo `context`.
    - Take transaction as an argument?
    - Can I wrap a Sqlite and Postgres transaction in an enum?

- [ ] Consider refactoring `Position` to have a `Root` variant (rather than `Option<Position>`).

- [ ] Consider wrapping actions in a struct that provides actor_id.

- [ ] Refactor `ArbitraryFrom` impls to take slices rather an vec refs.

- [ ] Log mutations and implement undo/redo.

- [ ] Use seeded rng for determinism (e.g. for generating Uuid's).

- [ ] Implement Delete* actions.
    - Should I use tombstones for soft-deletes? If I log all mutations/deltas, then I techincally
    don't need soft-deletes, since I retain the information needed to reconstruct. But it could be
    preferrable to use soft-deletes for other reasons. Not sure.

- [ ] Consider using a SortedFractionalIndices type to avoid having to defensively copy/sort
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