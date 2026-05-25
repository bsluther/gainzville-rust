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

    func rootsIn(logDay: LogDay) -> [Entry] {
        _ = roots  // establish SwiftUI dependency so callers re-render when data changes
        return core?.forestRootsIn(from: logDay.fromMs, to: logDay.toMs) ?? []
    }

    func children(of parentId: String) -> [Entry] {
        core?.forestChildren(parentId: parentId) ?? []
    }

    func wouldCreateCycle(entryId: String, proposedParentId: String) -> Bool {
        _ = roots
        return core?.forestWouldCreateCycle(entryId: entryId, proposedParentId: proposedParentId) ?? false
    }

    func positionBetween(parentId: String, predId: String?, succId: String?) -> Position? {
        _ = roots
        return core?.forestPositionBetween(parentId: parentId, predId: predId, succId: succId)
    }

    func moveEntry(_ entry: Entry, to position: Position) {
        guard let core else { return }
        try? core.runAction(action: .moveEntry(MoveEntry(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entry.id,
            position: position,
            temporal: entry.temporal
        )))
    }

    func updateEntryTemporal(entry: Entry, temporal: Temporal) {
        guard let core else { return }
        try? core.runAction(action: .moveEntry(MoveEntry(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entry.id,
            position: entry.position,
            temporal: temporal
        )))
    }

    /// Move an entry to root on the given day with a chosen start time.
    /// Preserves duration if set; replaces any prior start and drops any prior end.
    func moveEntryToRoot(_ entry: Entry, startTime: Date) {
        guard let core else { return }
        let ms = Int64(startTime.timeIntervalSince1970 * 1000)
        let newTemporal: Temporal = {
            switch entry.temporal {
            case .none, .start, .end, .startAndEnd:
                return .start(start: ms)
            case .duration(let d),
                 .startAndDuration(_, let d),
                 .durationAndEnd(let d, _):
                return .startAndDuration(start: ms, durationMs: d)
            }
        }()
        try? core.runAction(action: .moveEntry(MoveEntry(
            actorId: SYSTEM_ACTOR_ID,
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
        field: ValueField,
        value: AttributeValue
    ) {
        guard let core else { return }
        try? core.runAction(action: .updateAttributeValue(UpdateAttributeValue(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entryId,
            attributeId: attributeId,
            field: field,
            value: value
        )))
    }

    func createRootEntry(activityId: String?, name: String?, isSequence: Bool, for logDay: LogDay) {
        guard let core else { return }
        let suggestedMs = core.forestSuggestedRootDayInsertionTime(dayStart: logDay.fromMs)
        try? core.runAction(action: .createEntry(CreateEntry(
            actorId: SYSTEM_ACTOR_ID,
            entry: Entry(
                id: UUID().uuidString,
                activityId: activityId,
                ownerId: SYSTEM_ACTOR_ID,
                name: name,
                position: nil,
                isTemplate: false,
                displayAsSets: false,
                isSequence: isSequence,
                isComplete: false,
                temporal: .start(start: suggestedMs)
            )
        )))
    }

    func createChildEntry(in parent: Entry, activityId: String?, name: String?, isSequence: Bool) {
        guard let core else { return }
        guard let position = core.forestPositionAfterChildren(parentId: parent.id) else { return }
        try? core.runAction(action: .createEntry(CreateEntry(
            actorId: SYSTEM_ACTOR_ID,
            entry: Entry(
                id: UUID().uuidString,
                activityId: activityId,
                ownerId: SYSTEM_ACTOR_ID,
                name: name,
                position: position,
                isTemplate: false,
                displayAsSets: false,
                isSequence: isSequence,
                isComplete: false,
                temporal: .none
            )
        )))
    }

    func updateEntryCompletion(entry: Entry, isComplete: Bool) {
        guard let core else { return }
        try? core.runAction(action: .updateEntryCompletion(UpdateEntryCompletion(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entry.id,
            isComplete: isComplete
        )))
    }

    func deleteEntry(entry: Entry) {
        guard let core else { return }
        try? core.runAction(action: .deleteEntryRecursive(DeleteEntryRecursive(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entry.id
        )))
    }
}

// Per-entry view model: subscribes once to `findEntryJoinById` for its
// entry id, re-reads from the cache on every DataChange tick, and
// publishes the latest EntryJoin. Dropping this VM (when the
// EntryView leaves the hierarchy) drops the FfiQuerySubscription,
// which auto-removes the query from the Rust cache.
@MainActor
class EntryViewModel: ObservableObject {
    @Published var entryJoin: EntryJoin?
    private var subscription: FfiQuerySubscription?
    private var cancellable: AnyCancellable?
    private var entryId: String?
    private var core: GainzvilleCore?

    func start(core: GainzvilleCore, dataChange: DataChange, entryId: String) {
        guard self.entryId == nil else { return }
        self.core = core
        self.entryId = entryId
        subscription = try? core.subscribeQuery(query: .findEntryJoinById(FindEntryJoinById(entryId: entryId)))
        refresh()
        cancellable = dataChange.didChange.sink { [weak self] in
            self?.refresh()
        }
    }

    private func refresh() {
        guard let core, let entryId else { return }
        if case .findEntryJoinById(let join) = core.readQuery(query: .findEntryJoinById(FindEntryJoinById(entryId: entryId))) {
            entryJoin = join
        }
    }
}

// TODO: AttributesViewModel does not subscribe to DataChange.didChange, so the
// library attribute list (AttributesListView) won't live-refresh when an
// attribute is created or edited — it only reflects what was cached at subscribe
// time. Wire a DataChange sink that calls refresh(from:) like EntryViewModel /
// EditAttributesViewModel do. Surfaces once name/description editing lands (the
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
