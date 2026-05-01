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

// Holds the GainzvilleCore for env-based access in views that need to
// create their own subscriptions (e.g. per-entry EntryViewModel). Not
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

    func wouldCreateCycle(entryId: String, proposedParentId: String) -> Bool {
        _ = roots
        return core?.forestWouldCreateCycle(entryId: entryId, proposedParentId: proposedParentId) ?? false
    }

    func positionBetween(parentId: String, predId: String?, succId: String?) -> FfiPosition? {
        _ = roots
        return core?.forestPositionBetween(parentId: parentId, predId: predId, succId: succId)
    }

    func moveEntry(_ entry: FfiEntry, to position: FfiPosition) {
        guard let core else { return }
        try? core.runAction(action: .moveEntry(FfiMoveEntry(
            entryId: entry.id,
            position: position,
            temporal: entry.temporal
        )))
    }

    func updateEntryTemporal(entry: FfiEntry, temporal: FfiTemporal) {
        guard let core else { return }
        try? core.runAction(action: .moveEntry(FfiMoveEntry(
            entryId: entry.id,
            position: entry.position,
            temporal: temporal
        )))
    }

    /// Move an entry to root on the given day with a chosen start time.
    /// Preserves duration if set; replaces any prior start and drops any prior end.
    func moveEntryToRoot(_ entry: FfiEntry, startTime: Date) {
        guard let core else { return }
        let ms = Int64(startTime.timeIntervalSince1970 * 1000)
        let newTemporal: FfiTemporal = {
            switch entry.temporal {
            case .none, .start, .end, .startAndEnd:
                return .start(start: ms)
            case .duration(let d),
                 .startAndDuration(_, let d),
                 .durationAndEnd(let d, _):
                return .startAndDuration(start: ms, durationMs: d)
            }
        }()
        try? core.runAction(action: .moveEntry(FfiMoveEntry(
            entryId: entry.id,
            position: nil,
            temporal: newTemporal
        )))
    }

    /// Suggested initial time for placing a root entry on `day`.
    /// Wraps the same FFI helper used by `createRootEntry`.
    func suggestedRootInsertionTime(for day: LogDay) -> Date? {
        guard let core else { return nil }
        let ms = core.forestSuggestedRootDayInsertionTime(dayStart: day.fromMs)
        return Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
    }

    /// Update an entry's value for a given attribute. Today the Swift app only
    /// reaches this via attribute pairs returned by `FindAttributePairsForEntry`,
    /// so the underlying Value row is guaranteed to exist (per
    /// docs/attributes-design.md state 2/3). When the entry-attribute add UI
    /// lands, it will create the row first, then this can be called.
    func updateAttributeValue(
        entryId: String,
        attributeId: String,
        field: FfiValueField,
        value: FfiAttributeValue
    ) {
        guard let core else { return }
        try? core.runAction(action: .updateAttributeValue(FfiUpdateAttributeValue(
            entryId: entryId,
            attributeId: attributeId,
            field: field,
            value: value
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

    func createChildEntry(in parent: FfiEntry, activityId: String?, name: String?, isSequence: Bool) {
        guard let core else { return }
        guard let position = core.forestPositionAfterChildren(parentId: parent.id) else { return }
        try? core.runAction(action: .createEntry(FfiCreateEntry(
            id: UUID().uuidString,
            activityId: activityId,
            name: name,
            position: position,
            isTemplate: false,
            displayAsSets: false,
            isSequence: isSequence,
            isComplete: false,
            temporal: .none
        )))
    }

    func updateEntryCompletion(entry: FfiEntry, isComplete: Bool) {
        guard let core else { return }
        try? core.runAction(action: .updateEntryCompletion(FfiUpdateEntryCompletion(
            entryId: entry.id,
            isComplete: isComplete
        )))
    }

    func deleteEntry(entry: FfiEntry) {
        guard let core else { return }
        try? core.runAction(action: .deleteEntryRecursive(FfiDeleteEntryRecursive(
            entryId: entry.id
        )))
    }
}

// Per-entry view model: subscribes once to `findEntryJoinById` for its
// entry id, re-reads from the cache on every DataChange tick, and
// publishes the latest FfiEntryJoin. Dropping this VM (when the
// EntryView leaves the hierarchy) drops the FfiQuerySubscription,
// which auto-removes the query from the Rust cache.
@MainActor
class EntryViewModel: ObservableObject {
    @Published var entryJoin: FfiEntryJoin?
    private var subscription: FfiQuerySubscription?
    private var cancellable: AnyCancellable?
    private var entryId: String?
    private var core: GainzvilleCore?

    func start(core: GainzvilleCore, dataChange: DataChange, entryId: String) {
        guard self.entryId == nil else { return }
        self.core = core
        self.entryId = entryId
        subscription = try? core.subscribeQuery(query: .findEntryJoinById(entryId: entryId))
        refresh()
        cancellable = dataChange.didChange.sink { [weak self] in
            self?.refresh()
        }
    }

    private func refresh() {
        guard let core, let entryId else { return }
        if case .findEntryJoinById(let join) = core.readQuery(query: .findEntryJoinById(entryId: entryId)) {
            entryJoin = join
        }
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
