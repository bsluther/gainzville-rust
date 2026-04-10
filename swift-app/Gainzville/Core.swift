//
//  Core.swift
//  Gainzville
//
//  Created by Brian Luther on 4/9/26.
//

import Foundation
import SwiftUI
internal import Combine

// No-op CoreListener — still required by GainzvilleCore's constructor.
// Live updates flow through ActivitiesListenerBridge / subscribe_activities instead.
class AppListener: CoreListener {
    func onDataChanged() {}
}

// Bridges ActivitiesListener callbacks from Rust's background thread to the main thread.
class ActivitiesListenerBridge: ActivitiesListener {
    private let callback: ([FfiActivity]) -> Void

    init(_ callback: @escaping ([FfiActivity]) -> Void) {
        self.callback = callback
    }

    func onActivitiesChanged(activities: [FfiActivity]) {
        DispatchQueue.main.async { self.callback(activities) }
    }
}

// View model for the activity list. Subscribes once; stays live for the app's lifetime.
class ActivitiesViewModel: ObservableObject {
    @Published var activities: [FfiActivity] = []

    // Holds the bridge strongly — Rust's spawn also holds it, but keeping it here
    // makes the lifetime explicit and avoids relying on UniFFI's internal refcount.
    private var bridge: ActivitiesListenerBridge?

    func subscribe(to core: GainzvilleCore) {
        let bridge = ActivitiesListenerBridge { [weak self] activities in
            self?.activities = activities
        }
        self.bridge = bridge
        core.subscribeActivities(listener: bridge)
        core.startBackgroundTicker()
    }
}

func makeCore() throws -> GainzvilleCore {
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
        listener: AppListener()
    )
}
