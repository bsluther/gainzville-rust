## Attribute/Value Storage Design Decisions

### JSONB vs Normalized Tables

**Decision**: Single `attributes` table and single `values` table using JSON-as-TEXT for type-specific data, with index columns for query performance. TEXT is a simplifying measure for now; plan is to move to JSONB (Postgres) and JSONB BLOBs (SQLite) once the model stabilizes.

**Considered**: One table per attribute type and one table per value type (fully normalized). Rejected because:
- Repetitive schema and code (tried in a previous version of the project)
- Awkward to store list types (multiselect) in normalized rows
- DB-level constraints don't help: attributes are user-defined at runtime, so CHECK constraints can't validate values against attribute definitions anyway
- FK type safety (e.g. numeric value can't reference select attribute) is low-value since the attribute-value type relationship is immutable and validated in mutators
- Index columns are needed regardless (e.g. measures need `index_float` alongside display representation)

**JSON works well here because**:
- Rust enums provide strong application-level type safety
- All writes go through mutator validation
- Client-side attribute counts are small (thousands), so scanning is fine
- Postgres GIN indexes available if upgraded to JSONB later

### Planned vs Actual Values

**Decision**: Two nullable JSON-as-TEXT columns on a single row per entry-attribute pair.

**Considered**:
- Splitting the entry itself into planned/actual: too disruptive, plan and actual need to live visually within a single entry
- Two rows with a `kind` discriminator: more flexible but doubles rows and complicates the common "show both" query
- Two JSON columns (`plan`, `actual`): simpler, one row = one entry-attribute pair, one query gets both, storage efficient since most rows have only one populated

**No CHECK constraint on planned/actual.** Both columns are independently nullable. The value row itself represents the entry-attribute relationship, giving three states:
1. **No row** — entry does not have this attribute
2. **Row exists, both null** — entry has this attribute but no values yet (e.g. user added attribute, hasn't filled it in)
3. **Row exists, one or both non-null** — entry has planned and/or actual values

State 2 is needed because users can add/remove attributes per entry independently of the activity's defaults, so the value row is the only place that records "this entry has attribute X." A CHECK requiring at least one non-null column would prevent state 2 and require a separate junction table to track entry-attribute membership.

### Index Columns

**Decision**: Index columns derive from `actual` only. If no actual exists, index columns are null.

- Avoids conditional "which value do I index" logic in mutators
- Hot-path queries (max load, longest run, etc.) are against actuals
- Planning queries filter by entry timestamps and read JSON directly, don't need indexed aggregation
- Can add `planned_index_float` / `planned_index_string` later if needed

### Multiselect Indexing

**Decision**: Don't index multiselect values. Store the selection as a JSON array.

- Containment queries work on JSON if needed (native on JSONB, via `json_extract` on TEXT)
- At client-side volumes, scanning is fine
- Avoids junction tables that would break the single-table approach

### Rust Model Structure

**Decision**: Top-level `Attribute` struct with common fields + an `AttributeConfig` enum for type-specific data. Config variants wrap separate structs (e.g. `NumericConfig`, `SelectConfig`). See `core/src/models/attribute.rs`.

**Why struct + enum over top-level enum**: Common fields (`id`, `name`) are accessible without matching every variant. Maps directly to DB layout (common columns + JSON config column).

**Why separate config structs**: Enables typed narrowing — `attribute.expect_numeric() -> Result<&NumericConfig>`. Also gives a place for config-specific validation methods.

**No `attribute_type` field on the struct**: The enum variant is the discriminator. A separate field would create inconsistent states. Derive it when needed via `attribute.config.data_type()`.

### Single Measurement per Mass Value

**Decision**: A mass value is one measurement in one unit — `MassValue::Exact(MassMeasurement)` / `Range { unit, min, max }` (both range endpoints share the unit, so min ≤ max is validated without conversion). `MassConfig` carries a single required `default_unit`, used for the attach-time seed and for presenting an empty value. Planned measures (distance) should copy this shape.

**History**: Mass values were originally `Vec<MassMeasurement>` with aggregate semantics — each element an independent fact summing to the whole (`[220 lb, 100 kg]` = 440 lb), motivated by not reformatting mixed-unit user input. A survey of training-log use cases found no case where that's the natural shape; every mixed-unit scenario falls into one of three buckets, each better served another way:
- *Entry-time arithmetic* (mixed kg/lb plate loading, ruck pack sums) — the tracked quantity is the single total (which `index_float` already assumes), and a flat list doesn't capture the loadout faithfully anyway (no plate counts or per-side structure). A plate-math input helper is the right future tool.
- *Genuinely separate facts* (recipe ingredients, intervals, triathlon legs) — multiple values or child entries; aggregating them into one value is the wrong model.
- *Mixed-radix presentation of one quantity* (5'11", 12 st 6 lb, 7 lb 8 oz) — a display format, like the temporal type's `hh:mm:ss`. If wanted later, the upgrade path is a compound *display unit* (e.g. feet+inches) over a single stored magnitude — not a list of independent measurements.

The "don't reformat user input" concern is covered by storing the user's chosen unit per value (a stored value's unit may differ from `default_unit`).

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

- `AttributeConfig` uses serde's default external tagging: `{"Numeric": {"min": null, ...}}`. Originally used `#[serde(tag = "type")]` (internally tagged), but internally-tagged enums are incompatible with serde_json's `arbitrary_precision` feature (which is enabled workspace-wide via ivm/dbsp). External tagging avoids this by letting serde_json handle deserialization directly without going through serde's internal content buffer.
- `data_type TEXT` column populated at write time from the variant, for SQL-level filtering without parsing JSON.
- `#[serde(default)]` on new fields handles backward compatibility with existing JSON rows if variants evolve.
- See `sqlite/migrations/` and `postgres/migrations/` for table definitions.

## Deferred attribute UX considerations

A handful of UX questions were raised while building the first round of attribute views in the Swift app (`Features/Log/Attributes/`). They are recorded here so the rationale isn't lost.

### Range editing (implemented)
`Numeric`, `Select`, and `Mass` editors support both `Exact` and `Range` values. The action bar's Range checkbox toggles a pill between one exact input and a min–max pair (`RangePill`: two borderless inputs around a hyphen, the shared border on the composite). Select shows Range only when the config is `ordered`, and its two triggers share one options presentation with a min/max endpoint switcher. Semantics, decided deliberately:
- **Mode = stored value + local override.** There's no DB representation of an "empty range", so toggling writes nothing; a local override covers the gap until a commit (or abandonment) makes the stored value agree. Toggling range→exact collapses to min.
- **Prefill:** entering range mode, min inherits the exact value, max starts empty — prefilling both would let the debounce auto-commit a degenerate `5 – 5`.
- **Incomplete ranges never commit** (the mid-typing skip convention, extended): blur with half a range abandons the edit and resyncs.
- **Inverted ranges (min > max) hold during the debounce window and swap at blur/commit.** Select swaps at pick-commit (filtering the option list instead would deadlock raising min past the stored max). Mass swaps too — a range's endpoints share one unit, so the comparison needs no conversion (and core validates min ≤ max on write).

### Clear-value semantics (implemented)
`UpdateAttributeValue` takes an optional value, so a `None` write empties the field while keeping the attribute attached. Two paths reach it: the action bar's *Clear*, and emptying a numeric/mass exact input (the commit clears on debounce/blur rather than writing `0`, preserving the "intentionally zero" vs "cleared" distinction — zero requires typing `0`). An incomplete range still commits nothing; blur abandons it and resyncs from the stored value.

### Adding/removing attributes per entry (implemented)
A user can add or remove an attribute on any entry (log or template). `AttachValue { entry_id, attribute_id }` seeds the attribute's config default into a new value row — reaching state 3, or state 2 when the config default is `None`; `DeleteAttributeValue` removes the row. Both are idempotent (no-op if already attached / already absent), and both are in core + FFI. The picker is `EditAttributesView` (the entry menu's "Edit attributes"), which lists the owner's attributes with a per-entry toggle and, when the entry has an activity, a per-activity (template) toggle. `FindAttributePairsForEntry` still returns only existing value rows, which is correct now that attaching always creates one.

### Per-entry attribute order
`EntryJoin.attributes` is a `HashMap<Uuid, AttributePair>` (unordered). The Swift `AttributesSection` sorts by `attribute.name` ASCII as a placeholder. A real solution likely stores a per-entry-attribute order — either a `display_order` column on `values`, an array on the entry, or a separate ordering table. The figma shows deliberate orderings (Sets/Reps/Load) that name-sort doesn't preserve.

### Plan vs Actual toggle
`Value` carries both `plan` and `actual`. The figma shows a Plan/Log toggle on the entry; the data model is ready, but the toggle UI isn't built. `FfiUpdateAttributeValue.field` is plumbed through; the Swift editors hardcode `field: .actual` and would change to read a binding when the toggle exists.

### Per-attribute focus state (implemented as the action bar)
Per-attribute controls live in the focused attribute's action bar (keyboard accessory on iOS, popover on macOS, sheet header for picker kinds) rather than a permanent per-row menu. *Clear*, *Remove*, and *Range vs. exact* are wired; *Units* remains a placeholder. Mid-session presentation changes (the Range checkbox) reach the bar through `AttributeBarPublisher`, which re-publishes when an action's value-state changes — actions carry presentation as data and compare with a closure-blind `Equatable`.

### Unit selection / conversion for measures
`MassConfig.default_unit` is editable in the attribute profile (`AttributeDetailView` → Mass config) via `UpdateAttribute(Mass(SetDefaultUnit(..)))`; it's unconstrained since stored values carry their own unit. Still deferred: per-*entry* unit selection in the log editor (today the pill renders in `display_unit` — the actual value's unit, else the plan's, else `default_unit`) and conversion between units (e.g. the user switches a value from `lb` to `kg`) — a natural place for a pure helper in core (e.g. `MassValue::converted_to(unit)`), independent of the editor's UI state.

### Stateful "EditingAttribute" abstraction in core
Considered: a stateful editor type in core, reachable from any client, that holds intermediate edit state, runs validation, and handles unit redistribution for measures. Deferred in favor of a simpler arrangement:
- The Swift editors hold their own shadow state and commit through the existing `Action` API (`UpdateAttributeValue`).
- Domain rules that *would* benefit from cross-platform sharing — clamping, integer rounding, unit conversion — can be exposed as pure helpers when needed, without dragging stateful editor objects across the FFI boundary.

The trigger to revisit: when a third client appears (web, Android, etc.) or when unit-conversion logic exceeds what's reasonable to duplicate per-platform.
