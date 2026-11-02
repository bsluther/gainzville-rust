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

func makeCore() throws -> GainzvilleCore {
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
        actorId: SYSTEM_ACTOR_ID
    )
}
