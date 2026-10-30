//
//  GvQueries.swift
//  GvKit
//
//  The questions views can ask, as typed keys for `@GvQuery` and
//  `GvClient.read`. Nothing here says how a question is answered: child
//  slots and cycle checks come from Rust's in-memory forest, day roots and
//  entry joins from SQL, and Rust may change that without touching Swift.
//

import Foundation

extension GvKey where Value == [ChildSlot] {
    /// A sequence's children laid out for display: drop, child, drop, …, drop.
    static func childSlots(of parentId: String) -> GvKey<[ChildSlot]> {
        GvKey(query: .childSlots(parentId: parentId), defaultValue: []) { value in
            if case .childSlots(let slots) = value { return slots }
            return nil
        }
    }
}

extension GvKey where Value == [Entry] {
    /// Log roots on a local day, sorted by time.
    static func dayRoots(_ day: LogDay) -> GvKey<[Entry]> {
        let query = RootEntriesInInterval(from: day.fromMs, to: day.toMs)
        return GvKey(query: .domain(.rootEntriesInInterval(query)), defaultValue: []) { value in
            if case .domain(.rootEntriesInInterval(let roots)) = value { return roots }
            return nil
        }
    }
}

extension GvKey where Value == EntryJoin? {
    /// An entry with its activity and attributes. `nil` id = inactive.
    static func entryJoin(_ entryId: String?) -> GvKey<EntryJoin?> {
        let query = entryId.map { LiveQuery.domain(.findEntryJoinById(FindEntryJoinById(entryId: $0))) }
        return GvKey(query: query, defaultValue: nil) { value in
            if case .domain(.findEntryJoinById(let join)) = value { return .some(join) }
            return nil
        }
    }
}

extension GvKey where Value == Bool {
    /// Would moving `entryId` under `proposedParentId` create a cycle? Read
    /// once at drag time.
    static func wouldCreateCycle(entryId: String, under proposedParentId: String) -> GvKey<Bool> {
        GvKey(
            query: .wouldCreateCycle(entryId: entryId, proposedParentId: proposedParentId),
            defaultValue: false
        ) { value in
            if case .wouldCreateCycle(let cycle) = value { return cycle }
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
