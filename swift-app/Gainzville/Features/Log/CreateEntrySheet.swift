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

    private var filteredActivities: [Activity] {
        guard !searchText.isEmpty else { return activitiesVM.activities }
        let q = searchText.lowercased()
        return activitiesVM.activities.filter { $0.name.lowercased().contains(q) }
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: GvSpacing.xl) {
                    #if os(macOS)
                    // macOS shows the title as in-content text rather than via
                    // .navigationTitle. The macOS navigation title bar is a
                    // focus-reactive material strip whose color can't be made to
                    // match the rest of the sheet chrome (toolbar materials sit
                    // above any background we set — verified: presentationBackground,
                    // containerBackground, and toolbarBackground all had no effect).
                    // Dropping it leaves only the search + Cancel toolbar strips,
                    // which share one consistent chrome color.
                    Text("Create Entry")
                        .font(.gvTitle)
                        .fontWeight(.bold)
                        .foregroundStyle(Color.gvTextPrimary)
                        .padding(.top, GvSpacing.xl)
                    #endif
                    scratchSection
                    librarySection
                }
                .padding(.horizontal, GvSpacing.lg)
                .padding(.top, GvSpacing.lg)
                .padding(.bottom, GvSpacing.xl)
            }
            .background(Color.gvBackground)
            #if os(iOS)
            // iOS keeps the system search bar. macOS uses an in-content search
            // field inside librarySection instead — .searchable forces the field
            // into the toolbar strip flush to its edge, with no way to pad it.
            .searchable(text: $searchText, prompt: "Search activities")
            .navigationTitle("Create Entry")
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
        #else
        // Fix the macOS sheet size so filtering the library list doesn't resize
        // the whole sheet; the inner ScrollView absorbs overflow instead.
        .frame(width: 480, height: 600)
        #endif
    }

    // MARK: - Pick from library

    private var librarySection: some View {
        VStack(alignment: .leading, spacing: GvSpacing.md) {
            sectionHeader("PICK FROM LIBRARY")

            #if os(macOS)
            macSearchField
            #endif

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

    #if os(macOS)
    // In-content replacement for the toolbar .searchable on macOS. Styled to
    // match the Entry name field (plain field, gvSurface fill, rounded border).
    private var macSearchField: some View {
        HStack(spacing: GvSpacing.md) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(Color.gvTextSecondary)
            TextField("Search activities", text: $searchText)
                .textFieldStyle(.plain)
                .font(.gvBody)
                .foregroundStyle(Color.gvTextPrimary)
        }
        .padding(.horizontal, GvSpacing.lg)
        .padding(.vertical, GvSpacing.md)
        .background(Color.gvSurface)
        .clipShape(RoundedRectangle(cornerRadius: GvSpacing.entryScalarCornerRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GvSpacing.entryScalarCornerRadius)
                .stroke(Color.entryScalarBorder, lineWidth: GvSpacing.entryScalarBorderWidth)
        )
    }
    #endif

    // MARK: - Create from scratch

    private var scratchSection: some View {
        VStack(alignment: .leading, spacing: GvSpacing.md) {
            sectionHeader("CREATE FROM SCRATCH")

            HStack(spacing: GvSpacing.lg) {
                Text("Entry name")
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextPrimary)
                TextField("", text: $scratchName)
                    // .plain strips the macOS NSTextField bezel/focus ring, which
                    // otherwise draws a lighter filled rectangle inside our custom
                    // gvSurface container. iOS has no such bezel, so this also
                    // makes both platforms render only our background + border.
                    .textFieldStyle(.plain)
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

