//
//  ProofOfConceptView.swift
//  Gainzville
//
//  Created by Brian Luther on 4/9/26.
//

import SwiftUI

struct ProofOfConceptView: View {
    let core: GainzvilleCore
    @ObservedObject var viewModel: ActivitiesViewModel

    @State private var newName: String = ""
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                List(viewModel.activities, id: \.id) { activity in
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
            // No onAppear refresh needed — subscribe_activities yields the initial
            // snapshot immediately and keeps the list live thereafter.
        }
    }

    private func createActivity() {
        do {
            try core.runAction(action: .createActivity(FfiCreateActivity(
                id: UUID().uuidString,
                name: newName,
                description: nil
            )))
            newName = ""
            // No manual refresh — run_action broadcasts on the change channel,
            // which wakes the stream and calls onActivitiesChanged automatically.
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
