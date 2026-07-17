# Development

## Prerequisites

- Rust (stable) with the iOS/macOS targets for Swift builds:
  ```sh
  rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin x86_64-apple-darwin
  ```
- Docker (for Postgres)
- `sqlx-cli` (for migrations):
  ```sh
  cargo install sqlx-cli --no-default-features --features postgres
  ```
- Xcode (for the Swift app)

> **Note:** Building the `postgres` feature of `gv-sql` (pulled in by
> `gv-server`) connects to the live database for sqlx's compile-time query
> verification. **Start Postgres before building anything that touches the
> server.**

## Postgres

Start the database (`-d` detaches from the current shell):
```sh
docker-compose up -d
```

Stop it, keeping volumes (the database persists between runs):
```sh
docker-compose down
```

Stop it and erase volumes (delete the database):
```sh
docker-compose down -v
```

The container provisions a dev and a test database. Connect via `psql`:
```sh
# main database
docker exec -it gainzville-postgres psql -U gainzville -d gainzville_dev

# test database
docker exec -it gainzville-postgres psql -U gainzville -d gainzville_test
```

Credentials (dev only) are defined in `docker-compose.yml`: user `gainzville`,
password `dev_password`.

## Migrations (sqlx)

Migrations live under `gv-sql/`, one directory per backend.

**Postgres** (run from the workspace root):
```sh
sqlx migrate run --source gv-sql/postgres/migrations \
  --database-url postgres://gainzville:dev_password@localhost/gainzville_dev

# test database
sqlx migrate run --source gv-sql/postgres/migrations \
  --database-url postgres://gainzville:dev_password@localhost/gainzville_test
```

**SQLite:**
```sh
sqlx migrate run --source gv-sql/sqlite/migrations \
  --database-url "sqlite:test.db"
```

Add a new migration (timestamped):
```sh
sqlx migrate add --source gv-sql/<backend>/migrations <name>
```

## Building & testing Rust

```sh
cargo build
cargo test          # from the workspace root — see note below
```

> **Test from the workspace root, not `--package`.** The `ivm` crate enables
> `serde_json/arbitrary_precision`, which Cargo unifies workspace-wide; running
> the full suite from the root catches feature-unification issues that a
> per-package run would miss.

### Postgres sandbox

A scratch binary for experimenting against the test database:
```sh
TEST_DATABASE_URL="postgres://gainzville:dev_password@localhost:5432/gainzville_test" \
  cargo run --bin pg_sandbox
```

## Building the core for the Swift app

The Swift app consumes the Rust core through a generated XCFramework
(`swift-app/Frameworks/GvFfi.xcframework`) plus generated Swift bindings. Two
scripts in `scripts/` produce them — **always run from the workspace root.**

| Situation | Script |
|-----------|--------|
| The FFI surface changed (`gv-ffi/src/types.rs`, exported types, new `FfiAction` variants, etc.) | `scripts/regen-bindings.sh` |
| Implementation-only Rust change (no `#[uniffi::export]` signature changes) | `scripts/rebuild-xcframework.sh` |

```sh
./scripts/regen-bindings.sh      # regenerate bindings + rebuild xcframework
./scripts/rebuild-xcframework.sh # rebuild xcframework only (faster)
```

`regen-bindings.sh` rebuilds the `gv-ffi` dylib, regenerates `gv_ffi.swift` and
the C headers, copies them into `swift-app/`, then compiles the release
libraries for each Apple target and assembles the XCFramework.
`rebuild-xcframework.sh` skips binding regeneration.

> **When in doubt, use `regen-bindings.sh`.** UniFFI embeds interface checksums
> in both the Swift bindings and the compiled library; a mismatch causes a fatal
> crash at launch. `rebuild-xcframework.sh` is only safe when the change is
> purely internal Rust.

## Running & verifying the Swift app

Build from `swift-app/` using `-target` (not `-scheme`, which hits a
platform-resolution bug):

```sh
cd swift-app

# iOS
xcodebuild -project Gainzville.xcodeproj -target Gainzville build CONFIGURATION=Debug

# macOS
xcodebuild -project Gainzville.xcodeproj -target 'Gainzville macOS' build CONFIGURATION=Debug
```

For running on a simulator, design-system conventions, and app architecture, see
[`swift-app/SWIFT-APP.md`](./swift-app/SWIFT-APP.md) and
[`swift-app/DEVELOPMENT.md`](./swift-app/DEVELOPMENT.md).