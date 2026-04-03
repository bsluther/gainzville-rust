# Production Rust core with SwiftUI via UniFFI

**The most important finding: drop sqlx and tokio from your FFI boundary.** Every production app shipping Rust+SQLite on iOS—Mozilla Firefox, Element X (Matrix), Bitwarden—uses **rusqlite with the `bundled` feature**, not sqlx. This single architectural decision eliminates the hardest problem (tokio runtime management across FFI) and aligns Gainzville with battle-tested patterns running on hundreds of millions of devices. UniFFI v0.31.0 with proc macros is the clear production choice for the bridge layer, and SwiftUI's enormous training data advantage makes it substantially better served by agentic coding tools than Dioxus.

## The architecture that production apps actually use

The recommended structure separates pure Rust business logic from the FFI boundary layer. A **bridge crate** pattern keeps your core library free of UniFFI concerns while exposing a clean Swift API. This is the pattern used by Element X (Matrix), Bitwarden, and the typester.dev auth2 app.

```
gainzville/
├── Cargo.toml                    # Workspace root
├── rust/
│   ├── core/                     # Pure Rust: domain logic, rusqlite, no FFI awareness
│   ├── ffi/                      # UniFFI bridge crate: wraps core types for Swift
│   └── uniffi-bindgen/           # Tiny CLI crate for binding generation
├── apple/
│   ├── GainzvilleApp/            # SwiftUI app
│   └── Sources/UniFFI/           # Generated Swift bindings
├── Package.swift                 # SPM package for the Rust library
└── build-ios.sh                  # Build script for XCFramework
```

**UniFFI v0.31.0** (released January 2026) is the current version with **4,400+ GitHub stars** and production deployments at Mozilla, Android AOSP, Bitwarden, Proton, and Element/Matrix. The proc-macro approach is now strongly preferred over UDL files. Key decorators: `#[derive(uniffi::Record)]` for value types (Swift structs), `#[derive(uniffi::Object)]` for reference types (Swift classes wrapped in `Arc`), `#[derive(uniffi::Enum)]` for enums, and `#[uniffi::export]` for functions and methods. Generated Swift code is reasonably idiomatic—objects become `class + protocol` pairs (enabling mocking), records become structs, and `Result<T, E>` maps to Swift `throws`.

**Critical breaking changes in v0.29.0** (February 2025): `UniffiCustomTypeConverter` trait was removed in favor of the `custom_type!()` macro, external types UDL syntax changed, and several macros were deleted. Any tutorials from 2024 or earlier likely use the old API.

## Why rusqlite replaces sqlx for iOS

Mozilla's application-services team—the most mature Rust+iOS+SQLite codebase in existence—explicitly uses **rusqlite**, not sqlx. Element X's matrix-rust-sdk uses rusqlite for all three storage layers (state, crypto, event cache). No production iOS app found in this research uses sqlx with SQLite.

The core problem with sqlx on iOS is architectural: sqlx's SQLite driver spawns a dedicated `sqlx-sqlite-worker` background thread and **requires the multi-threaded tokio scheduler**. Calling `spawn_blocking` (which sqlx uses internally) on a `current_thread` runtime panics. This means using sqlx forces you to manage a tokio `Runtime` across the FFI boundary—the single hardest integration challenge in this entire stack. For a local embedded database, async provides no meaningful benefit.

The recommended approach wraps a `rusqlite::Connection` in a `Mutex` inside a UniFFI-exported object:

```rust
#[derive(uniffi::Object)]
pub struct FitnessDatabase {
    conn: Mutex<rusqlite::Connection>,
}

#[uniffi::export]
impl FitnessDatabase {
    #[uniffi::constructor]
    pub fn new(db_path: String) -> Result<Self, DatabaseError> {
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}
```

Swift resolves the sandbox path and passes it across: `FileManager.default.url(for: .documentDirectory, ...)`. **WAL mode** is well-supported on iOS (default for Core Data since iOS 7) and enables concurrent reads. Mozilla's production guidance: "Always put SQLite into WAL mode, then have exactly 1 writer connection and as many reader connections as you need." Use `PRAGMA user_version` for schema migrations—the same pattern Firefox uses across hundreds of millions of installs. Bundled SQLite avoids cross-compilation linking issues and gives you control over the exact SQLite version (**3.50.2** in rusqlite 0.37.0).

**You cannot use both sqlx and rusqlite** in the same dependency graph—they both link `libsqlite3-sys`, and Cargo only allows one version of a native-linking crate.

## Handling tokio if you still need it

If Gainzville needs tokio for non-database async work (HTTP requests, background sync), UniFFI supports `async fn` that maps directly to **Swift `async/await`**. UniFFI's async model is foreign-runtime-driven: Swift's structured concurrency polls Rust futures. However, tokio-dependent code (reqwest, tokio channels, timers) requires an active tokio runtime context.

The production-proven pattern is a **global `LazyLock<Runtime>`** with conservative thread counts:

```rust
static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("gainzville-rt")
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime")
});
```

Bridge methods spawn onto this runtime: `RUNTIME.spawn(async move { ... }).await`. Element X (Matrix) uses exactly this pattern—`tokio` with `rt-multi-thread` and `uniffi` with the `tokio` feature. For their iOS Notification Service Extension (constrained to **16MB RAM**), they use a lightweight runtime with limited threads.

UniFFI provides an `async_runtime="tokio"` attribute that uses `async-compat` under the hood, but **this is unreliable**: GitHub issue #2576 reports it fails on trait implementations, and issue #1726 suggests the team may deprecate it. The explicit `LazyLock<Runtime>` + `spawn` pattern is safer.

**Cancellation is not natively supported.** You must build cancellation manually via `AtomicBool` flags or channels, exposing a `cancel()` method on your UniFFI objects. Swift 6 `Sendable` conformance for async generated code is still incomplete (tracked in issue #2448).

The strongest recommendation: **isolate tokio to a thin networking layer** and keep the database layer synchronous with rusqlite. This dramatically reduces the async surface area crossing the FFI boundary.

## Production apps prove this architecture works at scale

**Element X (Matrix)** is the closest analog to Gainzville's architecture. It ships a full SwiftUI app backed by a Rust SDK using UniFFI, tokio (`rt-multi-thread`), and rusqlite for SQLite storage. The `matrix-sdk-ffi` crate generates Swift bindings consumed via a Swift Package. Element contributed significantly to UniFFI's async and proc-macro support.

**Bitwarden** uses UniFFI in a monorepo structure (`bitwarden/sdk-internal`) with a dedicated `bitwarden-uniffi` crate generating mobile bindings. The iOS app consumes a pre-built Swift Package from a separate `sdk-swift` repo, with automated PRs for SDK version bumps.

**Proton Mail** rewrote their mobile apps in late 2025/early 2026 with **~80% shared Rust codebase** and native SwiftUI on iOS. Proton Pass uses UniFFI with UDL files. This represents perhaps the most aggressive Rust core adoption in a consumer app—Rust handles business logic, navigation state, and even infinite scrolling logic.

**Mozilla application-services** is the canonical UniFFI deployment, powering Firefox iOS components (bookmarks sync, history, logins, autofill, telemetry). They pioneered the **"megazord" pattern**: all Rust crates compiled into a single static library, packaged as an XCFramework, distributed via SPM with generated Swift sources alongside the binary.

**1Password** and **Signal** notably built custom FFI solutions rather than using UniFFI. 1Password uses Typeshare (JSON serialization across FFI), while Signal uses custom `#[bridge_fn]` macros with cbindgen. Both predate UniFFI's maturity. For a new project in 2026, UniFFI is the clear choice—it eliminates the need to build and maintain custom tooling.

| Project | FFI tool | Async runtime | SQLite library | Distribution |
|---------|----------|--------------|----------------|-------------|
| Element X (Matrix) | UniFFI | Tokio multi-thread | rusqlite | Swift Package |
| Bitwarden | UniFFI | — | — | Swift Package |
| Proton Pass | UniFFI (UDL) | — | — | Swift Package |
| Mozilla Firefox iOS | UniFFI | — | rusqlite | XCFramework + SPM |
| 1Password | Custom (Typeshare) | Tokio | Private | Closed source |
| Signal | Custom (bridge_fn) | — | — | CocoaPod |

## Build pipeline and Xcode integration

**Swift Package Manager with XCFramework** is the standard distribution pattern. The build pipeline compiles Rust for three iOS targets (`aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios`), generates Swift bindings with `uniffi-bindgen-swift`, creates a fat simulator binary with `lipo`, and packages everything into an XCFramework with `xcodebuild -create-xcframework`.

The Ferrostar team (1.5+ years in production) recommends a **shell script** over Xcode build phase integration: "Given the relative difficulty of doing this and the overall flakiness of the Xcode build process, we opted for a simple, reliable shell script." The `Package.swift` references the binary target locally during development and switches to a remote URL with checksum for releases.

**`cargo-swift`** (v0.3.1) automates this entire pipeline with `cargo swift package` but has low adoption (~1,177 downloads) and may not cover edge cases. For a production app, maintaining your own build script gives more control. The `uniffi-starter` template by the Ferrostar developer provides a production-ready starting point.

Build times for the initial Rust compilation can take minutes, but **incremental builds are fast**. Never run `cargo clean` in build scripts. Use `[profile.release]` with `opt-level = "z"` and `strip = true` to reduce binary size from the ~32MB baseline. A `rust-toolchain.toml` in the repo root ensures all developers have the correct targets installed via rustup.

## SwiftUI with agentic coding tools is a clear win over Dioxus

SwiftUI launched in 2019 and has **7+ years** of training data across millions of GitHub repos, Stack Overflow answers, and tutorials. Dioxus launched in January 2022 with a fraction of the corpus. This gap is not subtle—it fundamentally changes AI coding assistant effectiveness.

Claude is widely regarded as the best model for Swift/SwiftUI code generation. Cursor + SwiftUI is a popular pairing where developers write Swift in Cursor and compile in Xcode. GitHub Copilot has an official Xcode extension. Dedicated tools like Alex and Apple's own Swift Assist (Xcode 16+) exist. Multiple `.cursorrules` templates for SwiftUI are publicly available. **None of this infrastructure exists for Dioxus.**

Dioxus mobile support remains experimental. The framework's own docs describe `dioxus-mobile` as "a re-export of dioxus-desktop with some minor tweaks." GitHub discussions show users struggling to deploy to physical iOS devices. The RSX macro syntax is novel enough that LLMs frequently hallucinate incorrect code, and Rust's borrow checker creates additional friction for AI-generated code that needs to compile, not just look right.

The SwiftUI+UniFFI architecture lets you leverage AI tools where they're most effective (UI layer) while keeping core logic in Rust where correctness matters more than velocity. This is not a theoretical advantage—developers at TantalusPath, the typester.dev auth2 app, and Ferrostar all ship with this exact split.

## Conclusion

The production-ready path for Gainzville is clear: **UniFFI v0.31.0 with proc macros**, a separate bridge crate, **rusqlite (bundled)** instead of sqlx, and tokio isolated to networking only. This matches the architecture of Element X, Mozilla Firefox, and Bitwarden—apps serving tens to hundreds of millions of users.

The most counterintuitive finding is that **removing sqlx eliminates the hardest problem**. The async runtime management that causes the most pain in Rust+iOS integration becomes nearly irrelevant when your database layer is synchronous. If you need async for HTTP or sync operations, a global `LazyLock<Runtime>` with 2 worker threads is the proven pattern.

The tooling ecosystem has matured significantly since 2023. UniFFI's proc-macro approach eliminates UDL file maintenance, `uniffi-bindgen-swift` handles XCFramework-compatible binding generation, and SPM distribution is well-documented. The remaining rough edges—Swift 6 `Sendable` conformance, no native cancellation, binary size overhead—are known quantities with established workarounds. Proton Mail's 2026 rewrite achieving 80% Rust code sharing demonstrates this architecture scales to complex consumer apps, not just libraries.