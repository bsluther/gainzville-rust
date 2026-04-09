# Swift Setup: Building and Consuming gv-ffi

This document covers the Rust build steps and Xcode project configuration needed to consume `gv-ffi` from a Swift iOS/macOS app. All steps assume macOS with Xcode installed.

---

## Prerequisites

```sh
# Rust toolchain (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# iOS device and Apple Silicon simulator targets
rustup target add aarch64-apple-ios
rustup target add aarch64-apple-ios-sim
```

No Postgres or special environment variables are needed — the SQLite layer uses runtime sqlx APIs and compiles without a live database.

---

## Building the library

### Release builds for each target

```sh
# From the workspace root:
cargo build --release --target aarch64-apple-ios        # physical device
cargo build --release --target aarch64-apple-ios-sim    # Apple Silicon simulator
```

Outputs:
```
target/aarch64-apple-ios/release/libgv_ffi.a
target/aarch64-apple-ios-sim/release/libgv_ffi.a
```

### Create the XCFramework

```sh
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libgv_ffi.a \
    -headers gv-ffi/bindings/ \
  -library target/aarch64-apple-ios-sim/release/libgv_ffi.a \
    -headers gv-ffi/bindings/ \
  -output GvFfi.xcframework
```

This produces `GvFfi.xcframework` containing slices for the physical device and Apple Silicon simulator. Xcode selects the correct slice automatically at build time.

### Regenerate Swift bindings (after API changes)

```sh
# Build a local debug dylib first:
cargo build

# Then generate:
cargo run --bin uniffi-bindgen -- generate \
  --library target/debug/libgv_ffi.dylib \
  --language swift \
  --out-dir gv-ffi/bindings/
```

Commit the updated files in `gv-ffi/bindings/` alongside any API changes.

---

## Alternative: cargo-swift (automates the above)

[cargo-swift](https://github.com/antoniusnaumann/cargo-swift) wraps the build and XCFramework steps into a single command and generates a `Package.swift` for Swift Package Manager distribution.

```sh
cargo install cargo-swift

# From gv-ffi/:
cargo swift package -p ios -n GvFfi
```

This produces a ready-to-use Swift package directory. Useful once the API stabilises.

---

## Xcode project setup

### Option A: Direct XCFramework (simplest for now)

> **Important:** Xcode tracks files explicitly in `project.pbxproj` — dropping files into the directory on disk is not enough. All files must be added through the Xcode GUI or they will be invisible to the build system. Use **File → New → File** to create new files, or drag existing files into the navigator to register them.

1. Create a `Frameworks/` group in the project navigator (right-click the project root → New Group).
2. Drag `GvFfi.xcframework` into the `Frameworks/` group. When prompted, check "Copy items if needed" and select the app target.
3. In **Build Phases → Link Binary With Libraries**, confirm `GvFfi.xcframework` is listed.
4. Add `gv-ffi/bindings/gv_ffi.swift` to the app target by dragging it into the source group in the navigator.
5. Drag `gv-ffi/bindings/gv_ffiFFI.h` and `gv-ffi/bindings/gv_ffiFFI.modulemap` into the `Frameworks/` group (no target membership needed).
6. Create a bridging header via **File → New → File → Other → Empty**, name it `GainzvilleBridgingHeader.h`, and add it to the app target. Contents:
   ```c
   #include "gv_ffiFFI.h"
   ```
7. In **Build Settings**, search for "Objective-C Bridging Header" (set filter to "All" if not visible). Set it to:
   ```
   Gainzville/GainzvilleBridgingHeader.h
   ```

**Why the bridging header?** `gv_ffi.swift` uses C types (`RustBuffer`, `RustCallStatus`, etc.) from `gv_ffiFFI.h` via a conditional `#if canImport(gv_ffiFFI)` block. For a static library XCFramework, Xcode does not automatically expose the bundled headers as a Swift module, so `canImport` returns false. The bridging header is what makes those C types visible to Swift.

### Option B: Swift Package Manager

If using `cargo-swift`, add the generated package as a local package dependency in Xcode:
**File → Add Package Dependencies → Add Local…** and point to the generated package directory.

---

## Basic usage

```swift
import Foundation

// 1. Implement the change-notification protocol.
class MyListener: CoreListener {
    weak var viewModel: ActivitiesViewModel?

    func onDataChanged() {
        Task { @MainActor in
            viewModel?.refresh()
        }
    }
}

// 2. Initialise the core (once, at app startup).
let dbURL = FileManager.default
    .urls(for: .documentDirectory, in: .userDomainMask)[0]
    .appendingPathComponent("gainzville.sqlite")

let listener = MyListener()
let core = try! GainzvilleCore(
    dbPath: "sqlite://\(dbURL.path)",
    actorId: myUser.actorId.uuidString,
    listener: listener
)
listener.viewModel = activitiesViewModel

// 3. Read data.
@Observable @MainActor
class ActivitiesViewModel {
    var activities: [FfiActivity] = []

    func refresh() {
        activities = core.getActivities()
    }
}

// 4. Write data.
let newActivity = FfiCreateActivity(
    id: UUID().uuidString,
    name: "Pull Up",
    description: nil
)
try core.runAction(action: .createActivity(newActivity))
// onDataChanged() fires → refresh() is called → activities updates → SwiftUI re-renders
```

---

## SQLite path format

Pass the database path as a full SQLite connection string:

| Scenario | Path string |
|---|---|
| File on disk | `"sqlite:///absolute/path/to/db.sqlite"` |
| In-memory (testing) | `"sqlite::memory:"` |
| Relative (not recommended) | `"sqlite://relative/path.sqlite"` |

For iOS, use `FileManager` to resolve the Documents or Application Support directory and pass the absolute path.

---

## Simulator vs device

The XCFramework contains separate slices for device (`aarch64-apple-ios`) and simulator (`aarch64-apple-ios-sim`). Xcode picks the right one automatically — no manual switching needed.

---

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| Crash on first call with "no runtime" panic | A `GainzvilleCore` method was called before `init` completed — shouldn't happen with `try!`/`try` init |
| `FfiError.generic` returned | Check the message string; it will contain the underlying `DomainError` description |
| Swift file not found after regenerating bindings | The generated file is always named `gv_ffi.swift`; check Xcode has it in the target's "Compile Sources" build phase |
| Linker errors about missing symbols | Ensure `GvFfi.xcframework` is in "Link Binary With Libraries" *and* "Embed Frameworks" |
