# Plan: Query Cache Architecture (AllActivities scope)

## Context

The current PoC uses a stream-based approach: `subscribe_activities` drives a tokio
stream and passes `Vec<FfiActivity>` directly through the listener callback on every
write. This works but doesn't match the target architecture.

The target: Rust maintains a query result cache. After any write, subscribed queries
are re-run against SQLite, results stored in the cache, then Swift is notified once
via the no-arg `on_data_changed()`. Swift reads from the cache synchronously via
`read_query(query)`. Scope: `AllActivities` only for this iteration.

Benefits captured: batching, snapshot coherence, no data in callbacks, simpler
threading, architectural flexibility (cache update mechanism is internal to Rust).

---

## Files to Modify

- `gv-ffi/src/types.rs` — add `FfiQuery`, `FfiQueryResult`
- `gv-ffi/src/core.rs` — add `QueryCache`, `QuerySubscription`, update `GainzvilleCore`
- `swift-app/Gainzville/Core.swift` — update `ActivitiesViewModel`
- Regenerate bindings + rebuild XCFramework after Rust changes

---

## Step 1: Add FFI query types (`types.rs`)

```rust
#[derive(uniffi::Enum, Clone)]
pub enum FfiQuery {
    AllActivities,
    // extended later: AllEntries, EntriesInInterval { from_ms: i64, to_ms: i64 }, etc.
}

#[derive(uniffi::Enum)]
pub enum FfiQueryResult {
    Activities(Vec<FfiActivity>),
}
```

---

## Step 2: Query cache types (`core.rs`)

Add internal (non-FFI) types for the cache:

```rust
#[derive(Hash, Eq, PartialEq, Clone)]
enum CacheKey {
    AllActivities,
}

impl From<FfiQuery> for CacheKey {
    fn from(q: FfiQuery) -> Self {
        match q { FfiQuery::AllActivities => CacheKey::AllActivities }
    }
}

enum CachedResult {
    Activities(Vec<Activity>),  // core types, converted to Ffi only on read
}

struct QueryCache {
    entries: HashMap<CacheKey, CachedResult>,
}
```

Add `cache: Arc<Mutex<QueryCache>>` to `GainzvilleCore`.

---

## Step 3: Shared async helper

A free async fn (not a method, so it can be called from both `run_action` and the
background ticker without needing `Arc<Self>`):

```rust
async fn refresh_subscribed_queries(
    client: &SqliteClient,
    cache: &Mutex<QueryCache>,
) -> Result<(), FfiError> {
    let keys: Vec<CacheKey> = cache.lock().unwrap().entries.keys().cloned().collect();
    for key in keys {
        let result = match key {
            CacheKey::AllActivities => {
                let mut conn = client.pool.acquire().await.map_err(...)?;
                let activities = SqliteQueryExecutor::new(&mut *conn)
                    .execute(AllActivities {}).await.map_err(FfiError::from)?;
                CachedResult::Activities(activities)
            }
        };
        cache.lock().unwrap().entries.insert(key, result);
    }
    Ok(())
}
```

---

## Step 4: Update `run_action`

After the write commits, refresh the cache then fire `on_data_changed()` once:

```rust
pub fn run_action(&self, action: FfiAction) -> Result<(), FfiError> {
    let core_action = ffi_action_to_core(action, self.actor_id)?;
    RUNTIME.block_on(self.client.run_action(core_action)).map_err(FfiError::from)?;
    RUNTIME.block_on(refresh_subscribed_queries(&self.client, &self.cache))?;
    self.listener.on_data_changed();
    Ok(())
}
```

---

## Step 5: `QuerySubscription` object (auto-unsubscribe on GC)

```rust
#[derive(uniffi::Object)]
pub struct QuerySubscription {
    key: CacheKey,
    cache: Arc<Mutex<QueryCache>>,
}

impl Drop for QuerySubscription {
    fn drop(&mut self) {
        if let Ok(mut c) = self.cache.lock() {
            c.entries.remove(&self.key);
        }
    }
}
```

When Swift drops its reference to the subscription object, UniFFI drops the Arc,
which runs `Drop`, which removes the query from the cache. No manual unsubscribe API
needed.

---

## Step 6: `subscribe_query` and `read_query` on `GainzvilleCore`

`subscribe_query`: insert the key into the cache first with the initial result (run
the query immediately), then return the `QuerySubscription` handle.

`read_query`: read from cache, convert core types to FFI types on the way out.

```rust
pub fn subscribe_query(&self, query: FfiQuery) -> Result<Arc<QuerySubscription>, FfiError> {
    let key = CacheKey::from(query);
    // Run initial fetch and insert before returning handle
    let initial = RUNTIME.block_on(run_query_for_key(&self.client, &key))?;
    self.cache.lock().unwrap().entries.insert(key.clone(), initial);
    Ok(Arc::new(QuerySubscription { key, cache: Arc::clone(&self.cache) }))
}

pub fn read_query(&self, query: FfiQuery) -> Option<FfiQueryResult> {
    let key = CacheKey::from(query);
    self.cache.lock().unwrap().entries.get(&key).map(|r| match r {
        CachedResult::Activities(v) =>
            FfiQueryResult::Activities(v.iter().cloned().map(FfiActivity::from).collect()),
    })
}
```

---

## Step 7: Update `start_background_ticker`

The ticker calls `client.run_action()` directly, bypassing `GainzvilleCore::run_action`
(cache not refreshed). Fix by also capturing `Arc<Mutex<QueryCache>>` and
`Arc<dyn CoreListener>` and calling `refresh_subscribed_queries` +
`listener.on_data_changed()` after each write.

---

## Step 8: Remove `subscribe_activities`

The stream-based `subscribe_activities` is replaced by `subscribe_query`. Remove from
`core.rs`. Also remove the now-unused `ActivitiesListener` trait.

---

## Step 9: Update Swift side (`Core.swift`)

Replace `ActivitiesListenerBridge` + stream-based `ActivitiesViewModel` with:

```swift
@MainActor
class ActivitiesViewModel: ObservableObject {
    @Published var activities: [FfiActivity] = []
    private var subscription: QuerySubscription?

    func subscribe(to core: GainzvilleCore) {
        subscription = try? core.subscribeQuery(query: .allActivities)
        refresh(from: core)
    }

    func refresh(from core: GainzvilleCore) {
        if case .activities(let list) = core.readQuery(query: .allActivities) {
            activities = list
        }
    }
}
```

`AppListener` is given a closure at init that calls `viewModel.refresh(from: core)` on
the main thread (same closure pattern used by `ActivitiesListenerBridge` previously):

```swift
class AppListener: CoreListener {
    private let onChanged: @Sendable () -> Void
    init(_ onChanged: @escaping @Sendable () -> Void) { self.onChanged = onChanged }
    func onDataChanged() { onChanged() }
}
```

Wired in `GainzvilleApp.init`:

```swift
let listener = AppListener {
    Task { @MainActor [weak viewModel] in
        viewModel?.refresh(from: core)
    }
}
```

---

## Step 10: Regenerate bindings + rebuild

```sh
cargo build -p gv_ffi
cargo run --bin uniffi-bindgen -- generate \
  --library target/debug/libgv_ffi.dylib --language swift --out-dir gv-ffi/bindings/
cp gv-ffi/bindings/gv_ffi.swift swift-app/Gainzville/gv_ffi.swift
cp gv-ffi/bindings/gv_ffiFFI.h swift-app/Frameworks/gv_ffiFFI.h
cp gv-ffi/bindings/gv_ffiFFI.modulemap swift-app/Frameworks/gv_ffiFFI.modulemap
# Then rebuild XCFramework for sim + device targets (see gv-ffi/README.md)
```

---

## Verification

- App launches → activities list populated immediately (initial cache fill on subscribe)
- Background ticker fires every 10s → `on_data_changed()` fires once → `refresh()` called → list updates
- Manually add activity → same path, list updates
- Drop subscription (future test) → `Drop` runs, query removed from cache, no further refreshes
