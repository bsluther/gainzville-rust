## Attribute/Value Storage Design Decisions

### JSONB vs Normalized Tables

**Decision**: Single `attributes` table and single `values` table using JSONB for type-specific data, with index columns for query performance.

**Considered**: One table per attribute type and one table per value type (fully normalized). Rejected because:
- Repetitive schema and code (tried in a previous version of the project)
- Awkward to store list types (multiselect) in normalized rows
- DB-level constraints don't help: attributes are user-defined at runtime, so CHECK constraints can't validate values against attribute definitions anyway
- FK type safety (e.g. numeric value can't reference select attribute) is low-value since the attribute-value type relationship is immutable and validated in mutators
- Index columns are needed regardless (e.g. measures need `index_float` alongside display representation)

**JSONB works well here because**:
- Rust enums provide strong application-level type safety
- All writes go through mutator validation
- Client-side attribute counts are small (thousands), so JSONB scanning is fine
- Postgres GIN indexes on JSONB are available if needed later

### Planned vs Actual Values

**Decision**: Two nullable JSONB columns on a single row per entry-attribute pair.

**Considered**:
- Splitting the entry itself into planned/actual: too disruptive, plan and actual need to live visually within a single entry
- Two rows with a `kind` discriminator: more flexible but doubles rows and complicates the common "show both" query
- Two JSONB columns (`planned`, `actual`): simpler, one row = one entry-attribute pair, one query gets both, storage efficient since most rows have only one populated

**No CHECK constraint on planned/actual.** Both columns are independently nullable. The value row itself represents the entry-attribute relationship, giving three states:
1. **No row** — entry does not have this attribute
2. **Row exists, both null** — entry has this attribute but no values yet (e.g. user added attribute, hasn't filled it in)
3. **Row exists, one or both non-null** — entry has planned and/or actual values

State 2 is needed because users can add/remove attributes per entry independently of the activity's defaults, so the value row is the only place that records "this entry has attribute X." A CHECK requiring at least one non-null column would prevent state 2 and require a separate junction table to track entry-attribute membership.

### Index Columns

**Decision**: Index columns derive from `actual` only. If no actual exists, index columns are null.

- Avoids conditional "which value do I index" logic in mutators
- Hot-path queries (max load, longest run, etc.) are against actuals
- Planning queries filter by entry timestamps and read JSONB directly, don't need indexed aggregation
- Can add `planned_index_float` / `planned_index_string` later if needed

### Multiselect Indexing

**Decision**: Don't index multiselect values. Store the selection as a JSON array.

- Containment queries (`data @> '["option"]'`) work on JSONB if needed
- At client-side volumes, scanning is fine
- Avoids junction tables that would break the single-table approach

### Rust Model Structure

**Decision**: Top-level `Attribute` struct with common fields + an `AttributeConfig` enum for type-specific data. Config variants wrap separate structs (e.g. `NumericConfig`, `SelectConfig`).

```rust
pub struct Attribute {
    pub id: Uuid,
    pub name: String,
    pub config: AttributeConfig,
}

pub enum AttributeConfig {
    Numeric(NumericConfig),
    Select(SelectConfig),
}
```

**Why struct + enum over top-level enum**: Common fields (`id`, `name`) are accessible without matching every variant. Maps directly to DB layout (common columns + JSONB config column).

**Why separate config structs**: Enables typed narrowing — `attribute.expect_numeric() -> Result<&NumericConfig>`. Also gives a place for config-specific validation methods.

**No `attribute_type` field on the struct**: The enum variant is the discriminator. A separate field would create inconsistent states. Derive it when needed via `attribute.attribute_type()`.

### No owner_id on Values

**Decision**: Values do not carry an `owner_id`. Ownership is derived from the entry.

**Invariant**: A value's owner is always the owner of the described entry (which must also match the attribute's owner). Storing `owner_id` on the value would duplicate this and require mutators to enforce consistency on every write.

**Considered**: Denormalized `owner_id` on values for filterless owner queries (e.g. "all of user A's values > X"). Rejected because:
- Those queries almost always need entry context anyway, so the join to `entries` happens regardless
- The join is cheap — `entry_id` is indexed (FK), and `entries.owner_id` is a simple column lookup
- At client-side volumes (thousands of entries), the join cost is negligible
- Can add a denormalized column later if profiling shows a need; removing one is harder

### Composite Key on Values

**Decision**: Values are identified by the composite key `(entry_id, attribute_id)` — no surrogate `id` column.

**Invariant**: At most one value per entry-attribute pair. Repetition (e.g. multiple notes) is modeled inside the attribute type, not by duplicating value rows.

**Why composite over surrogate**:
- Enforces the 0-1 value constraint at the DB level (PK = unique) without a separate unique index
- Matches the natural access pattern — values are always looked up by (entry, attribute), never by a standalone ID
- One fewer UUID to generate and manage per value row
- If anything references a value, `(entry_id, attribute_id)` is more meaningful than an opaque UUID

### Serde / DB Mapping

- `#[serde(tag = "type")]` on `AttributeConfig` produces internally-tagged JSON: `{"type": "Numeric", "min": null}`.
- `data_type TEXT` column populated at write time from the variant, for SQL-level filtering without parsing JSON.
- Two layers of type discrimination (column + JSON tag) are intentionally redundant across different access patterns.
- `#[serde(default)]` on new fields handles backward compatibility with existing JSONB rows if variants evolve.

### Sketch

```sql
attributes (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    data_type TEXT NOT NULL,
    config JSONB NOT NULL,       -- type-specific: options, constraints, unit_type, etc.
    ...
)

attribute_values (
    entry_id UUID NOT NULL REFERENCES entries(id),
    attribute_id UUID NOT NULL REFERENCES attributes(id),
    planned JSONB,
    actual JSONB,
    index_float FLOAT,           -- derived from actual only
    index_string TEXT,            -- derived from actual only
    PRIMARY KEY (entry_id, attribute_id)
)
```
