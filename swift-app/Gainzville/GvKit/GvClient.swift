//
//  GvClient.swift
//  GvKit
//
//  The Swift half of the live-query protocol (RFD 0003). Views don't use this
//  directly: they declare `@GvQuery` hooks, or call `gv.read(_:)` for a
//  one-off answer. GvClient owns the batch listener and routes each update
//  to the hooks subscribed to it.
//
//  Protocol, in one paragraph: `subscribe` returns a subscription id, the
//  first value and the version it belongs to. After every data change Rust
//  calls `LiveListener.onBatch` ONCE with `{ version, updates }`, holding only
//  the subscriptions whose result changed. GvClient applies the whole batch in
//  one main-actor step, so every hook moves to the new version together and
//  SwiftUI renders the screen once, never a mix of two versions.
//
//  Batches go through one queue. Rust enqueues batch v before any `subscribe`
//  can report version v, so a `subscribe` that returns v first applies what's
//  queued up to v: a newly mounted hook is never ahead of its neighbours because
//  of a batch still in flight.
//

import Foundation
import os
import SwiftUI

// MARK: - Listener

/// Receives batches on a Rust thread and forwards them. Like `AppListener`,
/// but the handler is fixed at init (never mutated while Rust may call it) and
/// the entry point is explicitly `nonisolated`: it does not run on the main
/// actor, whatever the module's default isolation says.
final class GvBatchListener: LiveListener {
    private let handler: @Sendable (LiveBatch) -> Void

    nonisolated init(handler: @escaping @Sendable (LiveBatch) -> Void) {
        self.handler = handler
    }

    nonisolated func onBatch(batch: LiveBatch) {
        handler(batch)
    }
}

/// Batches waiting for the main actor, in version order. The listener pushes on
/// a Rust thread, so this never blocks for longer than an array operation.
private nonisolated final class GvBatchQueue: @unchecked Sendable {
    private let lock = NSLock()
    private var batches: [LiveBatch] = []

    func push(_ batch: LiveBatch) {
        lock.withLock { batches.append(batch) }
    }

    /// Remove and return the queued batches, oldest first, up to `version` if given.
    func take(upTo version: UInt64? = nil) -> [LiveBatch] {
        lock.withLock {
            let count = version.map { v in batches.prefix { $0.version <= v }.count } ?? batches.count
            let taken = Array(batches.prefix(count))
            batches.removeFirst(count)
            return taken
        }
    }
}

// MARK: - Subscribers

/// Something that holds one live subscription's value (a `@GvQuery` hook's box).
@MainActor
protocol GvSubscriber: AnyObject {
    func receive(_ value: LiveResult, subscriptionId: UInt64, version: UInt64)
}

private struct WeakSubscriber {
    weak var subscriber: (any GvSubscriber)?
}

// MARK: - Client

@MainActor
final class GvClient {
    let core: GainzvilleCore
    /// Subscription id -> the hooks holding it. Identical queries share one id
    /// (Rust refcounts them), so several hooks can sit behind one id.
    private var routes: [UInt64: [WeakSubscriber]] = [:]
    private var subscribesSincePrune = 0
    private let queue = GvBatchQueue()
    private let signposter = OSSignposter(subsystem: "com.gainzville", category: "GvKit")

    /// Registers the batch listener; create this before any hook subscribes.
    init(core: GainzvilleCore) {
        self.core = core
        let queue = queue
        let listener = GvBatchListener { [weak self] batch in
            queue.push(batch)
            Task { @MainActor [weak self] in
                self?.applyQueued()
            }
        }
        core.setLiveListener(listener: listener)
    }

    /// Subscribe and route this subscription's future updates to `subscriber`.
    /// Returns the first value; the caller keeps `handle` alive for as long as
    /// it wants updates (releasing it unsubscribes).
    func subscribe(_ query: ClientQuery, for subscriber: any GvSubscriber) throws -> LiveSubscribed {
        // Runs SQL or a forest walk on the main thread: signposted so a slow
        // question shows up in Points of Interest and Hangs traces.
        let subscribed = try signposter.withIntervalSignpost(
            "liveSubscribe",
            id: signposter.makeSignpostID(),
            "\(String(describing: query), privacy: .public)"
        ) {
            try core.liveSubscribe(query: query)
        }
        applyQueued(upTo: subscribed.version)
        routes[subscribed.subscriptionId, default: []].append(WeakSubscriber(subscriber: subscriber))
        subscribesSincePrune += 1
        if subscribesSincePrune >= 64 { prune() }
        return subscribed
    }

    /// Answer once against the latest version, without subscribing. For
    /// imperative checks such as drag-time validation. Returns the key's
    /// default if the query is inactive or fails.
    func read<Value>(_ key: GvKey<Value>) -> Value {
        guard let query = key.query, let response = try? core.liveRead(query: query) else {
            return key.defaultValue
        }
        return key.decode(response) ?? key.defaultValue
    }

    private func applyQueued(upTo version: UInt64? = nil) {
        for batch in queue.take(upTo: version) {
            apply(batch)
        }
    }

    /// Apply one batch in one main-actor step.
    private func apply(_ batch: LiveBatch) {
        for update in batch.updates {
            guard let entries = routes[update.subscriptionId] else { continue }
            let live = entries.filter { $0.subscriber != nil }
            routes[update.subscriptionId] = live.isEmpty ? nil : live
            for entry in live {
                entry.subscriber?.receive(update.value, subscriptionId: update.subscriptionId, version: batch.version)
            }
        }
    }

    /// Drop routes whose hooks are gone (released hooks never deregister:
    /// their deinit can't touch main-actor state).
    private func prune() {
        subscribesSincePrune = 0
        routes = routes.compactMapValues { entries in
            let live = entries.filter { $0.subscriber != nil }
            return live.isEmpty ? nil : live
        }
    }
}

// MARK: - Environment

private struct GvClientKey: EnvironmentKey {
    static let defaultValue: GvClient? = nil
}

extension EnvironmentValues {
    /// The live-query client. `nil` in previews: hooks then show their defaults.
    var gvClient: GvClient? {
        get { self[GvClientKey.self] }
        set { self[GvClientKey.self] = newValue }
    }
}
