# Gainzville — Project Index

## Crates

| Crate | Role |
|-------|------|
| `core` | Domain model, actions, mutators, queries, delta/mutation types. No sqlx types in public API. |
| `client` | SQLite client: `SqliteQueryExecutor`, `SqliteApply`, `SqliteClient`. Offline-first target. |
| `server` | Postgres server: `PostgresQueryExecutor`, `PgApply`, `PostgresServer`. HTTP API + sync target. |
| `generation` | Arbitrary data generation traits for deterministic simulation and integration tests. |
| `dx-app` | Cross-platform Dioxus app (desktop, mobile, web). **Deprecated** — kept as reference only. |
| `ivm` | Experimental DBSP/incremental view maintenance for sync. |

## Docs

| Doc | When to consult |
|-----|----------------|
| [Domain model](./docs/model.md) | Understanding entities (Entry, Activity, Attribute, Value) and the ordered-forest structure |
| [Actions and queries](./docs/actions_and_queries.md) | Write path (Action→Mutator→Mutation→Apply) and read path (Query→QueryExecutor→DB) — the core I/O architecture |
| [Permissions](./docs/permissions.md) | Authorization rules and actor/user model |
| [Sync](./docs/sync.md) | Offline-first sync design: rebasing, HLC, global sequence numbers |
| [Features](./docs/features.md) | Product feature roadmap |
| [Generation](./docs/generation.md) | How arbitrary test data generation works |
| [Properties](./docs/properties.md) | Property-based testing strategy |
| [Attributes/Values design](./docs/attributes-design.md) | Typed attribute system and serde gotchas (`arbitrary_precision`) |
| [UI Architecture](./docs/ui/architecture.md) | Dioxus: platform targeting (`#[cfg]`), component model, CSS/Tailwind styling |
| [UI Design](./docs/ui/design.md) | Dioxus: navigation patterns, interaction models |
| [Adaptive rendering decisions](./docs/ui/adaptive-rendering-decisions.md) | Dioxus: historical rationale for `#[cfg]` vs CSS vs runtime rendering |

Additional Dioxus reference: `/dx-app/AGENTS.md` and `/dx-app/docs/00-OVERVIEW.md` through `10-WASM-SPLIT.md`.

Swift app patterns, platform targets, design system, and open work: [`swift-app/SWIFT-APP.md`](./swift-app/SWIFT-APP.md).

## Primary UI Target

The **Swift app** (`swift-app/`) is the primary UI. The Dioxus app (`dx-app/`) is deprecated — do not add features to it, but it remains as a reference implementation.

## Project Goals

- Offline-first sync
- HTTP API
- LLM-assisted import of unstructured training logs (markdown → `Action` arrays)
- Desktop app
- Deterministic simulation testing

## Development Notes

- **postgres docker required at compile time**: `cargo build` for `gv_server` connects to the live DB for sqlx compile-time verification. Start postgres before building.
- **Test from workspace root**: `cargo test` (not `--package`) to catch feature-unification issues. The `ivm` crate enables `serde_json/arbitrary_precision` workspace-wide, which breaks internally-tagged enums with numeric fields.
- **Swift app — building and verifying**: see [`swift-app/SWIFT-APP.md`](./swift-app/SWIFT-APP.md) for how to rebuild Rust binaries, regenerate Swift bindings after FFI changes, and verify the Swift app compiles after any change.
- **Previous versions (reference only, do not modify)**:
  - `/Users/brianluther/dev/swift/gv-2025-05-19/Gainzville` — Swift app (May 2025)
  - `/Users/brianluther/dev/gv/gv-2025-01-15` — React Native app (Jan 2025)
