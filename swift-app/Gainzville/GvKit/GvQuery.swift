//
//  GvQuery.swift
//  GvKit
//
//  `@GvQuery`: a live query as a SwiftUI hook.
//
//      @GvQuery var slots: [ChildSlot]
//      init(parent: Entry) { _slots = GvQuery(.childSlots(of: parent.id)) }
//
//  - Subscribes in `update()` (SwiftUI calls it before every body), so the
//    first value is there on the first render — no empty frame.
//  - Re-targets when the declared query changes (the view is re-initialised
//    with different inputs) and unsubscribes when SwiftUI releases the view's
//    state (the box, and with it the Rust handle, is freed).
//  - The value is always present: the key's default until the first answer,
//    then the last delivered value. Errors are a side channel on the projected
//    value (`$slots.error`), in the style of SQLiteData's `@FetchAll`.
//  - A query that depends on view @State (not on init inputs) can't be
//    declared in init; use `$value.load(_:)` from `.task(id:)` / `.onChange`.
//

import Foundation
import Observation
import SwiftUI

/// A typed live query: what to ask (a member of the client set), what to show
/// before an answer, and how to read the answer. Define keys in
/// `GvQueries.swift`.
struct GvKey<Value> {
    /// `nil` makes the hook inactive: no subscription, value is `defaultValue`.
    let query: ClientQuery?
    let defaultValue: Value
    /// `nil` if the answer isn't this key's shape (never expected).
    let decode: (ClientQueryResponse) -> Value?
}

enum GvQueryError: Error {
    /// Rust answered the query with an error (e.g. a lookup of a deleted entry).
    case failed(String)
}

@propertyWrapper
struct GvQuery<Value>: DynamicProperty {
    private let key: GvKey<Value>
    @State private var box: GvQueryBox<Value>
    @Environment(\.gvClient) private var client

    init(_ key: GvKey<Value>) {
        self.key = key
        // Evaluated on every view init but kept only the first time (State
        // semantics); creating a box is cheap and does not subscribe.
        _box = State(initialValue: GvQueryBox(defaultValue: key.defaultValue))
    }

    var wrappedValue: Value { box.value }

    var projectedValue: GvQueryState<Value> { GvQueryState(box: box) }

    // DynamicProperty.update() isn't guaranteed main-actor isolated by the SDK,
    // but SwiftUI calls it on the main thread before each body.
    nonisolated func update() {
        MainActor.assumeIsolated {
            box.bind(declared: key, client: client)
        }
    }
}

/// The hook's side channels and imperative controls (`$value`).
@MainActor
struct GvQueryState<Value> {
    fileprivate let box: GvQueryBox<Value>

    var error: (any Error)? { box.error }

    /// Re-target to `key` imperatively (for a query that depends on view
    /// @State). Holds until the declared query changes.
    func load(_ key: GvKey<Value>) { box.load(key) }
}

/// Per-hook storage, kept alive by SwiftUI `@State`. Holds the Rust handle.
///
/// Observation is driven by a single `revision` counter rather than by the
/// stored value itself: the value is set inside `update()` on (re)subscribe,
/// and mutating observed state while SwiftUI is updating the view would
/// schedule a redundant re-render (or warn). `update()` writes without
/// bumping; batch deliveries and `load` bump.
@Observable
final class GvQueryBox<Value>: GvSubscriber {
    private var revision: UInt64 = 0

    @ObservationIgnored private var storage: Value
    @ObservationIgnored private var storedError: (any Error)?
    @ObservationIgnored private var decode: ((ClientQueryResponse) -> Value?)?
    @ObservationIgnored private var hasDeclared = false
    @ObservationIgnored private var declaredQuery: ClientQuery?
    @ObservationIgnored private var activeQuery: ClientQuery?
    @ObservationIgnored private var handle: LiveSubscriptionHandle?
    @ObservationIgnored private var subscriptionId: UInt64?
    @ObservationIgnored private var version: UInt64 = 0
    @ObservationIgnored private weak var client: GvClient?

    init(defaultValue: Value) {
        storage = defaultValue
    }

    var value: Value {
        _ = revision
        return storage
    }

    var error: (any Error)? {
        _ = revision
        return storedError
    }

    /// Called from `update()`: follow the declared query.
    func bind(declared key: GvKey<Value>, client: GvClient?) {
        guard let client else { return }
        if hasDeclared, key.query == declaredQuery, self.client === client { return }
        hasDeclared = true
        declaredQuery = key.query
        start(key, client: client)
    }

    /// Imperative re-target; see `GvQueryState.load`.
    func load(_ key: GvKey<Value>) {
        guard let client else { return }  // not bound yet; the declared query applies
        guard key.query != activeQuery else { return }
        start(key, client: client)
        revision &+= 1
    }

    private func start(_ key: GvKey<Value>, client: GvClient) {
        self.client = client
        activeQuery = key.query
        decode = key.decode
        // Release the old subscription first (Rust refcount).
        handle = nil
        subscriptionId = nil
        version = 0
        storedError = nil
        guard let query = key.query else {
            storage = key.defaultValue
            return
        }
        do {
            let subscribed = try client.subscribe(query, for: self)
            handle = subscribed.handle
            subscriptionId = subscribed.subscriptionId
            version = subscribed.version
            accept(subscribed.value, fallback: key.defaultValue)
        } catch {
            storedError = error
            storage = key.defaultValue
        }
    }

    func receive(_ value: LiveResult, subscriptionId id: UInt64, version v: UInt64) {
        // Ignore updates for a previous target, and anything not newer than
        // what we hold (the first value can already reflect batch `v`).
        guard id == subscriptionId, v > version else { return }
        version = v
        accept(value, fallback: nil)
        revision &+= 1
    }

    private func accept(_ value: LiveResult, fallback: Value?) {
        switch value {
        case .failed(let message):
            // Keep the last good value; surface the failure on the side channel.
            storedError = GvQueryError.failed(message)
            if let fallback { storage = fallback }
        case .response(let response):
            if let decoded = decode?(response) {
                storage = decoded
                storedError = nil
            }
        }
    }
}
