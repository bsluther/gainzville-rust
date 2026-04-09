//
//  GainzvilleApp.swift
//  Gainzville
//
//  Created by Brian Luther on 4/9/26.
//

import SwiftUI

@main
struct GainzvilleApp: App {
    let core: GainzvilleCore
    
    init() {
        core = try! makeCore()
    }
    
    var body: some Scene {
        WindowGroup {
            ProofOfConceptView(core: core)
        }
    }
}
