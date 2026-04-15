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
    let activitiesVM: ActivitiesViewModel
    let attributesVM: AttributesViewModel

    init() {
        let listener = AppListener()
        let c = try! makeCore(listener: listener)

        let avm = ActivitiesViewModel()
        avm.subscribe(to: c)

        let atvm = AttributesViewModel()
        atvm.subscribe(to: c)

        // Wire the listener callback after all view models exist.
        listener.onChanged = { [weak avm, weak atvm] in
            Task { @MainActor [weak avm, weak atvm] in
                avm?.refresh(from: c)
                atvm?.refresh(from: c)
            }
        }

        c.startBackgroundTicker()
        core = c
        activitiesVM = avm
        attributesVM = atvm
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(activitiesVM)
                .environmentObject(attributesVM)
        }
        #if os(macOS)
        .defaultSize(width: 1100, height: 700)
        #endif
    }
}
