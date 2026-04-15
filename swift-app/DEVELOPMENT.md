# Swift App — Development Notes

## Project structure

The Xcode project has two app targets that share the same `Gainzville/` source folder:

| Target | SDK | Deployment target |
|--------|-----|-------------------|
| `Gainzville` | iphoneos | iOS 26.2 |
| `Gainzville macOS` | macosx | macOS 26.0 |

Source sharing works via Xcode's `PBXFileSystemSynchronizedRootGroup`: both targets point at the `Gainzville/` directory and Xcode auto-discovers every `.swift` file in the tree. Any new file added to `Gainzville/` is automatically included in both targets — no manual project file edits needed for normal development. Platform-specific code uses `#if os(iOS)` / `#if os(macOS)`.

The macOS target is a **native macOS app**, not Mac Catalyst. This gives proper macOS navigation patterns (`NavigationSplitView`, menu bar, etc.) rather than an iPad-app-on-Mac adaptation.

---

## Swift version pinned to 5.0

Both targets use `SWIFT_VERSION = 5.0` even though we'd prefer Swift 6 strict concurrency. The blocker is the generated `gv_ffi.swift` file produced by UniFFI 0.31.

**What breaks:** The project sets `SWIFT_DEFAULT_ACTOR_ISOLATION = MainActor`, which makes every declaration `@MainActor` by default. In the generated file, `rustCall` becomes a `@MainActor` global function. Some of its callers in `gv_ffi.swift` are in synchronous, non-isolated contexts, which Swift 6 rejects as a hard error:

```
error: call to main actor-isolated global function 'rustCall' in a synchronous nonisolated context
```

In Swift 5 mode, the same situation produces a warning (or nothing), so the build passes.

**Settings still active** (work fine in Swift 5 mode):
- `SWIFT_DEFAULT_ACTOR_ISOLATION = MainActor` — helps our application code default to main-actor isolation
- `SWIFT_APPROACHABLE_CONCURRENCY = YES` — friendlier concurrency diagnostics

**Path to Swift 6:** Either upgrade to a version of UniFFI that generates Swift 6 compatible bindings, or annotate `rustCall` and its callers with `nonisolated` in the generated file (fragile since it's regenerated). Track UniFFI's Swift 6 support in their changelog.

---

## macOS-only link dependency: SystemConfiguration.framework

The macOS target requires an extra framework that iOS does not:

```
OTHER_LDFLAGS = "-framework SystemConfiguration"
```

**Why:** The `whoami` crate (pulled in transitively by `gv_client`) calls `SCDynamicStoreCopyComputerName` to get the device/computer name on macOS. This symbol lives in `SystemConfiguration.framework`. On iOS, `whoami` uses different platform APIs and the framework isn't needed.

This is set in the macOS target's build settings in `project.pbxproj` (both Debug and Release configurations). If you see a linker error about `_SCDynamicStoreCopyComputerName` after a clean build, this is why.

---

## GvFfi.xcframework

The XCFramework bundles three static library slices:

| Slice | Rust target(s) | Use |
|-------|----------------|-----|
| `ios-arm64` | `aarch64-apple-ios` | Physical iOS/iPadOS devices |
| `ios-arm64-simulator` | `aarch64-apple-ios-sim` | iOS Simulator (Apple Silicon Mac) |
| `macos-arm64_x86_64` | `aarch64-apple-darwin` + `x86_64-apple-darwin` via `lipo` | macOS (Apple Silicon + Intel) |

The `macos-arm64_x86_64` slice is a fat binary created with `lipo -create`. Intel Mac support (`x86_64-apple-darwin`) is included so the framework works on Intel Macs and Rosetta environments.

**Rebuild scripts** (run from workspace root):

- `scripts/regen-bindings.sh` — use when the FFI API changed (new query types, new actions, type changes). Regenerates `gv_ffi.swift` + headers from the compiled dylib metadata, then rebuilds the XCFramework. **Required** when `#[uniffi::export]` signatures, `FfiAction`, or `types.rs` change — UniFFI embeds interface checksums in both the Swift bindings and the compiled library; a mismatch causes a fatal crash at launch.
- `scripts/rebuild-xcframework.sh` — use for internal Rust changes only (no FFI surface changes). Skips binding regeneration.

---

## SQLite database path

`Core.swift` resolves the database path using `FileManager.documentDirectory`, which is cross-platform:

```swift
FileManager.default
    .urls(for: .documentDirectory, in: .userDomainMask)[0]
    .appendingPathComponent("gainzville.sqlite")
```

The file is pre-created if missing before passing the path to `GainzvilleCore` because `sqlx` defaults to `create_if_missing = false` and will error on a non-existent file.

Effective paths:
- **iOS/iPadOS**: `<app sandbox>/Documents/gainzville.sqlite`
- **macOS**: `~/Documents/gainzville.sqlite` (not sandboxed in development builds)

---

## Hardcoded actorId

`Core.swift` currently uses a hardcoded UUID as the actor ID:

```swift
actorId: "eee9e6ae-6531-4580-8356-427604a0dc02"
```

This is a placeholder. When auth is implemented, this should be replaced with a real per-user/per-device actor ID persisted in the Keychain or derived from the auth token.

---

## Bridging header

`GainzvilleBridgingHeader.h` includes `gv_ffiFFI.h` (the C header generated by UniFFI). Both targets use:

```
SWIFT_OBJC_BRIDGING_HEADER = "$(SRCROOT)/Gainzville/GainzvilleBridgingHeader.h"
SWIFT_INCLUDE_PATHS = "$(SRCROOT)/Frameworks"
```

The `SWIFT_INCLUDE_PATHS` setting lets the compiler find `gv_ffiFFI.modulemap` (which declares the `GainzvilleFFI` Clang module imported by the generated Swift file). These files are in `Frameworks/` and are regenerated by `regen-bindings.sh`.
