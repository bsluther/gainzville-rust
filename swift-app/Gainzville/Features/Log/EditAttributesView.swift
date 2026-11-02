import SwiftUI

struct EditAttributesView: View {
    let entry: Entry
    let entryName: String
    /// Whether the entry belongs to an activity. The "All entries" (activity
    /// default) column is gated on this; it is not rendered until Phase 3, so
    /// the flag is currently carried but unused.
    let hasActivity: Bool
    @Binding var isPresented: Bool

    /// All attributes owned by the entry's owner.
    @GvQuery private var attributes: [Attribute]
    @GvQuery private var entryJoin: EntryJoin?
    /// The activity's template root, for the "All entries" column. Inactive if
    /// the entry has no activity.
    @GvQuery private var templateJoin: EntryJoin?
    @Environment(\.gvClient) private var gv

    @State private var searchText = ""
    // Display order is snapshotted once (entry-attached first, then alphabetical)
    // and frozen for the lifetime of the view so toggling never reorders rows.
    // Attributes created mid-session append to the end, never reorder existing rows.
    @State private var orderedIds: [String] = []
    @State private var hasSnapshot = false

    init(entry: Entry, entryName: String, hasActivity: Bool, isPresented: Binding<Bool>) {
        self.entry = entry
        self.entryName = entryName
        self.hasActivity = hasActivity
        _isPresented = isPresented
        _attributes = GvQuery(.attributesOwned(by: entry.ownerId))
        _entryJoin = GvQuery(.entryJoin(entry.id))
        _templateJoin = GvQuery(.activityTemplateJoin(entry.activityId))
    }

    private var attachedIds: Set<String> {
        Set((entryJoin?.attributes ?? []).map { $0.id })
    }

    private var activityAttachedIds: Set<String> {
        Set((templateJoin?.attributes ?? []).map { $0.id })
    }

    /// Attach or detach on `entryId`. Both are idempotent in core, so the UI
    /// never needs to guard against double-toggles.
    private func toggle(_ attributeId: String, on entryId: String, attached: Bool) {
        if attached {
            gv?.removeAttribute(entryId: entryId, attributeId: attributeId)
        } else {
            gv?.attachValue(entryId: entryId, attributeId: attributeId)
        }
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
                ForEach(displayedAttributes, id: \.id) { attr in
                    attributeRow(attr)
                    Divider()
                }
            }
            .padding(.top, GvSpacing.md)
        }
        // Opaque dark fill on both platforms. On iOS this is a NavigationLink
        // destination pushed inside the entry-menu sheet, and pushed views get a
        // translucent system backing over the sheet's presentationBackground —
        // an opaque fill here is what overrides it to the correct gvBackground.
        // It does cover the sheet's presentation border (so this screen has no
        // border, unlike the menu), an accepted tradeoff: the border can't be
        // kept without presenting this as its own sheet rather than a push.
        .background(Color.gvBackground)
        .navigationTitle("Edit Attributes")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $searchText, prompt: "Search attributes")
        #endif
        #if os(macOS)
        .frame(minWidth: 340, minHeight: 440)
        #endif
        .onChange(of: attributes, initial: true) { rebuildOrderIfNeeded() }
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

    // MARK: - Ordering (frozen snapshot)

    /// Attributes to render: the frozen order, resolved to live `Attribute`
    /// values and filtered by the search text.
    private var displayedAttributes: [Attribute] {
        let byId = Dictionary(attributes.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return orderedIds.compactMap { byId[$0] }.filter {
            searchText.isEmpty || $0.name.localizedCaseInsensitiveContains(searchText)
        }
    }

    /// Build the frozen order once the attribute list first arrives; afterward
    /// only append newly-created attributes at the end.
    private func rebuildOrderIfNeeded() {
        let all = attributes
        guard !all.isEmpty else { return }
        let attachedIds = attachedIds
        let activityAttachedIds = activityAttachedIds
        let byName: (Attribute, Attribute) -> Bool = {
            $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
        }
        if !hasSnapshot {
            // Group order (frozen at open): attributes on this entry first, then
            // attributes on the activity (template), then the rest — each group
            // alphabetical.
            let onEntry = all.filter { attachedIds.contains($0.id) }.sorted(by: byName)
            let onActivity = all
                .filter { !attachedIds.contains($0.id) && activityAttachedIds.contains($0.id) }
                .sorted(by: byName)
            let rest = all
                .filter { !attachedIds.contains($0.id) && !activityAttachedIds.contains($0.id) }
                .sorted(by: byName)
            orderedIds = onEntry.map(\.id) + onActivity.map(\.id) + rest.map(\.id)
            hasSnapshot = true
        } else {
            let known = Set(orderedIds)
            let newOnes = all.filter { !known.contains($0.id) }.sorted(by: byName)
            orderedIds.append(contentsOf: newOnes.map(\.id))
        }
    }

    // MARK: - Rows

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
                if hasActivity {
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

    private func attributeRow(_ attr: Attribute) -> some View {
        HStack {
            Text(attr.name)
                .font(.gvBody)
                .foregroundStyle(Color.gvTextPrimary)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 0) {
                GvCheckbox(checked: attachedIds.contains(attr.id)) {
                    toggle(attr.id, on: entry.id, attached: attachedIds.contains(attr.id))
                }
                .frame(width: 44)
                if hasActivity {
                    // Under materialization this affects future instances only,
                    // not existing log entries.
                    GvCheckbox(checked: activityAttachedIds.contains(attr.id)) {
                        if let rootId = templateJoin?.entry.id {
                            toggle(attr.id, on: rootId, attached: activityAttachedIds.contains(attr.id))
                        }
                    }
                    .frame(width: 44)
                }
            }
        }
        .padding(.vertical, GvSpacing.sm)
        .padding(.horizontal, GvSpacing.lg)
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
}
