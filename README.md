# Gainzville

Gainzville is a platform for athletes to log, analyze, and expand their
training. Instead of giving you a menu of predefined activities and data attributes, Gainzville
gives you the building blocks to create them.



https://github.com/user-attachments/assets/0d4f12cd-2e30-4372-9635-8e83e396f544



## Overview
- Users define a library of `Activities` (event categories) and typed data `Attributes`. They log
`Entries` of activities described by attributes. Training is analyzed using a domain-specific query
engine (WIP).
- One Rust core compiled into a SQLite client, a Postgres server, and a Swift app. The same actions
and queries run on every target; a thin FFI crate exposes core functionality to Swift.
- Writes are reified as mutations + deltas: mutations capture user intent, deltas are invertible
insert/update/delete records that support undo/redo, time-travel, and client-side rebasing.
- Randomized generation of user actions exercises rare code-paths in testing. Injected IO makes runs
deterministic to support reproducibility and deterministic simulation testing (WIP).
- Rust owns query subscriptions and a shared cache, Swift owns the main thread and reads cache
updates at its own cadence to debounce rapid updates and maintain snapshot isolation.

## The Data Model

Gainzville models an event, like performing an exercise, as an `Entry`: a node in a forest of entry
trees where each root has a timestamp. Entries with children are `Sequences` allowing composition of
workouts and organization of the training log. Entries may be instances of `Activities`, a categorical
description like "Run" or "Single-Leg Romanian Deadlift". Entries may be described by `Attributes`
containing typed nominal/ordinal/interval/ratio data.

<p align="center">
    <img src="docs/2025-11-23-core-model.png" width="1000" alt="Forest data model"><br>
    <em>The core model: exercises form a time-ordered forest with typed attributes and activity categorization</em>
</p>

Activities, attributes, and entries are defined and created by users. Gainzville includes a
(nascent) standard library of common activities and attributes built out of the same primitives
exposed to the user. This flexible meta-model gives users the convenience of an off-the-shelf
library of exercises and workouts while maintaining the flexibility to accommodate novel training
modalities.

## Reads and Writes

<p align="center">
    <img src="docs/2026-07-16-action-query-architecture.excalidraw.svg" width="1000" alt="Read-write architecture">
</p>

A `Query` is a reified request to read from the database that encodes its return type as a `QueryResponse`. Mutators and the `QueryStore` execute queries to read and subscribe to database state.

All durable writes are initiated by `Actions`, a description of a user intent such as `MoveEntry { actor_id, entry_id, position, temporal }`. Every action has a corresponding `Mutator` which reads the current database state (via queries) and returns a `Mutation` containing a set of `Deltas`, or an error if invariants would be violated. A delta is a normalized insert, update, or delete to a database table. A mutation contains the original action (user intent), computed deltas, and additional metadata.

Queries and deltas contain backend-agnostic, domain-level types. `Executors` bridge the gap between domain types and backend-specific implementations (SQLite, Postgres, and an in-memory model). Executors are responsible for implementing read/write logic and encoding/decoding to the backend.

## Sync (WIP)

Sync supports offline-first editing, collaboration, and a no-spinner UX. The high-level approach is server-reconciliation with client-side rebasing: a single authoritative server contains the source-of-truth, clients contain a local replica where local writes are applied on top of the last authoritative server state. Local writes are committed to the local database as well as a local-only log of mutations that are pending server-acknowledgement. The server accepts or rejects local writes and commits them as global transactions. When clients receive sync changes, they rebase local writes on top of the latest server updates by reverting local-only mutations, applying server changes, then re-applying local changes modulo those acknowledged by the server.

Significant inspiration is taken from [Figma's sync system design](https://www.figma.com/blog/realtime-editing-of-ordered-sequences/). Their system is particularly relevant because Figma designs are modeled as ordered trees much like Gainzville's entry forest. The Figma approach is "CRDT-like": they use fractional indexing + atomic registers to maintain the consistency of the tree, e.g. to avoid cycles. Unlike a pure CRDT approach which ensures that conflicts resulting in a cycle cannot occur, this approach accepts the possibility and leverages the central server to authoritatively resolve the cycle. In both the Figma and Gainzville use case, this server reconciliation is both rare and relatively low-cost - a misplaced node is uncommon and does not result in data loss.

Mutations and deltas support the sync system by capturing user intent for conflict resolution as well as a row-level record of the precise changes.


## Repository layout

### Rust crates

| Crate | Role |
|-------|------|
| `gv-core` | Domain model, actions, mutators, queries, delta/mutation types. No `sqlx` or `uniffi` dependency. |
| `gv-sql` | DB boundary: leaf column encoders, row table mirrors, `core ↔ Row` transforms, and per-backend `Sqlite*`/`Postgres*` executors. |
| `gv-client` | SQLite app shell: connection pool, app lifecycle, subscriptions. Offline-first target. |
| `gv-server` | Postgres HTTP server: routes, auth, request handling. HTTP API + sync target. |
| `gv-ffi` | FFI boundary: exposes `gv-core` types to Swift via UniFFI. |
| `generation` | Arbitrary data generation for deterministic simulation and integration tests. |
| `ivm` | Experimental DBSP / incremental view maintenance for sync. |

### Swift app

The primary UI lives in [`swift-app/`](./swift-app) — a SwiftUI app targeting
iOS and macOS, backed by the Rust core through a generated XCFramework.

## Documentation

The [`docs/`](./docs) tree covers the design in depth. Good entry points:

| Doc | Topic |
|-----|-------|
| [Domain model](./docs/model.md) | Entities (Entry, Activity, Attribute, Value) and the ordered-forest structure |
| [Actions and queries](./docs/actions_and_queries.md) | The core write path and read path |
| [Boundary transformations](./docs/boundary-transformations.md) | How domain types cross the DB and FFI boundaries |
| [Sync](./docs/sync.md) | Offline-first sync: rebasing, conflict resolution, global sequence numbers |
| [Permissions](./docs/permissions.md) | Authorization and the actor/user model |
| [Attributes / Values](./docs/attributes-design.md) | The typed attribute system |

Swift app patterns and platform notes: [`swift-app/SWIFT-APP.md`](./swift-app/SWIFT-APP.md).

## Getting started

Building and running — including the Postgres setup, migrations, and how to
build the Rust core for the Swift app — is documented in
[DEVELOPMENT.md](./DEVELOPMENT.md).
