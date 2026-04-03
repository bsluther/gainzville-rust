# Ghostty's architecture as a blueprint for Rust + SwiftUI apps

Mitchell Hashimoto's Ghostty terminal emulator implements one of the most sophisticated examples of a **native core library + platform UI shell** architecture in production today. Its Zig core compiles to a C-ABI static library consumed by a Swift/AppKit/SwiftUI macOS frontend and a GTK4 Linux frontend, with **over 90% of code shared** across platforms. The critical pattern for a Rust + UniFFI + SwiftUI app is Ghostty's event loop reconciliation: the platform owns `main()` and periodically calls a `tick()` function that drains a lock-free mailbox, while the core communicates back via C function pointer callbacks — a pattern directly applicable to bridging tokio and SwiftUI.

Ghostty has been rewritten five times across its GUI layer, and Mitchell's hard-won lessons about where to draw the core/UI boundary, how to manage threading across languages, and when to embrace platform idioms over cross-platform abstractions provide a practical engineering philosophy for anyone building a similar architecture with Rust.

## The core owns logic, the platform owns presentation

Ghostty's architecture splits cleanly along one principle: **libghostty handles everything that doesn't require platform-specific UI APIs**. The core encompasses terminal emulation, VT sequence parsing, font shaping, GPU rendering logic, PTY management, configuration parsing, and keyboard encoding. The platform layer (called "apprt" — application runtime) handles window creation, tabs, splits, menus, input method integration, and OS-specific features like Quick Look and secure input on macOS.

This split is implemented through Zig's compile-time interfaces. The `src/apprt.zig` file resolves at compile time to one of three implementations: `gtk` for Linux (Zig calling GTK4's C API), `embedded` for library mode (exports C API for Swift), or `glfw` for development. Zero runtime dispatch overhead — the compiler only analyzes the selected code path. The equivalent in Rust would be feature-gated modules or trait objects, though UniFFI adds its own abstraction layer.

The key insight from Mitchell's experience is what *not* to put in the core. He describes Ghostty as a **"reference implementation" of a libghostty consumer**, stating that "libghostty is the actual goal of the project" and the macOS/Linux apps are "flagship tech demos." This framing clarifies the boundary: the core is a reusable library with no opinions about windowing, and each platform consumer implements its own UI using native toolkits. For a Rust + SwiftUI app, this means the Rust crate should be a library crate with `crate-type = ["staticlib", "cdylib"]` that exposes domain operations — not UI state management, not navigation, not view models.

Mitchell's stated rationale for native frontends over cross-platform UI is unequivocal: "A lot of what makes a platform 'native' (particularly on macOS) dictates a certain tech stack. It's very hard to build a truly native application on macOS without using Swift." He found that achieving native tabs, native fullscreen, input method support, and system integrations required embracing each platform's idioms fully. This conviction was validated through **five complete GUI rewrites** — GLFW, pure SwiftUI, AppKit+SwiftUI, procedural GTK, and finally GObject-based GTK.

## The C FFI boundary uses opaque pointers and callback structs

Ghostty's FFI boundary lives in three files: `src/main_c.zig` (C function exports), `include/ghostty.h` (hand-written C header, ~1000+ lines), and `include/module.modulemap` (Swift module map). The pattern is classic C library design with opaque handles.

All core types cross the boundary as **opaque pointers**. Swift never sees the internal structure of `ghostty_app_t`, `ghostty_surface_t`, or `ghostty_config_t`. The lifecycle follows paired create/destroy functions: `ghostty_app_new()` / `ghostty_app_free()`, `ghostty_surface_new()` / `ghostty_surface_free()`. Memory ownership is unambiguous — **Zig allocates, Zig frees**, and the host calls the appropriate `_free()` function when done. String data passed via callbacks (like terminal titles) is borrowed for the duration of the callback; Swift must copy to retain.

The most architecturally significant element is the **`Options` callback struct** defined in `src/apprt/embedded.zig`. When Swift calls `ghostty_app_new()`, it passes a struct populated with C function pointers:

```c
typedef struct {
    void (*new_surface)(void* userdata, ...);
    void (*close_surface)(void* userdata, ...);
    void (*set_title)(void* userdata, const char*);
    void (*ring_bell)(void* userdata);
    void* userdata;  // typically Unmanaged<AppDelegate>.toOpaque()
} ghostty_app_options_t;
```

Each callback receives an opaque `userdata` pointer — the Swift object bridged via `Unmanaged<T>.toOpaque()`. This is how the Zig core requests UI operations without knowing anything about Swift. The pattern eliminates bidirectional compile-time dependencies: Zig compiles with no knowledge of Swift, and Swift consumes the C header with no knowledge of Zig internals.

For a Rust + UniFFI app, UniFFI replaces this entire manual layer. UniFFI's `#[uniffi::export(with_foreign)]` trait callbacks serve the same purpose as Ghostty's function pointer struct, but with generated type-safe Swift protocols instead of raw `void*` pointers. The tradeoff is clear: Ghostty's approach gives **zero serialization overhead** and complete control over the boundary, while UniFFI provides **automatic type mapping, memory safety, and multi-language support** at the cost of byte-buffer serialization for complex types.

## The tick pattern reconciles two event loops

This is the most important architectural decision in Ghostty, and the one most directly applicable to the tokio + SwiftUI challenge. Ghostty runs **3–4 threads per terminal surface**, and the core does *not* attempt to run its own event loop on the main thread.

**Swift owns `main()` and the platform event loop.** The Zig core never fights for control of the main thread. Instead, Swift periodically calls `ghostty_app_tick()`, which processes the Zig-side mailbox on the main thread. This is cooperative scheduling — Zig piggybacks on the platform's event loop. The I/O thread and renderer thread run their own internal loops (libxev for I/O, timer-based for rendering), but these are entirely encapsulated within the core and invisible to Swift.

Threads communicate via **lock-free `BlockingQueue` mailboxes**. When the I/O thread (which handles PTY reads and VT parsing) needs the main thread to perform a UI operation — changing the window title, requesting a new tab — it enqueues a message into the App mailbox. The next `ghostty_app_tick()` call drains this queue and invokes the appropriate Swift callback. The flow is:

1. Background thread detects state change (e.g., shell outputs title escape sequence)
2. Message enqueued into lock-free mailbox
3. Background thread signals wakeup to main thread (mach port or similar)
4. Swift event loop wakes, calls `ghostty_app_tick()`
5. Zig drains mailbox, calls Swift callback function pointer
6. Swift updates `@Observable` model → SwiftUI re-renders

In the opposite direction, Swift calls exported C functions directly on the main thread (`ghostty_surface_key()` for keyboard input, `ghostty_surface_mouse_*()` for mouse events). These either execute immediately for main-thread-safe operations or enqueue messages to background thread mailboxes.

**For a Rust + UniFFI + tokio app, the mapping is direct.** UniFFI's async support eliminates the need for a manual tick function in many cases — `async fn` exports bridge tokio futures to Swift `async`/`await` natively. But for push-based state updates (where the Rust side initiates changes), you still need Ghostty's callback pattern. The recommended architecture:

- Initialize a tokio runtime once (via `OnceCell<Runtime>` or similar), shared across all UniFFI objects
- Use `#[uniffi::export(async_runtime = "tokio")]` for request/response operations (database queries, network calls)
- Use `#[uniffi::export(with_foreign)]` trait callbacks for Rust → Swift push notifications (equivalent to Ghostty's callback struct)
- For high-frequency state changes, consider a `tick()` pattern with a `crossbeam` channel rather than invoking callbacks on every change

The renderer state in Ghostty uses a **dedicated mutex** with a deadlock-prevention pattern: the stream handler temporarily releases the renderer mutex if the mailbox is full, preventing priority inversion between the I/O and renderer threads. This level of fine-grained synchronization isn't typically needed in CRUD apps, but the principle applies — design your Rust core's concurrency so that no lock can block the main thread.

## SwiftUI wraps AppKit, not the other way around

Ghostty's macOS frontend evolved from pure SwiftUI to a hybrid AppKit+SwiftUI architecture, and this evolution contains the most practical lessons for iOS/macOS app developers.

The terminal rendering surface is an **`NSView` subclass** (`SurfaceView_AppKit.swift`) backed by a `CAMetalLayer`. This view holds a `ghostty_surface_t?` handle, implements `NSTextInputClient` for IME support, and forwards all input events to the Zig core via C FFI calls. It's wrapped for SwiftUI consumption via `NSViewRepresentable`.

The critical evolution was in state management. The split pane hierarchy was originally **"all-in on SwiftUI"** with extensive use of bindings, publishers, and `@ObservedObject`. This led to what the team called **"change notification soup"** — complex chains of state updates that were hard to reason about. In a major refactor (PR #7523, June 2025), they moved to an architecture where **SwiftUI handles views while AppKit handles data and business logic**.

The current pattern uses a **generic, immutable value-type `SplitTree<V>`**. Every state change produces an entirely new tree value. SwiftUI views are pure renderers of this static tree — they contain no logic. All business logic (new split, close, move, spatial navigation) lives in `BaseTerminalController` on the AppKit side. This eliminated the complexity of incremental state updates in favor of simple replacement.

For Zig → Swift state flow, the pattern is: Zig calls C function pointer callback → Swift handler executes → `NotificationCenter.post()` or `@Observable` property mutation → SwiftUI view re-renders. The `@Observable` macro (Swift 5.9+) provides fine-grained observation without the boilerplate of `ObservableObject` / `@Published`.

For a Rust + UniFFI + SwiftUI app, the applicable patterns are:

- **Export domain state as UniFFI `Record` types** (value types in Swift). Replace the entire state rather than trying to do incremental updates across the FFI boundary.
- **Keep view models in Swift**, not in Rust. The Rust core provides data and operations; Swift composes these into view-ready models on `@MainActor`.
- **Use `@Observable` classes** that hold UniFFI-generated types and call async UniFFI methods. UniFFI's async support maps cleanly to Swift's `Task {}` pattern in SwiftUI.
- **Avoid deep SwiftUI state trees** that depend on cross-FFI bindings. Ghostty learned this the hard way — flatten your cross-boundary state into simple value types.

## The build pipeline chains Zig → static lib → XCFramework → Xcode

Ghostty's build system is driven entirely by `build.zig`, which orchestrates a multi-step pipeline for macOS:

1. Zig compiles `libghostty.a` for aarch64 and x86_64
2. `libtool` merges all static library dependencies into one `.a`
3. `lipo` creates a universal binary from both architectures
4. A custom build step packages the universal library + `include/ghostty.h` + `module.modulemap` into `GhosttyKit.xcframework`
5. `xcodebuild` builds the Swift app against this XCFramework

For Rust, the equivalent pipeline would be: `cargo build --target aarch64-apple-ios` (and `x86_64-apple-ios-sim` for simulator) → `lipo` → XCFramework. With UniFFI, add `uniffi-bindgen generate` to produce the Swift bindings and header. Several tools automate this: `cargo-xcode`, `swift-bridge`, or manual build scripts. The XCFramework approach is the same — Ghostty validates that this pipeline works reliably at scale.

Notable build detail: Ghostty's `module.modulemap` is just three lines (`module GhosttyKit { umbrella header "ghostty.h" export * }`), which enables `import GhosttyKit` in Swift. UniFFI generates equivalent modulemap files automatically.

## Five GUI rewrites and the philosophy they produced

Mitchell Hashimoto has been remarkably transparent about Ghostty's architectural evolution, and his lessons generalize beyond terminal emulators.

**"You can't avoid the platform's type system."** The GTK rewrite (August 2025) was prompted by a persistent class of bugs where "the Zig memory or the GTK memory has been freed, but not both." The original approach tried to avoid GObject's type system, leading to lifecycle mismatches. The lesson: if you choose a platform framework, embrace its idioms fully rather than fighting them. For SwiftUI + Rust, this means letting Swift own view lifecycle, navigation, and state observation natively rather than trying to drive these from Rust.

**"Ghostty is 70% a font rendering engine."** About 70% of development time went into font rendering, not terminal emulation. The core's value isn't in the obvious feature (VT parsing) but in the hard, platform-specific work (font shaping, GPU rendering). For a Rust iOS app, the lesson is to put the hard, reusable logic in Rust (data sync, business rules, offline-first logic) and accept that the platform-specific UI work will dominate development time.

**Running Valgrind on the GTK rewrite** revealed that "our Zig codebase had one leak and one undefined memory access... All other memory issues revolved around C API boundaries." Memory bugs cluster at FFI boundaries. UniFFI mitigates this significantly by managing memory automatically (Arc-wrapping objects, copying data across the boundary), but the principle remains: **test your FFI boundary exhaustively**, especially lifecycle management.

## Ghostty uses no database — state persistence delegates to the platform

Ghostty does not use SQLite or any database. Configuration is stored in plain text files at `~/Library/Application Support/com.mitchellh.ghostty/config` on macOS. Window state persistence uses macOS's native `NSQuitAlwaysKeepsWindows` mechanism, which saves/restores window positions via `~/Library/Saved Application State/`. Mitchell explicitly confirmed that "Ghostty is using a native feature for this" with no custom persistence layer.

For a Rust + SwiftUI app with SQLite, Ghostty's architecture still provides guidance: **keep the database entirely in the Rust core**. Never expose database connections, query builders, or raw SQL across the FFI boundary. Export domain operations (`fetch_items()`, `create_item()`) as UniFFI async methods. The database pool (`sqlx::SqlitePool` or `rusqlite::Connection` wrapped in a mutex) lives as a field on an `#[derive(uniffi::Object)]` struct. This mirrors Ghostty's principle that the core owns all complex state — the platform layer only sees high-level operations and result types.

## Ghostty's raw FFI versus UniFFI: when each wins

Ghostty's manual C FFI approach gives **zero serialization overhead, complete threading control, and direct GPU/Metal integration**. This matters for a terminal emulator rendering at 60+ fps with font shaping on every frame. UniFFI's byte-buffer serialization (which one benchmark clocked at up to 1000x overhead for high-frequency micro-calls versus zero-copy) would be unacceptable for this use case.

For a CRUD-oriented iOS app with SQLite and tokio, **UniFFI is the clear choice**. The serialization overhead is negligible for database query results and UI state updates. What UniFFI provides that Ghostty's approach does not:

- **Automatic Swift type generation** — enums with associated data, structs, error types, async functions all map to idiomatic Swift without maintaining a C header
- **Memory safety** — Arc-wrapping objects, automatic reference counting, no manual `Unmanaged<T>.toOpaque()` bridging
- **Native async/await** — tokio futures become Swift `async` functions transparently
- **Multi-language support** — the same Rust core generates Kotlin bindings for Android with zero additional work

What Ghostty's approach provides that UniFFI doesn't:

- **Zero-copy data passing** for large buffers
- **Custom threading models** unconstrained by `Sync + Send` requirements
- **Fine-grained lifecycle control** — no Arc overhead, explicit allocate/free
- **No generated code layer** — simpler debugging, full transparency at the boundary

The structural patterns from Ghostty that apply regardless of FFI tooling: **opaque handles for core objects** (UniFFI `Object` types), **callback interfaces for core → platform communication** (UniFFI `with_foreign` traits), **the tick/mailbox pattern for event loop reconciliation** (adaptable via `crossbeam` channels), and **value-type state transfer** (UniFFI `Record` types). Ghostty's *architecture* is language-agnostic; only the FFI mechanism differs.

## Conclusion: the actionable blueprint

Ghostty validates a specific architectural pattern at scale with millions of daily users: **a core library that owns all complex state and logic, communicating with a thin native UI shell through a well-defined boundary of opaque handles, async operations, and push callbacks**. For a Rust core + SwiftUI iOS app using UniFFI with sqlx and tokio, the concrete implementation is:

Initialize a single tokio runtime in a `OnceCell`, shared across all UniFFI-exported objects. Export a small set of domain service objects (`#[derive(uniffi::Object)]`) with async methods annotated `#[uniffi::export(async_runtime = "tokio")]`. Transfer data as UniFFI `Record` types (value-copied, matching Ghostty's approach of giving each Surface a `DerivedConfig` copy to avoid shared-pointer lifetime issues). Use `#[uniffi::export(with_foreign)]` trait callbacks for Rust-initiated state changes — the equivalent of Ghostty's `Options` callback struct. Keep SQLite entirely inside the Rust core, exporting only domain operations. Let Swift own all view lifecycle, navigation, and state observation using `@Observable` classes that wrap UniFFI types. And most importantly, embrace the lesson Mitchell learned through five rewrites: **don't fight the platform's idioms**. SwiftUI should feel like SwiftUI. The Rust core is invisible infrastructure.