# UI Architecture Decisions

## Platform Targeting

### `#[cfg]` for input modality, CSS for layout

- **Input modality differences** (touch vs. pointer, one-handed reachability) → `#[cfg(feature = "mobile")]`
- **Spatial arrangement of the same content at different widths** → CSS
- **Different navigation containers** (tab bar vs. sidebar) → `#[cfg]`
- **Runtime viewport-width-dependent behavior** → `use_media_query` only if the above don't apply (no current cases)

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

### No runtime responsive rendering

No `use_media_query`, `use_breakpoint`, or runtime viewport-driven component swapping. Desktop components may be CSS-responsive (e.g., nav collapsing at narrow widths) but do not become mobile components. A `use_media_query` hook via `document::eval()` + `window.matchMedia()` is ~20 lines if a genuine runtime case surfaces later.

## Styling
