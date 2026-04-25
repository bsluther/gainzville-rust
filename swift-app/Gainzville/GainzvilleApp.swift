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
    let forestVM: ForestViewModel
    let logDayStore: LogDayStore

    init() {
        let listener = AppListener()
        let c = try! makeCore(listener: listener)

        let avm = ActivitiesViewModel()
        avm.subscribe(to: c)

        let atvm = AttributesViewModel()
        atvm.subscribe(to: c)

        let fvm = ForestViewModel()
        fvm.subscribe(to: c)

        // Wire the listener callback after all view models exist.
        listener.onChanged = { [weak avm, weak atvm, weak fvm] in
            Task { @MainActor [weak avm, weak atvm, weak fvm] in
                avm?.refresh(from: c)
                atvm?.refresh(from: c)
                fvm?.refresh(from: c)
            }
        }

        // Debug seeds — uncomment once on a fresh DB, then re-comment.
        // try? c.devSeedStdLib()                       // creates Reps, Load, Outcome, YDS Grade attrs
        // try? c.devCreateArbitraryEntries(count: 20)  // requires at least one activity to exist
        // Debug: uncomment to auto-create an activity every 10 s for testing.
        // c.startBackgroundTicker()
        core = c
        activitiesVM = avm
        attributesVM = atvm
        forestVM = fvm
        logDayStore = LogDayStore()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(activitiesVM)
                .environmentObject(attributesVM)
                .environmentObject(forestVM)
                .environmentObject(logDayStore)
        }
        #if os(macOS)
        .defaultSize(width: 1100, height: 700)
        #endif
    }
}
