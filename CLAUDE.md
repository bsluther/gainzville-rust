# Gainzville — Project Index

## Crates

| Crate | Role |
|-------|------|
| `gv-core` | Domain model, actions, mutators, queries, delta/mutation types. No `sqlx` or `uniffi` dependency. |
| `gv-sql` | DB boundary: `*Column` leaf encoders, `*Row` table mirrors, `core ↔ Row` transforms, and the per-backend executors (`Sqlite*`/`Postgres*`) behind `sqlite`/`postgres` features. |
| `gv-client` | SQLite app shell: connection pool, app lifecycle, subscriptions. Offline-first target. |
| `gv-server` | Postgres HTTP server: routes, auth, request handling. HTTP API + sync target. |
| `gv-ffi` | FFI boundary: exposes `gv-core` types to Swift via uniffi (`[uniffi::remote]` + `custom_type!`). Depends on `gv-core`/`gv-client`, not `gv-sql`. |
| `generation` | Arbitrary data generation traits for deterministic simulation and integration tests. |
| `ivm` | Experimental DBSP/incremental view maintenance for sync. |

## Docs

| Doc | When to consult |
|-----|----------------|
| [Domain model](./docs/model.md) | Understanding entities (Entry, Activity, Attribute, Value) and the ordered-forest structure |
| [Actions and queries](./docs/actions_and_queries.md) | Write path (Action→Mutator→Mutation→DeltaExecutor) and read path (Query→QueryExecutor→DB) — the core I/O architecture |
| [Boundary transformations](./docs/boundary-transformations.md) | How domain types cross the DB (`gv-sql`) and FFI (`gv-ffi`) boundaries; `*Column`/`*Row`, uniffi remote types, and gotchas |
| [Permissions](./docs/permissions.md) | Authorization rules and actor/user model |
| [Sync](./docs/sync.md) | Offline-first sync design: rebasing, HLC, global sequence numbers |
| [Features](./docs/features.md) | Product feature roadmap |
| [Properties](./docs/properties.md) | Property-based testing strategy |
| [Attributes/Values design](./docs/attributes-design.md) | Typed attribute system and serde gotchas (`arbitrary_precision`) |
| [Sets design](./docs/sets-design.md) | display_as_sets: invariants, ConvertToSets/DuplicateEntry actions, temporal model, sets UI |
| [Forest UI model](./docs/forest-ui-model.md) | How the entry forest is presented and traversed in the UI |

Swift app patterns, platform targets, design system, and open work: [`swift-app/SWIFT-APP.md`](./swift-app/SWIFT-APP.md). Swift/iOS architecture research and decisions live in [`docs/swift-architecture/`](./docs/swift-architecture/).

## Primary UI Target

The **Swift app** (`swift-app/`) is the primary UI.

## Project Goals

- Offline-first sync
- HTTP API
- LLM-assisted import of unstructured training logs (markdown → `Action` arrays)
- Desktop app
- Deterministic simulation testing

## Development Notes

- **postgres docker required at compile time**: building the `postgres` feature of `gv-sql` (pulled in by `gv-server`) connects to the live DB for sqlx `query!` compile-time verification. Start postgres before building.
- **Test from workspace root**: `cargo test` (not `--package`) to catch feature-unification issues. The `ivm` crate enables `serde_json/arbitrary_precision` workspace-wide, which breaks internally-tagged enums with numeric fields.
- **Swift app — building and verifying**: see [`swift-app/SWIFT-APP.md`](./swift-app/SWIFT-APP.md) for how to rebuild Rust binaries, regenerate Swift bindings after FFI changes, and verify the Swift app compiles after any change.
- **Previous versions (reference only, do not modify)**:
  - `/Users/brianluther/dev/swift/gv-2025-05-19/Gainzville` — Swift app (May 2025)
  - `/Users/brianluther/dev/gv/gv-2025-01-15` — React Native app (Jan 2025)
