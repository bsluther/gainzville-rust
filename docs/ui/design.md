# UI Design Decisions

## Navigation

### Library browsing: stack navigation, not sheets

On mobile, the library uses single-column stack navigation — detail replaces list, back button pops. On desktop, both panes are visible simultaneously.

```
Desktop:  [List | Detail]      — both visible, CSS layout
Mobile:   [List] → [Detail]    — one at a time, back button pops
```

The detail component is the same on both platforms. Navigation container differs by platform (configured via `#[cfg]`).

## Interaction Patterns

### Entry field editing: popover (desktop) vs. bottom sheet (mobile)

Entry field interactions (duration, reps, load, etc.) use a popover on desktop and a bottom sheet on mobile.

```
Desktop:  Popover anchored to field
Mobile:   Bottom sheet
```

This distinction is driven by input modality (pointer vs. touch), not screen size. A desktop app at 375px wide still uses a popover.
