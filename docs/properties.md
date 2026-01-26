

Scratch list of properties to test for.

### Serializaton/Deserialization
All database rows must be capable of being parsed successfully into domain models.

### Entries
Entries must be acyclic.
Template and log entry trees must be disjoint.

### Undo/Redo
Round-trip: performing some actions and then undoing them results in the original state.

### Authorization
Actors can only change data they are permitted to change.
Actors can only read data they are permitted to read.

### Sync