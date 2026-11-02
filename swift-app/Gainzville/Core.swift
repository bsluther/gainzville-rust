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

// View model for the activity list. Subscribes once via subscribe_query;
// stays live for the app's lifetime. Dropping `subscription` auto-unsubscribes.
@MainActor
class ActivitiesViewModel: ObservableObject {
    @Published var activities: [Activity] = []
    private var subscription: FfiQuerySubscription?
    private var core: GainzvilleCore?

    func subscribe(to core: GainzvilleCore) {
        self.core = core
        subscription = try? core.subscribeQuery(query: .allActivities(AllActivities()))
        refresh(from: core)
    }

    func refresh(from core: GainzvilleCore) {
        if case .allActivities(let list) = core.readQuery(query: .allActivities(AllActivities())) {
            activities = list
        }
    }

    func createActivity(name: String, description: String?) {
        guard let core else { return }
        let activityId = UUID().uuidString
        let activity = Activity(
            id: activityId,
            ownerId: SYSTEM_ACTOR_ID,
            sourceActivityId: nil,
            name: name,
            description: description
        )
        let template = Entry(
            id: UUID().uuidString,
            activityId: activityId,
            ownerId: SYSTEM_ACTOR_ID,
            name: nil,
            position: nil,
            isTemplate: true,
            displayAsSets: false,
            isSequence: false,
            isComplete: false,
            temporal: .none
        )
        try? core.runAction(action: .createActivity(CreateActivity(
            actorId: SYSTEM_ACTOR_ID,
            activity: activity,
            template: [template]
        )))
        // No manual refresh needed — runAction refreshes the cache and fires
        // on_data_changed, which triggers refresh via AppListener.
    }
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

// TODO: AttributesViewModel does not subscribe to DataChange.didChange, so the
// library attribute list (AttributesListView) won't live-refresh when an
// attribute is created or edited — it only reflects what was cached at subscribe
// time. Wire a DataChange sink that calls refresh(from:) like
// EditAttributesViewModel does (or move it to @GvQuery). Surfaces once name/description editing lands (the
// list shows those fields); not visible while only config defaults are editable.
@MainActor
class AttributesViewModel: ObservableObject {
    @Published var attributes: [Attribute] = []
    private var subscription: FfiQuerySubscription?

    func subscribe(to core: GainzvilleCore) {
        subscription = try? core.subscribeQuery(query: .allAttributes(AllAttributes()))
        refresh(from: core)
    }

    func refresh(from core: GainzvilleCore) {
        if case .allAttributes(let list) = core.readQuery(query: .allAttributes(AllAttributes())) {
            attributes = list
        }
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
