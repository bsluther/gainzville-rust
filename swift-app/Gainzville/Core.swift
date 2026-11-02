//
//  Core.swift
//  Gainzville
//
//  Created by Brian Luther on 4/9/26.
//

import Foundation
import SwiftUI

/// Until the client tracks the current actor, every action and ownership
/// field hard-codes the system actor (`gv_core::SYSTEM_ACTOR_ID`). When
/// real auth lands this becomes a property on `GainzvilleCore` or a
/// session object.
let SYSTEM_ACTOR_ID: String = "eee9e6ae-6531-4580-8356-427604a0dc02"
internal import Combine

// Bridges CoreListener callbacks from Rust to the main thread.
// The onChanged closure is set after both `core` and `viewModel` exist,
// avoiding the circular dependency at construction time.
class AppListener: CoreListener {
    var onChanged: (@Sendable () -> Void)?

    func onDataChanged() {
        onChanged?()
    }
}

// Holds the GainzvilleCore for env-based access in views that need to
// create their own subscriptions (e.g. TextSuggestionsModel). Not
// observable — exposes `core` as a constant; never publishes changes.
@MainActor
final class CoreEnv: ObservableObject {
    let core: GainzvilleCore
    init(core: GainzvilleCore) { self.core = core }
}

// Fan-out for the on_data_changed callback from Rust. Per-view view
// models subscribe via Combine and re-read their cached query on each
// tick. See `client/client.rs::subscribe_cache_ready` for the
// boundary contract: Rust writes the latest values into the cache,
// then notifies; Swift reads from cache at its leisure.
@MainActor
final class DataChange: ObservableObject {
    let didChange = PassthroughSubject<Void, Never>()
    func bump() { didChange.send(()) }
}

// View model for the forest. Subscribes via subscribeForest (backed by AllEntries cache).
// Exposes root entries as a @Published property; children are read synchronously on demand.
@MainActor
class ForestViewModel: ObservableObject {
    @Published var roots: [Entry] = []
    private var subscription: FfiQuerySubscription?
    private var core: GainzvilleCore?

    func subscribe(to core: GainzvilleCore) {
        self.core = core
        subscription = try? core.subscribeForest()
        refresh(from: core)
    }

    func refresh(from core: GainzvilleCore) {
        roots = core.forestRoots()
    }

    // Synchronous reads from the forest cache — always fresh on each render.
    // (Everything else moved to GvKit: see GvQueries and GvWrites.)

    func children(of parentId: String) -> [Entry] {
        core?.forestChildren(parentId: parentId) ?? []
    }

    /// The root template entry for an activity, read from the same forest cache
    /// the log uses. Backs activity-template editing in the library.
    func templateRoot(activityId: String) -> Entry? {
        _ = roots  // establish a SwiftUI dependency so views re-render on change
        return core?.forestActivityTemplateRoot(activityId: activityId)
    }

    // Cards that should mount expanded, consumed once by EntryView.onAppear.
    private var pendingExpandedEntryIds: Set<String> = []

    func markPendingExpanded(_ entryId: String) {
        pendingExpandedEntryIds.insert(entryId)
    }

    func consumePendingExpanded(_ entryId: String) -> Bool {
        pendingExpandedEntryIds.remove(entryId) != nil
    }
}

func makeCore(listener: AppListener) throws -> GainzvilleCore {
    let dbURL = FileManager.default
        .urls(for: .documentDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("gainzville.sqlite")

    // Pass -wipeDB in the scheme's launch arguments to start from a fresh DB.
    // Removes the WAL/SHM sidecars too, or a stale -wal could resurrect rows.
    if CommandLine.arguments.contains("-wipeDB") {
        for suffix in ["", "-wal", "-shm"] {
            try? FileManager.default.removeItem(atPath: dbURL.path + suffix)
        }
    }

    // sqlx defaults to create_if_missing=false, so pre-create the file if needed.
    if !FileManager.default.fileExists(atPath: dbURL.path) {
        FileManager.default.createFile(atPath: dbURL.path, contents: nil)
    }

    return try GainzvilleCore(
        dbPath: "sqlite://\(dbURL.path)",
        actorId: "eee9e6ae-6531-4580-8356-427604a0dc02",
        listener: listener
    )
}
