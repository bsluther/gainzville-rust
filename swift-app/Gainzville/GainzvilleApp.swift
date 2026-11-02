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
    let gvClient: GvClient
    let coreEnv: CoreEnv
    let dataChange: DataChange
    let entryExpansion: EntryExpansion
    let logDayStore: LogDayStore
    let dragState: DragState
    let attributeFocus: AttributeFocusModel
    let autocomplete: AutocompleteCoordinator

    init() {
        let listener = AppListener()
        let c = try! makeCore(listener: listener)
        // Live queries (GvKit). Registers the batch listener, so it must exist
        // before any @GvQuery subscribes.
        let gv = GvClient(core: c)

        let dc = DataChange()

        listener.onChanged = { [weak dc] in
            Task { @MainActor [weak dc] in
                dc?.bump()
            }
        }

        // Debug seeds — uncomment once on a fresh DB, then re-comment.
//         try? c.devSeedStdLib()                       // creates Reps, Load, Outcome, YDS Grade attrs
//          try? c.devCreateArbitraryEntries(count: 20)  // requires at least one activity to exist
//          try? c.devCreateArbitraryValues(count: 40)   // requires at least one entry and one attribute
        // Debug: uncomment to auto-create an activity every 10 s for testing.
        // c.startBackgroundTicker()
        core = c
        gvClient = gv
        coreEnv = CoreEnv(core: c)
        dataChange = dc
        entryExpansion = EntryExpansion()
        logDayStore = LogDayStore()
        dragState = DragState()
        attributeFocus = AttributeFocusModel()
        autocomplete = AutocompleteCoordinator()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(\.gvClient, gvClient)
                .environmentObject(coreEnv)
                .environmentObject(dataChange)
                .environment(entryExpansion)
                .environmentObject(logDayStore)
                .environmentObject(dragState)
                .environmentObject(attributeFocus)
                .environmentObject(autocomplete)
                #if os(macOS)
                // The per-view `.background(Color.gvBackground)` modifiers paint
                // the sidebar and detail correctly, but the bare window region
                // around the toolbar isn't covered by any view and falls through
                // to the (black) NSWindow content background. This fills that
                // surface so the whole window reads as gvBackground. (The toolbar
                // material still lightens it slightly — native macOS behavior.)
                .containerBackground(Color.gvBackground, for: .window)
                #endif
        }
        #if os(macOS)
        .defaultSize(width: 1100, height: 700)
        #endif
    }
}
