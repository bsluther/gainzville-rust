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
    let viewModel: ActivitiesViewModel

    init() {
        let listener = AppListener()
        let c = try! makeCore(listener: listener)
        let vm = ActivitiesViewModel()
        vm.subscribe(to: c)
        // Wire the listener callback after both core and viewModel exist.
        listener.onChanged = { [weak vm] in
            Task { @MainActor [weak vm] in
                vm?.refresh(from: c)
            }
        }
        c.startBackgroundTicker()
        core = c
        viewModel = vm
    }

    var body: some Scene {
        WindowGroup {
            ProofOfConceptView(core: core, viewModel: viewModel)
        }
    }
}
