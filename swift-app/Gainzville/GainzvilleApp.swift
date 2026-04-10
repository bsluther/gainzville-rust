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
        let c = try! makeCore()
        let vm = ActivitiesViewModel()
        vm.subscribe(to: c)
        core = c
        viewModel = vm
    }

    var body: some Scene {
        WindowGroup {
            ProofOfConceptView(core: core, viewModel: viewModel)
        }
    }
}
