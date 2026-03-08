

Features to add:
- Sets
- Attribute variants: Length, Text.
- Categories
- Permissions

Properties to test:
- Forest (acyclic)
- No dangling parent pointers
- Undo/redo roundtrip

Actions to add:
- UpdateEntryAttribute
- CreateEntryFromTemplate
    - Or should the client do the look-up, and just CreateEntry?
- CreateActivityTemplate
    - Or should each activity automatically have a template?

- [ ] Add a Permissions placeholder: `Permissions:can_write(&mut *tx, entry, actor)`.
    - For reads: assume only data the user is permittd to read is synced.

- [ ] Introduce a Measure trait
      - Move `defined_units` to trait.
      - Consider using `uom` crate for conversions.
      - API
        - Remove a unit: redistribute quantity over remaining units.
        - Add a unit: redistribute quantity over set of units.
        - Get the normalized quantity (for indexing).
      - Internallly, convert between quantity and a set of units.
        - Distributed quantity -> units.
        - Sum units -> quantity.


- [ ] Deterministically order attributes in entry_view.

- [ ] Change generation to not panic on empty parameter sets (entries, attributes, etc).
    
- [ ] Consider using an Edge trait to have a generic interface to the Entry forest.

- [ ] Consider adding a *Row type for all models.
    - Could implement Arbitrary for both, e.g. Entry and EntryRow. The Entry is always valid,
    the EntryRow may violate domain constraints.
    - If Model == RowModel, can just newtype for consistency, or not implement the row type.

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