
Features to add:
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

- [ ] Consider refactoring `Position` to have a `Root` variant (rather than `Option<Position>`).

- [ ] Reads currently assume a global scope: need to parameterized by actor.

- [ ] Consider wrapping actions in a struct that provides actor_id.
    - Perhaps the same for reads.

- [ ] Log mutations and implement undo/redo.

- [ ] Use seeded rng for determinism in application code (e.g. for generating Uuid's).

- [ ] Implement Delete* actions.
    - Should I use tombstones for soft-deletes? If I log all mutations/deltas, then I techincally
    don't need soft-deletes, since I retain the information needed to reconstruct. But it could be
    preferrable to use soft-deletes for other reasons. Not sure.

- [ ] Consider using a SortedFractionalIndices type to avoid having to defensively copy/sort
lists of fractional indices.

- [ ] Use `garde` to validate types like Email, Username, etc.