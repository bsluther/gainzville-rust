# Delta Updates Architecture Discussion

## Problem Context

Gainzville uses a Delta-based system for tracking database changes to support offline-first sync. The system needs to represent partial updates to entities where:
- Some fields may be nullable in the database
- Only certain fields are mutable (e.g., Actor has no mutable fields)
- Changes need to be tracked for sync and conflict resolution
- Type safety should prevent invalid states

## Evolution of the Approach

### Initial Approach: Double Option Pattern

**Problem**: How to represent field updates that distinguish between:
1. Field unchanged
2. Field set to null
3. Field set to a value

**Initial solution**: `Option<Option<T>>`

```rust
pub struct ActivityPatch {
    owner_id: Option<Uuid>,
    source_activity_id: Option<Option<Uuid>>,  // Double-wrapped for nullable field
    name: Option<String>,
    description: Option<Option<String>>,       // Double-wrapped for nullable field
}
```

**Downsides**:
- Confusing semantics: `None` vs `Some(None)` vs `Some(Some(value))`
- No distinction between nullable and non-nullable fields at type level
- Easy to misuse
- Unclear intent to readers

### Alternative 1: Custom Enum Per Field

```rust
pub enum Patch<T> {
    Unchanged,
    SetNull,
    SetValue(T),
}
```

**Downsides**:
- Allows `SetNull` on non-nullable fields (type safety issue)
- Conflates nullable and non-nullable field updates

### Alternative 2: Two Separate Enums (Interim Solution)

```rust
pub enum NonNullablePatch<T> {
    Unchanged,
    NewValue(T),
}

pub enum NullablePatch<T> {
    Unchanged,
    SetNull,
    NewValue(T),
}

pub struct UserPatch {
    username: NonNullablePatch<Username>,
    email: NonNullablePatch<Email>,
}
```

**Pros**:
- Type-safe: can't set non-nullable fields to null
- Clear intent

**Downsides**:
- Can't enforce that `M::Patch` contains only field patch types (but acceptable for internal API)
- Naming confusion with Delta also being a "change"

### Type Safety Issue: Uncorrelated Old/New States

**Problem discovered**: Nothing enforces field correspondence in Update deltas

```rust
Delta::Update {
    old: UserPatch { username: Set("alice"), email: Unchanged },
    new: UserPatch { username: Unchanged, email: Set("new@email.com") },
}
// Invalid: username changed in old but not new!
```

### Alternative: Paired Old/New Per Field

```rust
pub enum Change<T> {
    Unchanged,
    Changed { old: T, new: T },  // Old and new always paired
}

pub struct UserChangeset {
    username: Change<Username>,
    email: Change<Email>,
}

Delta::Update {
    id: Uuid,
    changeset: UserChangeset,
}
```

**Pros**:
- Type-safe: old/new always correspond per field
- Space-efficient: only store changed fields

**Downsides**:
- Can't reconstruct full entity state from changeset alone
- Insufficient for conflict resolution in sync system
- Need base entity + changeset to have complete picture

## Final Solution: Full Snapshots + Builder Pattern

### Design Goals

1. **Type Safety**: Only mutable fields can be changed (enforced at compile time)
2. **Conflict Resolution**: Full entity state available for validating changes
3. **Ergonomic API**: Chainable updates via builder pattern
4. **Per-Model Mutability**: Each model defines which fields are mutable
5. **Sync Support**: Complete state reconstruction from deltas
6. **Clear Intent**: Code clearly expresses what changed

### Architecture

**Delta stores full entity snapshots** (not just changed fields):

```rust
pub enum Delta<M: Model> {
    Insert {
        id: Uuid,
        entity: M,
    },
    Update {
        id: Uuid,
        old: M,      // Complete entity before change
        new: M,      // Complete entity after change
    },
    Delete {
        id: Uuid,
        old: M,      // Complete entity that was deleted
    },
}
```

**Builder pattern enforces mutability constraints**:

```rust
// Full entity with all fields
pub struct User {
    pub actor_id: Uuid,      // Immutable - set at creation
    pub username: Username,   // Mutable
    pub email: Email,         // Mutable
}

// Builder that only exposes mutable fields
pub struct UserUpdater {
    id: Uuid,
    old: User,
    new: User,
}

impl User {
    /// Start an update - consumes self to ensure you have current state
    pub fn updater(self) -> UserUpdater {
        UserUpdater {
            id: self.actor_id,
            old: self.clone(),
            new: self,
        }
    }
}

impl UserUpdater {
    // Only methods for mutable fields exist

    pub fn username(mut self, username: Username) -> Self {
        self.new.username = username;
        self
    }

    pub fn email(mut self, email: Email) -> Self {
        self.new.email = email;
        self
    }

    /// Build the Delta with full snapshots
    pub fn build(self) -> Delta<User> {
        Delta::Update {
            id: self.id,
            old: self.old,  // Full User snapshot
            new: self.new,  // Full User snapshot
        }
    }

    /// Only build if something actually changed
    pub fn build_if_changed(self) -> Option<Delta<User>> {
        if self.old.username != self.new.username
            || self.old.email != self.new.email {
            Some(self.build())
        } else {
            None
        }
    }
}
```

### Usage Example

```rust
// Chainable updates - only mutable fields can be changed
let delta = user
    .updater()
    .email(Email::parse("new@email.com")?)
    .username(Username::parse("newalice")?)
    .build();

// Attempting to change immutable field: COMPILE ERROR
// user.updater().actor_id(...) // ❌ Method doesn't exist

// Delta contains full User snapshots for conflict resolution:
// old: User { actor_id: uuid, username: "alice", email: "old@email.com" }
// new: User { actor_id: uuid, username: "newalice", email: "new@email.com" }
```

### Models with No Mutable Fields

For entities like Actor that have no mutable fields, either:

**Option 1: No updater method**
```rust
impl Actor {
    // No updater() method - attempting to update is a compile error
}
```

**Option 2: Updater with no update methods**
```rust
pub struct ActorUpdater {
    actor: Actor,
}

impl Actor {
    pub fn updater(self) -> ActorUpdater {
        ActorUpdater { actor: self }
    }
}

impl ActorUpdater {
    // No update methods

    pub fn build(self) -> Option<Delta<Actor>> {
        None  // Nothing can change
    }
}
```

### Changeset as a View Type

Changeset becomes a computed view for understanding what changed:

```rust
pub enum Change<T> {
    Unchanged,
    Changed { old: T, new: T },
}

pub struct UserChangeset {
    pub username: Change<Username>,
    pub email: Change<Email>,
}

impl User {
    /// Compute diff between two states
    pub fn diff(&self, other: &Self) -> UserChangeset {
        UserChangeset {
            username: if self.username != other.username {
                Change::Changed {
                    old: self.username.clone(),
                    new: other.username.clone(),
                }
            } else {
                Change::Unchanged
            },
            email: if self.email != other.email {
                Change::Changed {
                    old: self.email.clone(),
                    new: other.email.clone(),
                }
            } else {
                Change::Unchanged
            },
        }
    }
}

impl Delta<User> {
    /// Compute what fields actually changed
    pub fn changed_fields(&self) -> Option<UserChangeset> {
        match self {
            Delta::Update { old, new, .. } => Some(old.diff(new)),
            _ => None,
        }
    }
}
```

### Conflict Resolution

With full snapshots, conflict detection is straightforward:

```rust
impl Delta<User> {
    /// Check if this delta can be applied to current database state
    pub fn is_valid_against(&self, current: &User) -> bool {
        match self {
            Delta::Update { old, .. } => {
                // Delta's "old" should match current state
                // If not, there's a conflict (someone else modified it)
                old == current
            }
            Delta::Insert { .. } => {
                // Check that entity doesn't exist
                true
            }
            Delta::Delete { old, .. } => {
                // Check that entity matches expected state
                old == current
            }
        }
    }

    /// Attempt to apply delta, returning conflict if invalid
    pub fn apply_with_validation(&self, current: Option<&User>) -> Result<(), ConflictError> {
        match (self, current) {
            (Delta::Update { old, .. }, Some(current)) => {
                if old == current {
                    Ok(())
                } else {
                    Err(ConflictError::StateChanged {
                        expected: old.clone(),
                        actual: current.clone(),
                    })
                }
            }
            // ... other cases
        }
    }
}
```

## Benefits of Final Approach

1. **Type Safety**: Impossible to update immutable fields (no method exists)
2. **Complete State**: Full snapshots enable robust conflict resolution
3. **Clear API**: `user.updater().email(x).username(y).build()` is self-documenting
4. **Sync-Ready**: Can reconstruct database state at any point by replaying deltas
5. **Debugging**: Full before/after state aids debugging and audit logs
6. **Flexible**: Easy to add `changed_fields()` helper when needed
7. **Per-Model Control**: Each model defines its own mutability rules

## Tradeoffs

### Storage Overhead

**Cost**: Storing full entity (including unchanged fields) uses more space than storing only changed fields.

**Why acceptable**:
- Deltas are typically transient (in-memory during sync, then applied and discarded)
- Complete state is essential for conflict resolution in offline-first sync
- Can compress delta streams if size becomes an issue
- Development/debugging benefits outweigh storage costs
- Modern systems have plenty of memory for transient data structures

### Changeset Not Stored

**Implication**: `changed_fields()` must be computed on-demand via `diff()`.

**Why acceptable**:
- Rarely needed (mainly for debugging/logging)
- Cheap to compute (simple field comparisons)
- Keeps Delta structure simple
- Can cache via `OnceCell` if needed

## Future Considerations

### Nullable Fields

When adding nullable fields to models:

```rust
pub struct User {
    pub actor_id: Uuid,
    pub username: Username,
    pub email: Email,
    pub nickname: Option<String>,  // Nullable field
}

impl UserUpdater {
    pub fn nickname(mut self, nickname: Option<String>) -> Self {
        self.new.nickname = nickname;
        self
    }

    // Or separate methods for clarity:
    pub fn set_nickname(mut self, nickname: String) -> Self {
        self.new.nickname = Some(nickname);
        self
    }

    pub fn clear_nickname(mut self) -> Self {
        self.new.nickname = None;
        self
    }
}
```

The builder can expose whatever API makes sense for nullable fields.

### Batch Updates

For applying multiple deltas in a transaction:

```rust
pub async fn apply_deltas(
    tx: &mut Transaction<Postgres>,
    deltas: Vec<ModelDelta>,
) -> Result<()> {
    for delta in deltas {
        delta.apply_delta(tx).await?;
    }
    Ok(())
}
```

### Optimistic Updates

For optimistic UI updates before sync:

```rust
// Apply delta optimistically
local_state.apply(delta.clone());

// Queue for sync
sync_queue.push(delta);

// Later: validate against server state
if !delta.is_valid_against(&server_state) {
    // Rollback or resolve conflict
}
```

### Delta Compression

If storage becomes a concern for long-lived delta logs:

```rust
// Deduplicate unchanged fields
pub fn compress_deltas(deltas: Vec<Delta<User>>) -> Vec<Delta<User>> {
    // Collapse consecutive updates to same entity
    // Only store final state
}

// Or use binary format instead of JSON
pub fn serialize_compact(delta: &Delta<User>) -> Vec<u8> {
    // Custom binary serialization
}
```

### Alternative: Mutable Subset Type

If you want to be explicit about which fields are mutable:

```rust
pub struct User {
    pub actor_id: Uuid,              // Immutable
    pub mutable: UserMutableFields,  // All mutable fields grouped
}

pub struct UserMutableFields {
    pub username: Username,
    pub email: Email,
}

// Delta stores full User (including immutable fields)
// But builder only exposes UserMutableFields
```

This makes mutability more explicit in the type system, but adds nesting complexity.

## Summary

The final approach uses:
- **Full entity snapshots** in Delta for conflict resolution and state reconstruction
- **Builder pattern** to enforce per-field mutability constraints at compile time
- **Changeset as view type** computed on-demand when needed
- **Type safety** preventing invalid states without runtime overhead

This balances the needs of a sync system (complete state), type safety (can't modify immutable fields), and ergonomics (chainable builder API).
