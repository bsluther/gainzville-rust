# Client Refactor Plan

## Goal
Port query subscription/cache logic from `gv-ffi/src/core.rs` into the `client` crate, then simplify the FFI layer to delegate to the client.

## Architecture

```
SqlitePool
    └── QueryStore  (pool clone + Arc<Mutex<HashMap<AnyQuery, AnyQueryResponse>>>)
            - run_query<Q: Query>          (typed, HRTB bound)
            - run_any_query(AnyQuery)      (type-erased, for cache)
            - subscribe_query(AnyQuery)    -> Arc<QuerySubscription>
            - read_cached_query(AnyQuery)  -> Option<AnyQueryResponse>
            - refresh_subscribed_queries() (private, called by bg task)
            - bg task: change_rx -> refresh all -> on_cache_ready()

SqliteClient  (facade: pool + change_transmitter + cache_ready_transmitter + QueryStore)
    - run_action        (writes, broadcasts change)
    - run_query         (delegates to QueryStore)
    - run_any_query     (delegates to QueryStore)
    - subscribe_query   (delegates to QueryStore)
    - read_cached_query (delegates to QueryStore)
    - subscribe_cache_ready() -> broadcast::Receiver<()>
    - start_background_ticker(actor_id) (spawns test activity writer)
    - stream_*          (existing Dioxus streams, use change_transmitter directly)

GainzvilleCore (FFI facade, thin wrapper over SqliteClient)
    - CoreListener stays at FFI level, wired via subscribe_cache_ready()
    - QuerySubscription needs FfiQuerySubscription wrapper for uniffi::Object
```

## Completed

- [x] Fixed `run_query` on `SqliteClient` — HRTB where clause + `.await`
- [x] Created `client/query_store.rs`:
  - `QueryStore` with pool + cache, background refresh task
  - `QuerySubscription` with Drop-based auto-unsubscribe
  - `run_query`, `run_any_query`, `subscribe_query`, `read_cached_query`, `refresh_subscribed_queries`
- [x] Wired `QueryStore` into `SqliteClient`:
  - `from_pool` constructs `QueryStore` with `cache_ready_transmitter` callback
  - `init` delegates to `from_pool` + runs migrations
  - Delegation methods on `SqliteClient`
  - `subscribe_cache_ready()` exposes second broadcast
  - `start_background_ticker(actor_id)` ported from FFI (simplified — no manual refresh needed)

## Remaining

### 1. Cleanup in `client` crate
- [ ] Delete `client/query_cache.rs` (replaced by `query_store.rs`)
- [ ] Update `client/lib.rs` — remove `pub mod query_cache`
- [ ] Fix unused import warning: `FindActivityById` only used in tests module — move to tests or keep via `pub use`

### 2. Port `gv-ffi/src/core.rs` to use new `SqliteClient` API
- [ ] Remove `run_query_for_key` free fn — replaced by `client.run_any_query`
- [ ] Remove `refresh_subscribed_queries` free fn — now internal to `QueryStore`
- [ ] Remove `QueryCache` struct — now inside `QueryStore`
- [ ] Remove `cache: Arc<Mutex<QueryCache>>` field from `GainzvilleCore` — no longer needed
- [ ] Simplify `GainzvilleCore::run_action` — just call `client.run_action`, no manual refresh or notify
- [ ] Simplify `GainzvilleCore::subscribe_query` — delegate to `client.subscribe_query`
- [ ] Simplify `GainzvilleCore::read_query` — delegate to `client.read_cached_query` + map to Ffi types
- [ ] Wire `CoreListener` via `client.subscribe_cache_ready()` — subscribe in `GainzvilleCore::new`, spawn bg task that calls `listener.on_data_changed()` on each tick
- [ ] `start_background_ticker` — delegate to `client.start_background_ticker(self.actor_id)`
- [ ] Add `FfiQuerySubscription(Arc<QuerySubscription>)` with `#[derive(uniffi::Object)]` — thin wrapper so UniFFI can expose it to Swift

### 3. Expand `AnyQuery`/`AnyQueryResponse` coverage
- [ ] `run_any_query` currently only handles `AllActivities`, returns error for all others
- [ ] Add variants to `AnyQueryResponse` and arms to `run_any_query` as queries are needed

## Key Design Notes

- `CoreListener` (`#[uniffi::export(with_foreign)]`) stays FFI-only — client uses generic `Arc<dyn Fn() + Send + Sync>`
- `QuerySubscription` drop-to-unsubscribe works across FFI via the `FfiQuerySubscription` newtype wrapper
- `broadcast::Sender` is Clone and Arc-backed — cheap to pass around
- Change flow: `run_action` commits → `change_transmitter` fires → `QueryStore` bg task refreshes cache → `cache_ready_transmitter` fires → FFI listener calls `on_data_changed()` → Swift reads from cache
- `run_action` in FFI is now fire-and-forget for cache — no synchronous refresh before return (behavioral change from old design; Swift waits for `on_data_changed` callback)
