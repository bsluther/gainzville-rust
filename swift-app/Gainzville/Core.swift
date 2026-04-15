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

    func subscribe(to core: GainzvilleCore) {
        subscription = try? core.subscribeQuery(query: .allActivities)
        refresh(from: core)
    }

    func refresh(from core: GainzvilleCore) {
        if case .allActivities(let list) = core.readQuery(query: .allActivities) {
            activities = list
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
