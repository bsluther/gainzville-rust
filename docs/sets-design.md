# Sets Design

## Summary

Sets are the standard way athletes describe workouts: "6 sets of 4 reps of bench press" means six
repeated blocks of the same exercise. Gainzville models a set as a **sequence entry whose immediate
children (set members) repeat the same activity**, marked with `display_as_sets = true`. Sets are
primarily a *presentation* strategy: instead of rendering a sequence of (Bench Press, Bench Press,
…) as nested cards, the UI hides the sequence level and shows what looks like a single Bench Press
with an attribute-like "Sets" picker (mockup: `ux-design-assets/figma-log-scroll-1.png`).

**Terminology:** "set member" = an immediate child of the `display_as_sets` sequence. The sequence
itself is called the "sets sequence" or "wrapper".

## Model

The wrapper is an ordinary sequence entry plus one flag — there is no new table or entity:

- The wrapper is **anonymous** (`activity_id = None`, `name = None`). Activity queries ("all my
  bench presses") hit the members, never the wrapper.
- Set numbers are the members' sibling order (fractional index). Reordering members is legal;
  the sets UI just doesn't expose it.
- Members may carry any attributes/values, varying freely between sets (set 2 has Reps+Load,
  set 3 has Reps only). Values are keyed `(entry_id, attribute_id)`, so nothing set-specific is
  needed here.
- Members may themselves be sequences ("sets of Core Series"). Member subtrees need **not** be
  structurally equal — only member activity equality is enforced. Nested sets (a member that is
  itself a sets sequence) are not prohibited.

## Invariants

Enforced continuously in the mutators (no toggle-time-only checking, no sync carve-out — rebased
mutations re-validate automatically):

1. `display_as_sets = true` ⇒ the entry is a sequence.
2. The sequence has **≥ 1 member** while flagged. Rationale: the card renders *as* its members;
   with zero members there is no title source at all (the wrapper is anonymous, and anonymous
   sequences may not have names).
3. All members share one `activity_id`, or all have none. Mixed activity/anonymous is illegal.

Because `activity_id` is immutable after creation, the complete enforcement surface is:

| Mutator | Guard |
|---|---|
| `UpdateEntry(SetDisplayAsSets(true))` | sets shape: sequence, ≥1 member, homogeneous activity |
| `UpdateEntry(SetDisplayAsSets(false))` | always legal ("break out") |
| `UpdateEntry(SetIsSequence(false))` | rejected while flagged (would also deep-delete members) |
| `CreateEntry` | rejected if born flagged; activity match when the parent is flagged |
| `CreateEntryFromActivity` | activity match when the destination parent is flagged |
| `CreateActivity` | template trees validate the sets shape for any flagged template entry |
| `MoveEntry` (into a flagged parent) | activity match (same-parent reorders pass trivially) |
| `MoveEntry` (out of a flagged parent) | rejected if the mover is the last member |
| `DeleteEntryRecursive` | rejected if the target is a flagged parent's last member |
| `ConvertToSets` | see below; rejected on activity template roots and when the anonymous wrapper would break an enclosing sets sequence's homogeneity |
| `DuplicateEntry` | homogeneity holds by construction; rejected on activity template roots |

Fault taxonomy: guards on the *attempted* transition are `Rejected(Precondition)`. Discovering that
*stored* members of an already-flagged sequence disagree on activity is an `InvariantViolation`
("sets members share one activity") — a bug, which the property/fuzz harness halts on.

## Actions

### `ConvertToSets { actor_id, entry_id, sequence_id }`

Converts an entry E into a sets sequence in **one atomic mutation**:

- Insert wrapper S at E's position: anonymous, `is_sequence`, `display_as_sets = true`,
  `is_template = E.is_template`, incomplete.
- **Temporal split**: S takes E's start/end components; E keeps only its duration component.
  The sequence owns the timeline slot; members carry durations. A root E must already be on the
  timeline, so the root "must have start or end" rule transfers losslessly to S.
- Reparent E under S as the sole member (set 1).

`sequence_id` is client-supplied (like `CreateEntry`'s client-built entry) so the UI can reference
S before the mutation lands — used to keep the card expanded across the E→S identity swap.

Rejected on activity template roots: the template-tree rule requires the root to carry the
activity's id, and the wrapper is anonymous (see Deferred).

### `DuplicateEntry { actor_id, entry_id }`

Deep-copies the entry's subtree **verbatim** — attributes, values, member temporals, completion —
with fresh entry ids (values re-key automatically since they are keyed by entry id). The copy is
inserted immediately after the source among its siblings; a forest root duplicates as another root
with the same temporal, landing adjacent in the day view. Shared by the sets "+" button and the
entry-menu Duplicate item. Implemented over `duplicate_subtree`, a sibling of
`instantiate_subtree` sharing its id-map/remap/re-key mechanics (`core/src/instantiation.rs`).

Rejected on activity template roots (each activity has exactly one template root).

### `UpdateEntry(EntryChange::SetDisplayAsSets(bool))`

`true` validates the sets shape; `false` ("break out") is always legal and returns the sequence to
the standard nested-cards view, after which members may diverge. Re-flagging re-validates.

Breaking out also **names the wrapper** when it has no name: `"<first member's display name>
Sets"`, following the canonical display-name rule (member's own name, else its activity's name; a
fully anonymous member leaves the sequence unnamed rather than producing "Unnamed Sets"). The
rename rides the same `Delta::Update` as the flag — one atomic mutation — and never clobbers an
existing name.

## Temporal model

The sets sequence has a **normal temporal obeying all existing rules** — the root invariant and
day-grouping (`canonical_instant`) are untouched. The sets UI shows exactly one Time row — the
sequence's — under the Sets control (start/end: when the set group happened). Each member then shows
a **per-set Duration row** that edits only its own duration component (the hang/plank/sprint time).
Members carry duration only; the sequence owns start/end. (In v1, member temporal was hidden
entirely — reachable only by breaking out; the per-set Duration row shipped 2026-06-13, see "Per-set
duration" below.)

Rejected alternatives: full per-member **Time** rows with start/end (repetitive UX — two competing
"Time" headers; the shipped per-set row shows only **Duration**, a distinct label below a separator,
so it reads as hierarchy rather than repetition); inferring sequence time from members (touches the
invariant, Forest, and the day-filter query path); mutator-maintained pairing of sequence time to
the first member (drift-prone manual sync).

### Per-set duration (member duration row)

Shipped 2026-06-13. A flat, always-visible **Duration** row on each set member (and on library
template entries) edits only the member's duration, in the attribute-row style. It is a Swift-only
view (`DurationAttribute` in `Features/Log/Attributes/TemporalAttribute.swift`) with **no model
change** — every entry already carries a `temporal`, and `ConvertToSets` already gives members a
duration-only temporal, so this closed a *presentation* gap, not a storage gap (D1).

- **Duration-only writes (clobber-all).** The row always writes a duration-only temporal
  (`Temporal::Duration` or `None`) through the existing `MoveEntry` / `updateEntryTemporal` path.
  The 2-of-3 rule is satisfied structurally (no over-determined variant exists), so the row needs
  none of the full editor's conflict-alert machinery. For the (currently unreachable) member that
  already carries start+end, the row shows the **derived** duration and editing it **replaces**
  start/end — a deliberate clobber, accepted because the case is rare and the alternative (porting
  the conflict UI into a row that hides start/end) is costly and confusing.
- **Set members are never roots** (they have a position under the wrapper), so the root "must have
  start or end" rule never binds them — duration-only members are always legal.
- **Templates** render the same flat Duration row in place of the old collapsible "Time" accordion
  (which, for templates, only ever revealed a single Duration row anyway). A template sets sequence
  shows no sequence-level temporal row — its durations live per-member.
- **Deferred — configurable presentation:** letting the user choose *per entry* whether Time and/or
  Duration appear (e.g. `present_time`/`present_duration` flags from the Edit Attributes sheet) was
  considered and deferred: it pushes pure-presentation state into the domain model at a large
  cross-layer cost, and the stated use cases (fingerboarding, planks, sprints) are all set members
  already served without it. The general framing — Time and Duration as attribute-style controls
  that are **views onto the same `temporal`** rather than separate stored attributes — is recorded
  in `attributes-design.md` ("Time / Duration as temporal views").

## UI (Swift app)

- Layout, top to bottom: the sequence's **Time** row, the **Sets** row, a separator, then the
  selected member's **Duration** row, attributes, children, and footer. The separator divides the
  sequence-owned rows (Time, Sets) from the per-member rows.
- **Sets row** renders only when `display_as_sets == true`: numbered pills (sibling order), `+`,
  `−`. It sits below the sequence's Time row (which describes the whole group) and above the
  separator.
- **Per-set Duration row**: a flat Duration control on the selected member, the first per-member row
  below the separator. Edits the member's duration only (see "Per-set duration" above).
- The card **titles as the selected member** (the wrapper is anonymous); the Duration row,
  attributes, children, and completion footer are the selected member's, re-keyed per member so
  editor state doesn't leak across set switches.
- **`+` duplicates the LAST member and appends**; the new set becomes selected. Independent of
  picker selection (the next set most resembles the previous one).
- **`−` deletes the SELECTED member** (whole subtree + values); selection moves to its
  predecessor. Disabled at one member (core also rejects).
- **Conversion**: the entry menu's "With sets" converts only — the entry becomes set 1 and the
  card shows `Sets: [1]`. Conversion ≠ addition. The card stays expanded across the swap via the
  client-minted `sequence_id` (`ForestViewModel.pendingExpandedEntryIds`).
- **Break out**: menu item on sets cards; clears the flag, returns to nested cards.
- "With sets" and "Duplicate" are hidden on activity template roots (core rejects both).

## Templates

Sets work in template trees with no special-casing: both root-temporal enforcement sites are
`!is_template`-guarded, templates are duration-only, and `instantiate_subtree` preserves
`display_as_sets` (the instantiated root takes the caller's temporal; members keep template
durations — exactly the sets temporal shape). `CreateActivity` validates the sets shape inside
submitted templates. The one exception: an activity's template **root** cannot itself be a sets
sequence yet (see Deferred). In the UI, template entries render the flat per-set Duration row in
place of the old collapsible Time accordion (see "Per-set duration").

## Sync

No special handling. Invariants live in mutators, and rebase re-applies (re-validates) local
mutations: two offline clients adding members with different activities resolve by the second
client's action being rejected at rebase. Fractional indexing keeps concurrent member additions
collision-free. Duplicates mint fresh UUIDs, so concurrent duplicates cannot collide.

## Deferred / future work

- **Template-root sets** ("Bench Press's template = 3×5"): requires the wrapper to inherit the
  activity id, which makes activity queries double-hit (wrapper + members) on instantiated logs
  and breaks wrapper-anonymity symmetry. Revisit when template editing UX matures.
- ~~**Per-set duration field**~~ — **shipped 2026-06-13** (see "Per-set duration" above).
- **Configurable Time/Duration presentation**: per-entry control over which temporal fields show,
  as attribute-style views onto the one `temporal` (not separate stored values). See "Per-set
  duration" above and `attributes-design.md` → "Time / Duration as temporal views". Deferred —
  pushes pure-presentation state into the domain model; current consumers (set members, templates)
  are served by context-derivation.
- **Conversion affordance in the add-attributes UI**: the Sets control presents like an
  attribute, so "add Sets" may belong next to "add attribute" rather than in the entry menu.

## Decision log

| # | Decision | Rationale |
|---|---|---|
| D1 | Sequence owns the Time row; member temporal hidden in sets UI | Zero model/query changes; per-set durations stay representable |
| D2 | `+` = duplicate last + append, exact copy; `−` = delete selected | Predictable, matches gym flow; exact copy keeps one duplicate helper with no special cases |
| D3 | "With sets" converts only (lands at 1 set); Sets row only when flagged | Conversion ≠ addition; matches mockup |
| D4 | Wrap is one atomic action with client-supplied `sequence_id` | One-action-per-transaction architecture; UI state transfer across the identity swap |
| D5 | Core enforces ≥1 member | A sets card without members has no title source; UI `−` guard alone wouldn't cover other write paths |
| D6 | Break out always legal | Escape hatch required by D5 and D1 |
| D7 | Template-root conversion rejected in v1 | Anonymous wrapper vs. template-root activity rule; defers query double-hit semantics |
| D8 | Entry-menu Duplicate wired in v1 | The action exists for "+" anyway; menu placeholder was already present |
| D9 | Per-set Duration row (member, duration-only, clobber-all); templates use it in place of the Time accordion | Closes the v1 presentation gap (D1) with zero model change; duration-only can't over-determine, so no conflict UI needed; configurable per-entry Time/Duration presentation deferred to attribute-style temporal views |
