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
    let coreEnv: CoreEnv
    let dataChange: DataChange
    let activitiesVM: ActivitiesViewModel
    let attributesVM: AttributesViewModel
    let forestVM: ForestViewModel
    let logDayStore: LogDayStore
    let dragState: DragState
    let attributeFocus: AttributeFocusModel

    init() {
        let listener = AppListener()
        let c = try! makeCore(listener: listener)

        let avm = ActivitiesViewModel()
        avm.subscribe(to: c)

        let atvm = AttributesViewModel()
        atvm.subscribe(to: c)

        let fvm = ForestViewModel()
        fvm.subscribe(to: c)

        let dc = DataChange()

        // Wire the listener callback after all view models exist.
        // Singleton VMs refresh first, then dataChange fans out to per-view
        // VMs (e.g. EntryViewModel) so they re-read the cache too.
        listener.onChanged = { [weak avm, weak atvm, weak fvm, weak dc] in
            Task { @MainActor [weak avm, weak atvm, weak fvm, weak dc] in
                avm?.refresh(from: c)
                atvm?.refresh(from: c)
                fvm?.refresh(from: c)
                dc?.bump()
            }
        }

        // Debug seeds — uncomment once on a fresh DB, then re-comment.
//         try? c.devSeedStdLib()                       // creates Reps, Load, Outcome, YDS Grade attrs
//         try? c.devCreateArbitraryEntries(count: 20)  // requires at least one activity to exist
//         try? c.devCreateArbitraryValues(count: 40)   // requires at least one entry and one attribute
        // Debug: uncomment to auto-create an activity every 10 s for testing.
        // c.startBackgroundTicker()
        core = c
        coreEnv = CoreEnv(core: c)
        dataChange = dc
        activitiesVM = avm
        attributesVM = atvm
        forestVM = fvm
        logDayStore = LogDayStore()
        dragState = DragState()
        attributeFocus = AttributeFocusModel()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(coreEnv)
                .environmentObject(dataChange)
                .environmentObject(activitiesVM)
                .environmentObject(attributesVM)
                .environmentObject(forestVM)
                .environmentObject(logDayStore)
                .environmentObject(dragState)
                .environmentObject(attributeFocus)
        }
        #if os(macOS)
        .defaultSize(width: 1100, height: 700)
        #endif
    }
}
