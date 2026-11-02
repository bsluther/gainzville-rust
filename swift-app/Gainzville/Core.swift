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
