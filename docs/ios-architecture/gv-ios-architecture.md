# Gainzville iOS Architecture: Decisions, Considerations, and Next Steps

> Companion to the research documents on **Rust core + SwiftUI via UniFFI** and
> **Ghostty architecture patterns**. This document records priorities, decisions
> reached or informed, open questions, and pointers for implementation. It assumes
> the reader has access to those research documents.


> This research was conducted without reference to the codebase. It is a valuable starting point but
> may omit important concerns.

---

## Framing

The goal is a **robust, production-grade** iOS app for Gainzville — not a proof of
concept. That framing shapes every tradeoff: invest in the things we know we'll want,
accept short-term complexity for long-term correctness, and treat the Rust core as the
long-lived strategic asset it is.

The guiding analogy throughout is **Ghostty**: Mitchell Hashimoto's terminal emulator
separates a Zig core library from a native SwiftUI/AppKit macOS frontend. The
structural patterns transfer directly even though the domain and languages differ.
Key differences to keep in mind when borrowing from Ghostty:

| Ghostty | Gainzville |
|---------|------------|
| Zig → C ABI → Swift (manual FFI) | Rust → UniFFI → Swift (generated bindings) |
| Rendering library (GPU, font, PTY) | Data/sync library (SQLite, server sync) |
| High-frequency callbacks (60fps render) | Low-frequency callbacks (cache invalidation) |
| No database | SQLite as source of truth |
| Single-user, local state | Multi-device sync with eventual consistency |

---

## Core Architectural Decisions

### 1. Rust core as library; Swift owns the platform

Swift owns `main()` and the platform event loop. The Rust core never fights for the
main thread. This is the Ghostty principle stated plainly, and it applies here
without modification.

**Decision: confirmed.** All platform concerns — navigation, view lifecycle, window
management, system integration — belong in Swift. The Rust core is invisible
infrastructure.

---

### 2. UniFFI as the FFI layer

UniFFI v0.31.0 with proc macros (`#[derive(uniffi::Object)]`, `#[derive(uniffi::Record)]`,
`#[uniffi::export]`) is the production-proven choice. It replaces the hand-written
C header + modulemap approach Ghostty uses.

**What UniFFI gives over raw C FFI:**
- Automatic Swift type generation (enums with associated data, async functions, error types)
- Memory safety via Arc-wrapping for objects; value-copy for records
- Native async/await bridging for tokio futures
- Android/Kotlin bindings for free (future-proofing for cross-platform)

**What it costs vs. raw C FFI:**
- Byte-buffer serialization overhead for complex types (negligible for CRUD data, matters for high-frequency large buffers — not a concern here)
- Less control over threading model at the boundary (mitigated by the async isolation strategy below)

**Key UniFFI concepts:**
- `Object` types: live on Rust heap, Arc-counted, Swift holds an opaque handle and calls methods into Rust
- `Record` types: fully serialized/copied at the boundary — Swift owns its copy independently
- `with_foreign` trait callbacks: Swift implements a Rust-defined protocol, used for Rust → Swift push notifications

---

### 3. The real async-over-FFI problem, and how to sidestep it

The pain of async across the FFI boundary is not that tokio exists inside the core.
It's specifically about **Swift awaiting a Rust future** — that path requires UniFFI's
async machinery, which has rough edges (issue #2576, incomplete Swift 6 Sendable
conformance) and forces `rt-multi-thread` for sqlx.

**The key insight:** if Swift never awaits anything on the hot path, those problems
largely disappear. Tokio can run `rt-multi-thread` entirely inside the core and
Swift doesn't care.

**Decision: isolate async to the core's internals.** The primary data flow is:

```
Rust (async/tokio internally) → in-memory cache → Swift reads synchronously
```

Swift initiates reads; Swift fires commands as fire-and-forget; Rust pushes
change notifications via a callback. No `async/await` on the call paths that
matter for UI performance.

---

### 4. sqlx vs. rusqlite — a nuanced decision

The original research recommended dropping sqlx for rusqlite. That recommendation
was targeted at **use case A**: Swift making direct, point-in-time database queries
synchronously across the FFI. In that model, sqlx's `rt-multi-thread` requirement
creates the FFI async pain.

**But use case A is not the primary model for Gainzville.** The primary model is
**use case B: reactive live queries**, where the Rust core maintains an in-memory
cache and Swift reads from it. In that model, the database driver is an internal
Rust concern — Swift never triggers a sqlx query directly.

| | Use case A (direct query) | Use case B (cache + reactive) |
|---|---|---|
| sqlx pain | High — async crosses FFI | None — entirely internal |
| rusqlite advantage | Eliminates async FFI | Minimal — already contained |
| Applies to Gainzville | Narrow (inside actions only) | Primary data model |

**Decision: sqlx can stay.** Run `rt-multi-thread` tokio internally. The cache layer
above the database is what Swift interacts with, not the database driver.

**Open question:** whether specific action paths need to await confirmation from a
write before proceeding. Design to keep this surface area minimal. Fire-and-forget
is strongly preferred; avoid building general-purpose async confirmation machinery
unless a concrete need forces it.

---

## Data Flow Architecture

### The reactive cache model

```
┌────────────────────────────────────────────────────────┐
│                      Rust Core                         │
│                                                        │
│  tokio (rt-multi-thread)                               │
│  ┌──────────────┐     ┌─────────────────────────────┐ │
│  │  sqlx +      │────▶│  In-memory read cache        │ │
│  │  SQLite      │     │  (RwLock<QueryCache>)        │◀──── Swift reads (sync)
│  └──────────────┘     │                             │ │
│                       │  Forest structure            │ │
│  ┌──────────────┐     │  Materialized query results  │ │
│  │  Sync server │────▶│  keyed by subscription scope │ │
│  │  (HTTP/WS)   │     └─────────────────────────────┘ │
│  └──────────────┘              │                       │
│                                ▼                       │
│                    crossbeam channel                   │
│                    ChangeNotification { scope }        │
└────────────────────────────────────────────────────────┘
                                 │
                    wakeup callback (with_foreign trait)
                                 │
                                 ▼
┌────────────────────────────────────────────────────────┐
│                    Swift / SwiftUI                      │
│                                                        │
│  MainActor drains channel → reads cache for each scope │
│  @Observable ViewModel.entries = core.read_entries()   │
│  SwiftUI view re-renders                               │
└────────────────────────────────────────────────────────┘
```

### Command path (Swift → Rust)

```swift
// Fire and forget — Swift does not await or track
core.logEntry(entry: newEntry)

// Core enqueues onto tokio channel, processes async internally,
// updates cache, notification fires, Swift re-reads.
```

### Notification path (Rust → Swift)

Rust calls a `with_foreign` callback trait method — the equivalent of Ghostty's
C function pointer in the Options struct. Swift handles it on whatever thread it
arrives, dispatches to `@MainActor`, drains the crossbeam channel.

---

## The Mailbox / Tick Pattern

### What it is

Ghostty calls this a **mailbox** with a **tick()** drain. General pattern names
for the same shape:

- **Reactor pattern** — a single demultiplexer drains accumulated events from multiple producers
- **Message pump** — the Windows `GetMessage`/`DispatchMessage` loop is the classic version
- **Actor inbox** — each actor has a mailbox, processes messages when scheduled
- **Game loop event queue** — the game development framing

The pattern is: producers (background tokio tasks updating cache) enqueue lightweight
`ChangeNotification` values into a `crossbeam::channel`. A single drain point on
the main thread (called by Swift, either on a timer or triggered by a wakeup signal)
processes the queue and dispatches reads.

### Why the channel, not direct callbacks per change

- Multiple changes can land in rapid succession (sync patch arrives, several cache
  entries update). The channel batches them — Swift does one read pass rather than
  thrashing.
- Rust never blocks waiting for Swift to handle a notification. Enqueue and move on.
- Absorbs the impedance mismatch between Rust's internal thread activity and Swift's
  threading model — same reason Ghostty uses a mailbox instead of direct callbacks.

### Implementation shape

```rust
// Rust side: UniFFI object holds sender + cache + callback handle
#[derive(uniffi::Object)]
pub struct GainzvilleCore {
    cache: Arc<RwLock<QueryCache>>,
    tx: crossbeam::channel::Sender<ChangeScope>,
    listener: Arc<dyn CoreListener>,  // with_foreign trait
}

// Swift implements this protocol
#[uniffi::export(with_foreign)]
pub trait CoreListener: Send + Sync {
    fn on_data_changed(&self);  // lightweight wakeup only
}

// Swift side: on wakeup, drain the channel synchronously
func onDataChanged() {
    Task { @MainActor in
        while let scope = core.drainNext() {
            viewModel.refresh(scope: scope)
        }
    }
}
```

---

## SwiftUI Integration

### Views as pure renderers

**"No view holds mutable state or participates in change propagation."**

This is the constraint that eliminated "change notification soup" in Ghostty's macOS
app. Applied to Gainzville: SwiftUI views receive a complete value-type snapshot and
render it. They do not hold references to Rust objects, do not participate in
observation graphs, do not propagate change signals.

**Reasonable violations:** UI-local ephemeral state — form field contents, debounce
state, validation feedback, focus, animation progress. These belong in `@State` in
the view. The constraint applies to *domain state*, not *UI-interaction state*. Form
fields are the canonical example: the intermediate string isn't a domain entity until
the user commits it as a command.

### @Observable as the integration point

```swift
@Observable
@MainActor
class EntriesViewModel {
    var entries: [EntryRecord] = []

    func refresh(scope: DataScope) {
        // synchronous FFI call — reads pre-computed cache state
        entries = core.readEntries(scope: scope)
    }
}
```

- No `ObservableObject`, no `@Published`, no Combine
- `@Observable` macro gives fine-grained tracking — only views that read `entries` re-render
- `@MainActor` ensures the mutation is always on the main thread
- The synchronous `readEntries()` call is fast — it's reading from an in-memory cache, not querying SQLite

### No AppKit needed

Ghostty's AppKit layer exists for Metal rendering, `NSTextInputClient` (IME), and
direct GPU buffer management — none of which apply to Gainzville. Pure SwiftUI +
`@Observable` is the correct target for a data-driven fitness app.

---

## State Ownership: Core vs. Swift

### The heuristic

**Core knows about data subscriptions, not screens.**

- Core holds: query caches, Forest structure, sync state, write queue, active subscriptions
- Swift holds: navigation state, which view is active, UI-ephemeral state, view models built from Core data

Ghostty's core knows which surface is "focused" because that affects *rendering* — a
domain concern. It does not know which menu the user has open. The analogous split for
Gainzville: the core knows which query scopes are subscribed (to prioritize cache
warming); it does not know which screen is visible.

### The Forest — a domain concern

The Forest (set of exercise trees with parent/child/position relationships) belongs in
the Rust core. The signal: the operations it supports are domain invariants, not display
transforms.

**Operations that belong in Rust:**
- `get_children(entry_id)` — domain query
- `get_ancestors(entry_id)` — domain query
- `get_descendants(entry_id)` — domain query
- `can_drop(dragged: id, onto: id) → Bool` — cycle-prevention invariant; must be consistent with sync layer
- `get_successor(entry_id)` — domain query

**What crosses the FFI:** flat `EntryRecord` values and IDs. Swift never traverses the
Forest structure directly; it asks domain questions and gets flat answers.

The Forest is maintained as an in-memory data structure inside the core, kept consistent
with SQLite ground truth, updated when entries change. It is part of the read cache
— synchronous reads, no database round-trip.

**Marked for assessment:** evaluate in codebase context whether additional Forest
operations should live in core. Start with the list above and promote only when logic
has enough domain meaning to warrant it.

---

## Memory and Ownership at the FFI Boundary

With UniFFI (distinct from Ghostty's raw C FFI):

| Type | Ownership model |
|------|----------------|
| `Record` types | Fully serialized/copied at the boundary. Swift owns its copy independently. No shared memory after the call returns. |
| `Object` types | Live on Rust heap, Arc-counted. Swift holds an opaque handle. Methods are calls into Rust. Swift holding the handle keeps the Arc alive. |
| Raw buffers (hypothetical) | Only relevant for high-frequency large data (GPU, audio). Not applicable to Gainzville. Would require stepping outside UniFFI for that specific case. |

"Obligated to copy during the callback" is a raw C FFI concern (borrowed pointer
only valid during the call). UniFFI eliminates this for Records. For Objects, you
hold the handle and call methods — the object lives in Rust memory permanently
until all handles are dropped.

---

## Key Resources and Jump Points

### Primary references
- **Research doc 1:** Production Rust core + SwiftUI via UniFFI (Element X, Mozilla, Bitwarden patterns)
- **Research doc 2:** Ghostty architecture patterns and translation to Rust + UniFFI

### Ghostty source
- `src/apprt/embedded.zig` — the Options callback struct (analog to `with_foreign` trait)
- `src/main_c.zig` + `include/ghostty.h` — the C FFI boundary shape
- `ghostty-org/ghostling` — minimum viable libghostty consumer; useful reference for the consumer side of a C library
- PR #7523 — the split tree value-type refactor; documents the "change notification soup" problem and the fix

### Mitchell Hashimoto writing
- `mitchellh.com/writing/zig-and-swiftui` — Zig + SwiftUI integration details
- `mitchellh.com/writing/ghostty-gtk-rewrite` — lessons from five GUI rewrites; philosophy on fighting vs. embracing platform idioms
- `mitchellh.com/writing/ghostty-1-0-reflection` — high-level architecture rationale

### UniFFI
- `mozilla.github.io/uniffi-rs/latest/` — proc macro guide, async overview, Swift bindings
- `github.com/ianthetechie/uniffi-starter` — non-trivial starter project with XCFramework + SPM pipeline (Ferrostar author)
- `github.com/matrix-org/matrix-rust-sdk` — closest production analog: tokio + rusqlite + UniFFI + SwiftUI

### Patterns and concepts
- **Reactor pattern** — general name for the mailbox/tick shape
- **CQRS** — commands and queries separated; informs the fire-and-forget command / reactive read split
- **Materialized views** — the read cache is a set of materialized query results; ElectricSQL/LiveStore/PowerSync use this framing on the client
- **crossbeam** crate — the channel substrate for the notification queue

---

## Open Questions and Next Steps

### Decisions still open
- **sqlx vs. rusqlite:** provisional decision is to keep sqlx given the cache model. Revisit if integration proves unexpectedly painful. The bundled rusqlite path remains available.
- **Subscription registration API:** how does Swift tell the core which query scopes to keep warm? Explicit `subscribe(scope:)` / `unsubscribe(scope:)` calls tied to view lifecycle? Implicit based on first read? Needs design.
- **Write confirmation:** can the entire write path remain fire-and-forget? Identify any concrete cases that require confirmation before proceeding (e.g., conflict on sync, validation failure that core must evaluate). Design around these specifically rather than building a general async confirmation mechanism.
- **Forest scope:** audit the existing Gainzville domain model against the Forest operation list. Confirm which traversal helpers belong in core vs. Swift display logic.

### Implementation starting points
1. **Skeleton crate structure:** `core/` (pure Rust, no UniFFI), `ffi/` (bridge crate, UniFFI proc macros), `uniffi-bindgen/` (CLI). No logic in `ffi/` — it wraps `core/` types.
2. **Build pipeline:** shell script for XCFramework generation (aarch64 + x86_64-sim → lipo → xcframework). Reference: `uniffi-starter` repo.
3. **Mailbox prototype:** implement `CoreListener` callback trait + crossbeam channel + drain method. Wire to a dummy cache before attaching real sqlx queries.
4. **First real query:** entries in date range, exposed as a subscription scope. End-to-end: tokio task updates cache, notification fires, Swift `EntriesViewModel` refreshes, SwiftUI renders.
5. **Forest integration:** once the cache/notification skeleton works, bring in the Forest structure and wire its query methods to the same synchronous read pattern.

### Research threads not expanded
- **Sync architecture specifics:** patch delta application, conflict resolution strategy, server protocol. This will interact with the tokio internal architecture significantly.
- **`dioxus-web` path:** if SwiftUI + UniFFI proves impractical, Dioxus compiled to WASM running in a WKWebView is a fallback. Defer unless blocked.
- **Agentic coding workflow:** how to structure the codebase for effective use of Claude/Cursor on the Swift layer specifically. Cursor + SwiftUI has an established ecosystem; worth setting up `.cursorrules` early.
- **Testing strategy across the FFI boundary:** UniFFI-exported types can be tested from Swift via XCTest against the real Rust library. Worth designing the test harness before writing much logic.
