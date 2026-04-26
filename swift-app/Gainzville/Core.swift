//
//  Core.swift
//  Gainzville
//
//  Created by Brian Luther on 4/9/26.
//

import Foundation
import SwiftUI
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

// View model for the activity list. Subscribes once via subscribe_query;
// stays live for the app's lifetime. Dropping `subscription` auto-unsubscribes.
@MainActor
class ActivitiesViewModel: ObservableObject {
    @Published var activities: [FfiActivity] = []
    private var subscription: FfiQuerySubscription?
    private var core: GainzvilleCore?

    func subscribe(to core: GainzvilleCore) {
        self.core = core
        subscription = try? core.subscribeQuery(query: .allActivities)
        refresh(from: core)
    }

    func refresh(from core: GainzvilleCore) {
        if case .allActivities(let list) = core.readQuery(query: .allActivities) {
            activities = list
        }
    }

    func createActivity(name: String, description: String?) {
        guard let core else { return }
        try? core.runAction(action: .createActivity(FfiCreateActivity(
            id: UUID().uuidString,
            name: name,
            description: description
        )))
        // No manual refresh needed — runAction refreshes the cache and fires
        // on_data_changed, which triggers refresh via AppListener.
    }
}

// View model for the forest. Subscribes via subscribeForest (backed by AllEntries cache).
// Exposes root entries as a @Published property; children are read synchronously on demand.
@MainActor
class ForestViewModel: ObservableObject {
    @Published var roots: [FfiEntry] = []
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

    func rootsIn(logDay: LogDay) -> [FfiEntry] {
        _ = roots  // establish SwiftUI dependency so callers re-render when data changes
        return core?.forestRootsIn(from: logDay.fromMs, to: logDay.toMs) ?? []
    }

    func children(of parentId: String) -> [FfiEntry] {
        core?.forestChildren(parentId: parentId) ?? []
    }

    func updateEntryTemporal(entry: FfiEntry, temporal: FfiTemporal) {
        guard let core else { return }
        try? core.runAction(action: .moveEntry(FfiMoveEntry(
            entryId: entry.id,
            position: entry.position,
            temporal: temporal
        )))
    }

    func createRootEntry(activityId: String?, name: String?, isSequence: Bool, for logDay: LogDay) {
        guard let core else { return }
        let suggestedMs = core.forestSuggestedRootDayInsertionTime(dayStart: logDay.fromMs)
        try? core.runAction(action: .createEntry(FfiCreateEntry(
            id: UUID().uuidString,
            activityId: activityId,
            name: name,
            position: nil,
            isTemplate: false,
            displayAsSets: false,
            isSequence: isSequence,
            isComplete: false,
            temporal: .start(start: suggestedMs)
        )))
    }
}

@MainActor
class AttributesViewModel: ObservableObject {
    @Published var attributes: [FfiAttribute] = []
    private var subscription: FfiQuerySubscription?

    func subscribe(to core: GainzvilleCore) {
        subscription = try? core.subscribeQuery(query: .allAttributes)
        refresh(from: core)
    }

    func refresh(from core: GainzvilleCore) {
        if case .allAttributes(let list) = core.readQuery(query: .allAttributes) {
            attributes = list
        }
    }
}

func makeCore(listener: AppListener) throws -> GainzvilleCore {
    let dbURL = FileManager.default
        .urls(for: .documentDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("gainzville.sqlite")

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
