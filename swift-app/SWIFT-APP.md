# Swift App — Patterns & Guidelines

## Architecture

- **Business logic lives in `core`** (Rust, via FFI). The Swift app is a thin client: it reads from `ForestViewModel`/`ActivitiesViewModel`, dispatches actions, and renders.
- **UI state and rendering logic live in Swift.** The boundary between core concerns and UI concerns can be blurry (e.g. display names, temporal formatting) — consider carefully before adding logic to either side.
- **Target platforms: iOS and macOS.** All views should build for both. Use `#if os()` or platform-specific types when behaviour genuinely differs; don't bend design to avoid it.

## Code Style

- Prefer idiomatic Swift and modern SwiftUI (Swift 6+, `@Observable`, typed throws, etc.).
- Be willing to break out to `NSViewRepresentable` / `UIViewRepresentable` when SwiftUI can't achieve the design goal — AppKit/UIKit are first-class options, not last resorts.
- Subcomponents scoped to one view stay in that file. Extract to a shared location only when used across views.
- Platform-specific types are named by platform (`FooMacOS`, `FooIOS`), not by container (`FooPopover`, `FooSheet`). The container is external to the component.

## Design System

- Tokens: `GvColor`, `GvFont`, `GvSpacing` in `DesignSystem/`. Semantic aliases (e.g. `gvAttributeField`) live in `GvColor.swift` when they don't warrant a full asset catalog entry.
- `platformPopover` (`GvPresentation.swift`) handles sheet-vs-popover automatically. Use it for all picker-style overlays. If a use case doesn't fit (e.g. toolbar anchoring), drop down to direct `.sheet`/`.popover` — don't contort the abstraction.
- AppKit-backed pickers (`CalendarPickerMacOS`, `TimeFieldMacOS`) use `NSViewRepresentable` with cleared backgrounds to avoid double-box rendering. Follow this pattern for any new AppKit picker.

## Building & Updating the FFI Library

When Rust code changes, agents and developers need to rebuild the Swift-facing artifacts. Two scripts in `scripts/` handle this — always run from the **workspace root**.

| Situation | Script |
|-----------|--------|
| FFI surface changed (`types.rs`, exported types, new `FfiAction` variants, etc.) | `scripts/regen-bindings.sh` |
| Implementation-only Rust change (no `#[uniffi::export]` signature changes) | `scripts/rebuild-xcframework.sh` |

`regen-bindings.sh` regenerates `gv_ffi.swift` + headers, then rebuilds the xcframework. `rebuild-xcframework.sh` skips binding regeneration. **When in doubt, use `regen-bindings.sh`** — a mismatch between the Swift bindings and the compiled library causes a fatal crash at launch.

## Verifying the Swift Build

Run from `swift-app/` — use `-target`, not `-scheme` (scheme + destination hits a platform-resolution bug):

```bash
# iOS
xcodebuild -project Gainzville.xcodeproj -target Gainzville build CONFIGURATION=Debug 2>&1 | grep -E '(error:|BUILD SUCCEEDED|BUILD FAILED)'

# macOS
xcodebuild -project Gainzville.xcodeproj -target 'Gainzville macOS' build CONFIGURATION=Debug 2>&1 | grep -E '(error:|BUILD SUCCEEDED|BUILD FAILED)'
```

SourceKit LSP will show false "Cannot find type" errors for FFI types (`FfiEntry`, `FfiTemporal`, etc.) and design tokens (`GvSpacing`, `Color.gvSurface`). These are noise — trust `xcodebuild` output for real errors.

## Future / Open Questions

- **Unset controls**: no UI yet to clear individual temporal values (start, end, duration).
- **Inline duration field (macOS)**: current stepper popover is a placeholder; long-term goal is an inline `hh:mm:ss` text field with a custom formatter.
- **Sync / offline**: the Swift app is read/write via FFI today; full offline-first sync behaviour is driven by the Rust `client` crate.
