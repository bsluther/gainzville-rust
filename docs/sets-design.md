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
sequence's — under the Sets control, and hides member temporal entirely. Members may still carry
durations in the model (template instantiation produces exactly that shape); they're just not
editable from the sets UI in v1 — break out to reach them.

Rejected alternatives: per-member time rows (repetitive UX); inferring sequence time from members
(touches the invariant, Forest, and the day-filter query path); mutator-maintained pairing of
sequence time to the first member (drift-prone manual sync).

## UI (Swift app)

- **Sets row** renders only when `display_as_sets == true`, at the top of the attribute section,
  above Time: numbered pills (sibling order), `+`, `−`.
- The card **titles as the selected member** (the wrapper is anonymous); attributes, children, and
  the completion footer below the Time row are the selected member's, re-keyed per member so
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
sequence yet (see Deferred).

## Sync

No special handling. Invariants live in mutators, and rebase re-applies (re-validates) local
mutations: two offline clients adding members with different activities resolve by the second
client's action being rejected at rebase. Fractional indexing keeps concurrent member additions
collision-free. Duplicates mint fresh UUIDs, so concurrent duplicates cannot collide.

## Deferred / future work

- **Template-root sets** ("Bench Press's template = 3×5"): requires the wrapper to inherit the
  activity id, which makes activity queries double-hit (wrapper + members) on instantiated logs
  and breaks wrapper-anonymity symmetry. Revisit when template editing UX matures.
- **Per-set duration field**: an attribute-like Duration control on each set that edits only the
  member temporal's duration.
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
