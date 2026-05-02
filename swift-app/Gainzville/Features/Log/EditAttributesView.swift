import SwiftUI

struct EditAttributesView: View {
    let entryName: String
    let activityName: String?
    @Binding var isPresented: Bool

    @State private var searchText = ""
    @State private var entryChecked: Set<String> = ["Reps", "Load"]
    @State private var activityChecked: Set<String> = ["Reps", "Load"]

    private let placeholderAttributes = [
        "Reps", "Load", "Notes", "Start time", "End time", "Distance", "Duration", "Tempo"
    ]

    private var filtered: [String] {
        guard !searchText.isEmpty else { return placeholderAttributes }
        return placeholderAttributes.filter { $0.localizedCaseInsensitiveContains(searchText) }
    }

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                #if os(macOS)
                macHeader
                Divider()
                macSearchField
                Divider()
                #endif
                columnHeaders
                Divider()
                ForEach(filtered, id: \.self) { name in
                    attributeRow(name: name)
                    Divider()
                }
            }
            .padding(.top, GvSpacing.md)
        }
        .background(Color.gvBackground)
        .navigationTitle("Edit Attributes")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $searchText, prompt: "Search attributes")
        #endif
        #if os(macOS)
        .frame(minWidth: 340, minHeight: 440)
        #endif
        .toolbar {
            #if os(iOS)
            ToolbarItem(placement: .principal) {
                VStack(spacing: 1) {
                    Text("Edit Attributes").font(.headline)
                    Text("for \(entryName)").font(.caption).foregroundStyle(Color.gvTextSecondary)
                }
            }
            #endif
            ToolbarItem(placement: .confirmationAction) {
                Button { isPresented = false } label: {
                    Image(systemName: "xmark")
                }
            }
        }
    }

    #if os(macOS)
    private var macHeader: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("Edit Attributes").font(.headline).foregroundStyle(Color.gvTextPrimary)
            Text("for \(entryName)").font(.caption).foregroundStyle(Color.gvTextSecondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, GvSpacing.md)
        .padding(.horizontal, GvSpacing.lg)
    }

    private var macSearchField: some View {
        HStack {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(Color.gvTextSecondary)
            TextField("Search attributes", text: $searchText)
                .textFieldStyle(.plain)
        }
        .padding(.vertical, GvSpacing.sm)
        .padding(.horizontal, GvSpacing.lg)
    }
    #endif

    private var columnHeaders: some View {
        HStack {
            Text("Attribute")
                .font(.gvCaption)
                .foregroundStyle(Color.gvTextSecondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 0) {
                Text("This entry")
                    .font(.gvCaption)
                    .foregroundStyle(Color.gvTextSecondary)
                    .multilineTextAlignment(.center)
                    .frame(width: 44)
                if let activityName {
                    Text("All entries")
                        .font(.gvCaption)
                        .foregroundStyle(Color.gvTextSecondary)
                        .multilineTextAlignment(.center)
                        .lineLimit(2)
                        .frame(width: 44)
                }
            }
        }
        .padding(.vertical, GvSpacing.md)
        .padding(.horizontal, GvSpacing.lg)
    }

    private func attributeRow(name: String) -> some View {
        HStack {
            Text(name)
                .font(.gvBody)
                .foregroundStyle(Color.gvTextPrimary)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 0) {
                GvCheckbox(checked: entryChecked.contains(name)) {
                    if entryChecked.contains(name) { entryChecked.remove(name) }
                    else { entryChecked.insert(name) }
                }
                .frame(width: 44)
                if activityName != nil {
                    GvCheckbox(checked: activityChecked.contains(name)) {
                        if activityChecked.contains(name) { activityChecked.remove(name) }
                        else { activityChecked.insert(name) }
                    }
                    .frame(width: 44)
                }
            }
        }
        .padding(.vertical, GvSpacing.sm)
        .padding(.horizontal, GvSpacing.lg)
    }
}
