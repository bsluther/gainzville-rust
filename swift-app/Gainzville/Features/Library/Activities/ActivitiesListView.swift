import SwiftUI

struct ActivitiesListView: View {
    let activities: [FfiActivity]
    @EnvironmentObject var activitiesVM: ActivitiesViewModel
    @State private var showingCreate = false

    var body: some View {
        Group {
            if activities.isEmpty {
                ContentUnavailableView(
                    "No Activities",
                    systemImage: "figure.run",
                    description: Text("Tap + to create your first activity.")
                )
            } else {
                List(activities, id: \.id) { activity in
                    NavigationLink(value: LibraryDestination.activity(activity)) {
                        VStack(alignment: .leading, spacing: GvSpacing.sm) {
                            Text(activity.name)
                                .font(.gvBody)
                            if let desc = activity.description {
                                Text(desc)
                                    .font(.gvCaption)
                                    .foregroundStyle(Color.gvTextSecondary)
                            }
                        }
                        .padding(.vertical, GvSpacing.sm)
                    }
                }
                .listStyle(.plain)
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    showingCreate = true
                } label: {
                    Image(systemName: "plus")
                }
            }
        }
        .sheet(isPresented: $showingCreate) {
            CreateActivityView { name, description in
                activitiesVM.createActivity(name: name, description: description)
            }
        }
    }
}

// MARK: - Create Activity sheet

private struct CreateActivityView: View {
    var onCreate: (String, String?) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var description = ""

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Name", text: $name)
                }
                Section {
                    TextField("Description (optional)", text: $description, axis: .vertical)
                        .lineLimit(3...6)
                }
            }
            .navigationTitle("New Activity")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") {
                        onCreate(name, description.isEmpty ? nil : description)
                        dismiss()
                    }
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }
}
