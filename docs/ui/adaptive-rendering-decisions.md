# Adaptive Rendering & Platform UX Decisions

**Date:** 2025-03-25
**Status:** Accepted

## Context

Gainzville targets multiple platforms (web, desktop, mobile) via Dioxus. Two features initially seemed to require runtime responsive rendering — swapping entirely different component trees based on viewport breakpoints (e.g., a `use_media_query` hook). After analysis, neither case actually requires this. The distinction that matters is **platform/input modality** (compile-time) vs. **layout at different widths** (CSS), not **viewport-driven component swapping** (runtime).

Dioxus has no built-in `use_media_query` or `use_breakpoint` hook as of 0.7. A custom implementation via `document::eval()` and `window.matchMedia()` is straightforward (~20 lines), but the question is whether it's needed at all.

## Decisions

### 1. Library activity/attribute browsing: stack navigation, not sheets

**Decision:** On mobile, the library uses single-column stack navigation (detail replaces list, back button pops). On desktop, both panes are visible simultaneously. No bottom sheet for detail views.

**Rationale:**

- A bottom sheet is a **modal** interface. Library content contains navigable links (subcategories → other activities), which implies hierarchical, stack-based navigation. These two models conflict: a sheet says "this is transient, your context is preserved underneath" while following a link says "you've moved forward in a hierarchy." Pushing a second view from within a sheet breaks the sheet metaphor.
- Without the sheet, the difference between desktop and mobile is **purely layout** — wide shows both panes, narrow shows one at a time. CSS handles this. The navigation model is identical on both platforms (select item → see detail), only the spatial arrangement differs.
- Stack navigation is the standard mobile pattern and gives us real navigation semantics: deep links work, browser/gesture back works, and subcategory links are just pushes.

**Pattern:**

```
Desktop (wide):  [List | Detail]        — both visible, CSS layout
Mobile (narrow): [List] → [Detail]      — one at a time, back button pops
```

The detail component is the same in both cases. On mobile it lives in a different navigation container (configured via `#[cfg]`), but the component itself is platform-agnostic.

### 2. Entry field interaction: popover (desktop) vs. sheet (mobile) via `#[cfg]`

**Decision:** Use `#[cfg(feature = "mobile")]` to select between a bottom sheet and a popover for entry field interactions (duration, reps, load, etc.). This is a compile-time platform distinction, not a viewport breakpoint.

**Rationale:**

- The reason sheets work better on mobile is **input modality**, not screen size. Thumbs are imprecise; bottom-anchored, consistently-placed targets are reachable one-handed. A desktop user who resizes their browser to 375px still has a mouse with pixel-level precision — giving them a bottom sheet solves the wrong problem.
- Obsidian is a good reference: nearly all context menus are bottom sheets on mobile, dropdowns/popovers on desktop. The distinction maps to touch vs. pointer, not wide vs. narrow.
- `#[cfg]` makes this easy to reason about: on desktop you get one interaction component, on mobile you get another. No runtime checks, no signal initialization flash, no ambiguity.

**Pattern:**

```rust
#[cfg(feature = "mobile")]
fn EntryFieldEditor(/* ... */) -> Element {
    rsx! { BottomSheet { /* ... */ } }
}

#[cfg(not(feature = "mobile"))]
fn EntryFieldEditor(/* ... */) -> Element {
    rsx! { Popover { /* ... */ } }
}
```

### 3. No runtime responsive rendering (for now)

**Decision:** Do not implement `use_media_query`, `use_breakpoint`, or any runtime viewport-driven component swapping. Rely on `#[cfg]` for platform differences and CSS for layout adaptation.

**Rationale:**

- The two motivating use cases decompose cleanly into platform concerns (`#[cfg]`) and layout concerns (CSS). The middle category — runtime viewport checks swapping component trees — is empty for current requirements.
- Desktop components can still be CSS-responsive (e.g., nav items collapse behind a menu button at narrow widths), but they don't need to become mobile components. A desktop app at 375px wide is still a desktop app with a pointer.
- If a genuine runtime case surfaces later, the implementation cost is trivial: a `use_media_query` hook using `document::eval()` + `window.matchMedia()` is ~20 lines and works across all webview-based renderers. There is no cost deferred by waiting.

**Heuristic for future decisions:** Before reaching for `use_media_query`, ask:

1. Is the difference about **input modality** (touch vs. pointer, one-handed reachability)? → `#[cfg]`
2. Is the difference about **spatial arrangement** of the same content? → CSS
3. Is the difference about **fundamentally different navigation containers** (tab bar vs. sidebar)? → `#[cfg]`
4. Is there a remaining case that's truly viewport-width-dependent at runtime? → Only then consider `use_media_query`

## References

- Dioxus 0.7 platform guide: https://dioxuslabs.com/learn/0.7/guides/platforms/
- `document::eval()` API for JS interop (works on web, desktop webview, mobile webview)
- `window.matchMedia()` for future use if runtime responsive rendering is needed
- `dioxus-resize-observer` (community crate) for container-query-like element size tracking if needed
