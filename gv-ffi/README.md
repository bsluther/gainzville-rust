# gv-ffi

UniFFI bridge crate that exposes the Gainzville Rust core to Swift. Zero business logic lives here — it is a thin synchronous wrapper around `gv_sqlite::SqliteClient`.

## Regenerating Swift bindings

Run this any time the exported API changes (from workspace root):

```sh
# 1. Build a local debug dylib
cargo build

# 2. Generate bindings to staging dir
cargo run --bin uniffi-bindgen -- generate \
  --library target/debug/libgv_ffi.dylib \
  --language swift \
  --out-dir gv-ffi/bindings/

# 3. Copy each file to its destination in swift-app
cp gv-ffi/bindings/gv_ffi.swift          swift-app/Gainzville/gv_ffi.swift
cp gv-ffi/bindings/gv_ffiFFI.h           swift-app/Frameworks/gv_ffiFFI.h
cp gv-ffi/bindings/gv_ffiFFI.modulemap   swift-app/Frameworks/gv_ffiFFI.modulemap
```

## Rebuilding the XCFramework

Run this any time the Rust implementation changes (from workspace root):

```sh
cargo build --release --target aarch64-apple-ios
cargo build --release --target aarch64-apple-ios-sim

rm -rf swift-app/Frameworks/GvFfi.xcframework
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libgv_ffi.a \
    -headers gv-ffi/bindings/ \
  -library target/aarch64-apple-ios-sim/release/libgv_ffi.a \
    -headers gv-ffi/bindings/ \
  -output swift-app/Frameworks/GvFfi.xcframework
```

Note: after replacing the XCFramework, Xcode picks up the change automatically on the next build.

## Architecture

Swift calls into Rust from its own thread pool with no tokio context. The bridge solves this with a single static multi-threaded tokio runtime (`LazyLock<Runtime>`). All `#[uniffi::export]` methods are **synchronous** at the FFI boundary; async work is driven internally via `RUNTIME.block_on(...)`. This is the same pattern used by Element X (Matrix), Mozilla Firefox, and Bitwarden.

```
Swift (any thread)
    │  synchronous call
    ▼
GainzvilleCore (uniffi::Object)
    │  RUNTIME.block_on(...)
    ▼
SqliteClient  ──►  SQLite (tokio + sqlx)
    │
    └──► listener.on_data_changed()  ──►  Swift callback
```

## API surface

```swift
// Initialise with path to SQLite file and a change-notification listener.
GainzvilleCore(dbPath: String, actorId: String, listener: CoreListener)

// Write — fire and forget; triggers on_data_changed() on success.
func runAction(action: FfiAction) throws

// Read — synchronous snapshot, no DB round-trip cache yet.
func getActivities() -> [FfiActivity]
```

Swift implements the `CoreListener` protocol to receive change notifications:

```swift
func onDataChanged()   // called after every successful runAction
```

## FFI types

| Rust type     | Swift type        | Notes                                      |
|---------------|-------------------|--------------------------------------------|
| `FfiActivity` | `struct FfiActivity` | `Equatable`, `Hashable`, memberwise init |
| `FfiAction`   | `enum FfiAction`  | Currently: `.createActivity(FfiCreateActivity)` |
| `FfiError`    | `enum FfiError: Swift.Error` | `.generic(message: String)`     |

All `Uuid` fields are `String` at the boundary. `DateTime<Utc>` will be `Int64` (ms) when added.

## SQLX offline mode

Not required. `gv_sqlite` uses the runtime sqlx APIs (`sqlx::query_as::<_, T>(...)`) rather than compile-time checked macros, so no `.sqlx/` cache is needed for iOS cross-compilation.

## Current limitations / open work

- **Read path is live SQLite** — `getActivities()` re-queries the database on every call. The planned `AppState` in-memory cache (see `docs/swift-architecture/design.md`) will make reads allocation-free and eliminate DB round-trips.
- **Action coverage** — only `CreateActivity` is wired up as a PoC. Remaining `Action` variants need corresponding `FfiAction` cases and conversion logic in `types.rs`.
- **`DomainError::Database(sqlx::Error)`** — core's error type wraps a sqlx type, violating the "no sqlx in core public API" principle. The FFI boundary absorbs this by converting to `FfiError::Generic(String)`, but the underlying issue should be fixed in `core`.
- **No `Temporal` / `Position` in FFI types** — `Entry` is not yet exposed; the full `FfiEntry` / `FfiEntryJoin` types need design decisions about how `Temporal` (a rich enum) maps across the boundary.
