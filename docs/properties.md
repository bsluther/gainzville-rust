

Working list of properties to test for.

### Serializaton/Deserialization
All database rows must be capable of being parsed successfully into domain models.

### Entries
Entries must be acyclic.
Template and log entry trees must be disjoint.
The activity associated with an entry has the same owner as the entry.
- This checks that Copy-On-Add is being used.

### Attributes
All values have an associated attribute.
The attribute associated with a value has the same owner as the value.
The value assigned to an entry has the same owner as the entry.
- In the current design this is trivially true: the owner of a value is defined as whoever owns the
entry.
For an attribute A, all values, default values, and bounds (min/max) of A conform to the schema
defined by A.
- Example: if a NumericAttribute is configured to use integers, then the min and max bounds are
integer values (stored as f64's, currently).

### Undo/Redo
Round-trip: performing some actions and then undoing them results in the original state.

### Authorization
Actors can only change data they are permitted to change.
Actors can only read data they are permitted to read.

### Sync