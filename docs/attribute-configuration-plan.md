# Attribute editing & configuration — phased implementation plan

Covers three related features and the decisions that connect them:

1. **Entry attribute set** — add/remove attribute values on a specific entry.
2. **Attribute config** — configure an attribute's baseline defaults (library profile).
3. **Activity config / template** — configure an activity's template (a tree) and the
   default attribute values attached to its entries.

## Foundational decisions

- **Materialized at create (model B).** A `Value` row is a concrete fact, never a
  render-time resolution. Editing a template or attribute default **never** mutates
  existing entries — it only affects future instantiations. This cleanly decouples the
  three features.
- **Create-time template instantiation is deferred** to a later feature. These three
  features build the editing surfaces + data model that instantiation will consume.
  Until then, creating an entry from an activity yields a bare entry; attributes are
  attached via Feature 1.
- **Precedence (activity config > attribute config)** becomes a create-time resolution
  (in the future instantiation feature), not a live rule.
- **DB layer is ready.** SQLite delta executors already handle `Attribute` Update and
  `Value` Insert/Update/Delete. Verify Postgres parity; no schema work expected.

---

## Phase 1 — Entry attribute set

The screen: `EditAttributesView` (currently static placeholder data).

### Write path
- **Attach** = new action `AttachValue { actor_id, entry_id, attribute_id }`.
  - Mutator loads the attribute, resolves its config default into a seeded `Value`
    (both `plan` and `actual` set to the default; `None` default → attached empty),
    applies a no-op-if-exists guard, emits `Delta::<Value>::Insert`.
  - **Mass** (no scalar default): seed **both `plan` and `actual`** (same as the scalar
    rule) with `MassValue::Exact` holding one `MassMeasurement` per `default_unit`, each
    magnitude `0` (units shown, empty values).
  - Seeding lives in core as `Attribute::seed_value(entry_id) -> Value`: for
    Numeric/Select it maps `config.default` one-to-one to an `AttributeValue`; for Mass it
    constructs the measurement list from `default_units`. (A thin
    `default_value() -> Option<AttributeValue>` covers only the scalar cases — Mass is
    handled inside `seed_value`.) Reused by the future instantiation feature.
- **Detach** = new action `DeleteAttributeValue { actor_id, entry_id, attribute_id }`.
  - No-op (empty deltas) if the value doesn't exist — idempotent toggle.
  - Owner check mirrors `update_attribute_value`. Emits `Delta::<Value>::Delete { old }`.
- **`CreateValue` guard**: make the existing `create_value` mutator a no-op if a value
  for `(entry_id, attribute_id)` already exists (instead of hitting a PK conflict).
- FFI: export `AttachValue` / `DeleteAttributeValue` via uniffi. ArbitraryFrom impls for
  deterministic-simulation tests (owner-aware, per `feedback_arbitrary_owner_aware`).

### Read path
- No core changes. Entry's attached values already arrive via `FindEntryJoinById`
  (`EntryViewModel`) / `AttributePair`.
- `EditAttributesView` subscribes to `FindAttributesByOwner(owner_id)` for the full list.

### UI
- Wire the **"This entry" column only**. Add an explicit `hasActivity` parameter
  (don't key the activity column off `activityName`); the "All entries" column is
  deferred to Phase 3. (The parameter is added in Phase 1 but stays unused until Phase 3.)
- **Sort (snapshot at open, frozen for component lifetime):** entry-attached attributes
  first, then the rest alphabetical by name. Checkbox state updates live; positions
  never reorder during interaction. An attribute created mid-session (delivered by the
  `FindAttributesByOwner` subscription) appends to the end of the frozen list — never
  reorders existing rows. Search filters the frozen list.

---

## Phase 2 — Attribute config

The screen: library `AttributeDetailView` (currently "Config: Coming soon").

### Write path — `UpdateAttribute`
```
struct UpdateAttribute { actor_id, attribute_id, change: AttributeChange }

enum AttributeChange {
    SetName(String),                 // common, unconstrained
    SetDescription(Option<String>),  // common, unconstrained
    Numeric(NumericChange),          // SetDefault, (later) RaiseMax, LowerMin, SetInteger
    Select(SelectChange),            // SetDefault, (later) AddOption, RenameOption, SetOrdered
    Mass(MassChange),                // SetDefaultUnits / AddUnit / RemoveUnit
}
```
- One `Action` variant; payload enum captures intent (mirrors `UpdateAttributeValue`'s
  `ValueField`, and `AttributeConfig`/`AttributePair`'s Numeric/Select/Mass grouping).
- Mutator matches the type variant against `attribute.config` via the existing
  `expect_numeric`/`expect_select`/`expect_mass` helpers (mismatch → `AttributeMismatch`),
  builds the new `Attribute`, emits `Delta::<Attribute>::Update`.
- **Phase 2 implements only:** `SetName`, `SetDescription`, the three `Set*Default`
  variants, and `Mass` `default_units` editing (free add/remove — unit changes don't
  invalidate existing values, so *not* additive-constrained, unlike select options).
- Additive-only constraints (select options, numeric bounds) are deferred along with
  the read-only display of those fields; add their `AttributeChange` variants then.
- **Mutator validation** (each is one guard): `owner_id == actor_id` for all variants;
  `SelectChange::SetDefault(Some(s))` requires `s ∈ config.options`;
  `NumericChange::SetDefault(Some(v))` must satisfy `integer`/`min`/`max` (reuse
  `NumericConfig::new`'s checks).
- **Prerequisite:** confirm the Postgres delta executor handles `Delta::<Attribute>::Update`
  (SQLite already does); no existing mutator emits it yet.
- FFI + ArbitraryFrom as above.

### UI
- Default control per type **reuses visual primitives** (`gvAttributePill()`,
  `GvCheckbox`, dropdown chrome) in bespoke, simpler controls — *not* the stateful log
  editors (a config default is a scalar: no plan/actual, no range, no debounce-to-
  `UpdateAttributeValue`). Establishes the shared visual language without touching the
  log editors.
- Other config fields rendered read-only for now.
- `AttributeDetailView` must read live (reflect edits) — subscribe rather than hold a
  static `Attribute` copy.

---

## Phase 3 — Activity config / template

The screen: library `ActivityDetailView` (currently "Attributes: Coming soon").

### Read path — forest method
- Templates live in the single `AllEntries` cache; **filter on read.**
  - Add `forest_activity_template_root(activity_id) -> Option<Entry>`
    (`is_template && activity_id == Some(id) && position == None`; `CreateActivity`
    guarantees exactly one).
  - Rename/narrow the log path to `log_roots_in` with an **explicit `!is_template`**
    filter (don't rely on the implicit "templates have no canonical instant" guarantee).
- `ActivityDetailView` subscribes to the forest (like `LogView`), finds the root, and
  renders it with the reused `EntryView`. `ForestViewModel.children`, `EntryView`, and
  `EntryViewModel` all work unchanged on template entries.
- Favor free functions / static methods on `Forest` so traversal logic can also operate
  on a template subtree, not only the whole cache.

### Write path
- Reuse existing actions: `CreateEntry`, `MoveEntry`, `AttachValue`,
  `UpdateAttributeValue`, `DeleteAttributeValue` — a template entry is just an entry.
- **Temporal invariant** for template entries: only `Temporal::None` or
  `Temporal::Duration` (i.e. `temporal.start().is_none() && temporal.end().is_none()`).
  - Centralize in a shared validation helper used by both `create_entry` and
    `move_entry`.
  - Template roots are **exempt** from the log-root "must have a start or end" rule.
  - `move_entry` already enforces "parent and child both template or both log", so
    drag-and-drop *within* a template tree needs no additional core work.

### UI
- **Reuse `EntryView`** via *scaffold + injected chrome*: extract the shared structure
  (name header, attribute section, children recursion, container) into a scaffold taking
  closure/`@ViewBuilder` slots for the variable pieces — temporal editor, header trailing
  accessory, menu, drop delegate. Two call sites (log / template) supply the right
  pieces. (Composition over conditionals; avoids both reimplementation and a flag-branched
  god-view.) **Impl details to be worked out at the start of this phase.**
  - Template temporal editor = duration-only (no start/end pickers).
  - Completion checkbox suppressed for templates.
  - **Drag-and-drop retained** for sequence templates (restructure via `MoveEntry`);
    day-root drop is log-only.
- Wire the **"All entries" column** in `EditAttributesView` (gated by `hasActivity`),
  populated from the activity's template attributes; add the activity-attached sort group.

---

## Out of scope (noted)
- ~~Create-time template instantiation (consumes all three features).~~
  **Implemented:** `CreateEntryFromActivity` action + mutator deep-copies the
  activity's template subtree (entries + values, fresh ids, `is_template`
  cleared) via `instantiation::instantiate_subtree`; the Swift create flow routes
  through it when an activity is chosen. Template `None` values copy verbatim
  (activity-config precedence); structure (incl. `is_sequence`) comes from the
  template.
- "Create activity from a log entry" menu action (sequences, and anonymous scalars).
- Additive-only enforcement for select options / numeric bounds (deferred with their
  read-only displays in Phase 2).