import SwiftUI

// Sheet for creating a new entry. Parameterized via `onCommit` so it can be
// presented from the log root level (temporal = suggested day time, position = nil)
// or from within a sequence (temporal = none, position = parent + fracIndex).
struct CreateEntrySheet: View {
    @Binding var isPresented: Bool
    /// Called when the user finalizes a selection.
    /// activityId: set when picking from library; name: set for anonymous entries.
    var onCommit: (_ activityId: String?, _ name: String?, _ isSequence: Bool) -> Void

    @EnvironmentObject private var activitiesVM: ActivitiesViewModel

    @State private var searchText = ""
    @State private var scratchName = ""
    @State private var scratchIsSequence = false

    private var filteredActivities: [FfiActivity] {
        guard !searchText.isEmpty else { return activitiesVM.activities }
        let q = searchText.lowercased()
        return activitiesVM.activities.filter { $0.name.lowercased().contains(q) }
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: GvSpacing.xl) {
                    scratchSection
                    librarySection
                }
                .padding(.horizontal, GvSpacing.lg)
                .padding(.top, GvSpacing.lg)
                .padding(.bottom, GvSpacing.xl)
            }
            .searchable(text: $searchText, prompt: "Search activities")
            .background(Color.gvBackground)
            .navigationTitle("Create Entry")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { isPresented = false }
                }
            }
        }
        #if os(iOS)
        .presentationDetents([.large])
        #endif
    }

    // MARK: - Pick from library

    private var librarySection: some View {
        VStack(alignment: .leading, spacing: GvSpacing.md) {
            sectionHeader("PICK FROM LIBRARY")

            if filteredActivities.isEmpty {
                Text(searchText.isEmpty ? "No activities in library." : "No matches.")
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextSecondary)
                    .padding(.vertical, GvSpacing.md)
            } else {
                VStack(spacing: 0) {
                    ForEach(filteredActivities, id: \.id) { activity in
                        Button {
                            onCommit(activity.id, nil, false)
                        } label: {
                            HStack {
                                Text(activity.name)
                                    .font(.gvBody)
                                    .foregroundStyle(Color.gvTextPrimary)
                                Spacer()
                            }
                            .padding(.horizontal, GvSpacing.lg)
                            .padding(.vertical, GvSpacing.lg)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)

                        if activity.id != filteredActivities.last?.id {
                            Rectangle()
                                .fill(Color.gvNeutral800)
                                .frame(height: 0.5)
                        }
                    }
                }
                .background(Color.gvSurface)
                .clipShape(RoundedRectangle(cornerRadius: GvSpacing.entryScalarCornerRadius))
                .overlay(
                    RoundedRectangle(cornerRadius: GvSpacing.entryScalarCornerRadius)
                        .stroke(Color.entryScalarBorder, lineWidth: GvSpacing.entryScalarBorderWidth)
                )
            }
        }
    }

    // MARK: - Create from scratch

    private var scratchSection: some View {
        VStack(alignment: .leading, spacing: GvSpacing.md) {
            sectionHeader("CREATE FROM SCRATCH")

            HStack(spacing: GvSpacing.lg) {
                Text("Entry name")
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextPrimary)
                TextField("", text: $scratchName)
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextPrimary)
                    .padding(.horizontal, GvSpacing.lg)
                    .padding(.vertical, GvSpacing.md)
                    .background(Color.gvSurface)
                    .clipShape(RoundedRectangle(cornerRadius: GvSpacing.entryScalarCornerRadius))
                    .overlay(
                        RoundedRectangle(cornerRadius: GvSpacing.entryScalarCornerRadius)
                            .stroke(Color.entryScalarBorder, lineWidth: GvSpacing.entryScalarBorderWidth)
                    )
            }

            HStack {
                GvCheckbox(checked: scratchIsSequence) {
                    scratchIsSequence.toggle()
                }
                Text("Sequence")
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextPrimary)
                    .onTapGesture { scratchIsSequence.toggle() }

                Spacer()

                Button("Create") {
                    let trimmed = scratchName.trimmingCharacters(in: .whitespaces)
                    onCommit(nil, trimmed, scratchIsSequence)
                }
                .buttonStyle(.borderedProminent)
                .disabled(scratchName.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    // MARK: - Helpers

    private func sectionHeader(_ text: String) -> some View {
        Text(text)
            .font(.gvCaption)
            .foregroundStyle(Color.gvTextSecondary)
    }
}

