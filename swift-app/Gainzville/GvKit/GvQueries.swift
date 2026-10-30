//
//  GvQueries.swift
//  GvKit
//
//  The questions views can ask, as typed keys for `@GvQuery` and
//  `GvClient.read`. Each key names one member of the client set
//  (`ClientQuery`, generated from `gv_client::queries`) and picks its answer
//  out of the matching `ClientQueryResponse` case. Nothing here says how a
//  question is answered: child slots, cycle checks and insertion times come
//  from Rust's in-memory forest, day roots and entry joins from SQL, and Rust
//  may change that without touching Swift.
//

import Foundation

extension GvKey where Value == [ChildSlot] {
    /// A sequence's children laid out for display: drop, child, drop, …, drop.
    static func childSlots(of parentId: String) -> GvKey<[ChildSlot]> {
        GvKey(query: .childSlots(ChildSlots(parentId: parentId)), defaultValue: []) { response in
            if case .childSlots(let slots) = response { return slots }
            return nil
        }
    }
}

extension GvKey where Value == [Entry] {
    /// Log roots on a local day, sorted by time.
    static func dayRoots(_ day: LogDay) -> GvKey<[Entry]> {
        let query = RootEntriesInInterval(from: day.fromMs, to: day.toMs)
        return GvKey(query: .rootEntriesInInterval(query), defaultValue: []) { response in
            if case .rootEntriesInInterval(let roots) = response { return roots }
            return nil
        }
    }
}

extension GvKey where Value == EntryJoin? {
    /// An entry with its activity and attributes. `nil` id = inactive.
    static func entryJoin(_ entryId: String?) -> GvKey<EntryJoin?> {
        let query = entryId.map { ClientQuery.findEntryJoinById(FindEntryJoinById(entryId: $0)) }
        return GvKey(query: query, defaultValue: nil) { response in
            if case .findEntryJoinById(let join) = response { return .some(join) }
            return nil
        }
    }
}

extension GvKey where Value == Bool {
    /// Would moving `entryId` under `proposedParentId` create a cycle? Read
    /// once at drag time.
    static func wouldCreateCycle(entryId: String, under proposedParentId: String) -> GvKey<Bool> {
        GvKey(
            query: .wouldCreateCycle(WouldCreateCycle(entryId: entryId, proposedParentId: proposedParentId)),
            defaultValue: false
        ) { response in
            if case .wouldCreateCycle(let cycle) = response { return cycle }
            return nil
        }
    }
}

extension GvKey where Value == UtcDateTime {
    /// When a new log root on `day` should start (Unix ms): now if `day` is
    /// today, else just after its last root, else noon. Depends on the clock,
    /// so read it once at the moment of use; don't subscribe.
    static func suggestedRootInsertionTime(on day: LogDay) -> GvKey<UtcDateTime> {
        let noon = day.fromMs + 12 * 3_600_000
        return GvKey(
            query: .suggestedRootInsertionTime(SuggestedRootInsertionTime(dayStart: day.fromMs)),
            defaultValue: noon
        ) { response in
            if case .suggestedRootInsertionTime(let ms) = response { return ms }
            return nil
        }
    }
}

// Slot identity comes from Rust (drop ids are derived from the neighbours;
// entry slots use the entry id), so SwiftUI keeps per-slot state attached to
// the right gap across moves.
extension ChildSlot: Identifiable {
    public var id: String {
        switch self {
        case .drop(let id, _, _, _): return id
        case .entry(let entry): return entry.id
        }
    }
}
