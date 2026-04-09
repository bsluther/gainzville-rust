//
//  ProofOfConceptView.swift
//  Gainzville
//
//  Created by Brian Luther on 4/9/26.
//

import SwiftUI

struct ProofOfConceptView: View {
    let core: GainzvilleCore

    @State private var activities: [FfiActivity] = []
    @State private var newName: String = ""
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                List(activities, id: \.id) { activity in
                    VStack(alignment: .leading) {
                        Text(activity.name)
                        if let desc = activity.description {
                            Text(desc).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
                if let error = errorMessage {
                    Text(error).foregroundStyle(.red).padding()
                }
                HStack {
                    TextField("New activity name", text: $newName)
                        .textFieldStyle(.roundedBorder)
                    Button("Add") {
                        createActivity()
                    }
                    .disabled(newName.isEmpty)
                }
                .padding()
            }
            .navigationTitle("Activities")
            .onAppear { refresh() }
        }
    }

    private func refresh() {
        activities = core.getActivities()
    }

    private func createActivity() {
        do {
            try core.runAction(action: .createActivity(FfiCreateActivity(
                id: UUID().uuidString,
                name: newName,
                description: nil
            )))
            newName = ""
            refresh()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
