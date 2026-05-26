import SwiftUI
internal import Combine

// View model for EditAttributesView. Subscribes to the owner's full attribute
// list and to the entry's join (for the set of attributes currently attached),
// re-reading both on every DataChange tick. Attaching/detaching dispatch the
// AttachValue / DeleteAttributeValue actions; both are idempotent in core, so
// the UI never needs to guard against double-toggles.
@MainActor
final class EditAttributesViewModel: ObservableObject {
    @Published private(set) var attributes: [Attribute] = []  // all owned by the entry's owner
    @Published private(set) var attachedIds: Set<String> = []
    // Attributes attached to the activity's template root ("All entries" column).
    @Published private(set) var activityAttachedIds: Set<String> = []

    private var core: GainzvilleCore?
    private var ownerId: String = ""
    private var entryId: String = ""
    private var activityId: String?
    private var templateRootId: String?
    private var attrSub: FfiQuerySubscription?
    private var joinSub: FfiQuerySubscription?
    private var templateJoinSub: FfiQuerySubscription?
    private var cancellable: AnyCancellable?
    private var started = false

    func start(
        core: GainzvilleCore?,
        dataChange: DataChange,
        entryId: String,
        ownerId: String,
        activityId: String?
    ) {
        guard !started, let core else { return }
        started = true
        self.core = core
        self.entryId = entryId
        self.ownerId = ownerId
        self.activityId = activityId
        attrSub = try? core.subscribeQuery(
            query: .findAttributesByOwner(FindAttributesByOwner(ownerId: ownerId)))
        joinSub = try? core.subscribeQuery(
            query: .findEntryJoinById(FindEntryJoinById(entryId: entryId)))
        refresh()
        cancellable = dataChange.didChange.sink { [weak self] in self?.refresh() }
    }

    /// Resolve the activity's template root from the (app-wide) forest cache the
    /// first time it's available, then subscribe to its join. Done lazily in
    /// refresh so a cold-start race — sheet opened before the forest populates —
    /// recovers on a later DataChange instead of leaving the column dead.
    private func resolveTemplateRootIfNeeded() {
        guard templateRootId == nil, let core, let activityId else { return }
        guard let root = core.forestActivityTemplateRoot(activityId: activityId) else { return }
        templateRootId = root.id
        templateJoinSub = try? core.subscribeQuery(
            query: .findEntryJoinById(FindEntryJoinById(entryId: root.id)))
    }

    private func refresh() {
        guard let core else { return }
        resolveTemplateRootIfNeeded()
        // Set the attached sets before `attributes`: the `attributes` publisher
        // drives the order snapshot, which reads the attached sets to group
        // attached attributes first. Publishing it last guarantees the snapshot
        // sees up-to-date sets regardless of subscriber timing.
        if case .findEntryJoinById(let join) =
            core.readQuery(query: .findEntryJoinById(FindEntryJoinById(entryId: entryId))) {
            attachedIds = Set((join?.attributes ?? []).map { $0.id })
        }
        if let templateRootId,
           case .findEntryJoinById(let join) =
            core.readQuery(query: .findEntryJoinById(FindEntryJoinById(entryId: templateRootId))) {
            activityAttachedIds = Set((join?.attributes ?? []).map { $0.id })
        }
        if case .findAttributesByOwner(let list) =
            core.readQuery(query: .findAttributesByOwner(FindAttributesByOwner(ownerId: ownerId))) {
            attributes = list
        }
    }

    /// Toggle attachment on this entry ("This entry" column).
    func toggle(_ attributeId: String) {
        toggle(attributeId, on: entryId, attached: attachedIds.contains(attributeId))
    }

    /// Toggle attachment on the activity's template root ("All entries" column).
    /// Under materialization this affects future instances only, not existing
    /// log entries.
    func toggleActivity(_ attributeId: String) {
        guard let templateRootId else { return }
        toggle(attributeId, on: templateRootId, attached: activityAttachedIds.contains(attributeId))
    }

    private func toggle(_ attributeId: String, on targetEntryId: String, attached: Bool) {
        guard let core else { return }
        let action: Action = attached
            ? .deleteAttributeValue(DeleteAttributeValue(
                actorId: SYSTEM_ACTOR_ID, entryId: targetEntryId, attributeId: attributeId))
            : .attachValue(AttachValue(
                actorId: SYSTEM_ACTOR_ID, entryId: targetEntryId, attributeId: attributeId))
        try? core.runAction(action: action)
    }
}

struct EditAttributesView: View {
    let entry: Entry
    let entryName: String
    /// Whether the entry belongs to an activity. The "All entries" (activity
    /// default) column is gated on this; it is not rendered until Phase 3, so
    /// the flag is currently carried but unused.
    let hasActivity: Bool
    @Binding var isPresented: Bool

    @EnvironmentObject private var coreEnv: CoreEnv
    @EnvironmentObject private var dataChange: DataChange
    @StateObject private var vm = EditAttributesViewModel()

    @State private var searchText = ""
    // Display order is snapshotted once (entry-attached first, then alphabetical)
    // and frozen for the lifetime of the view so toggling never reorders rows.
    // Attributes created mid-session append to the end, never reorder existing rows.
    @State private var orderedIds: [String] = []
    @State private var hasSnapshot = false

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
        .onAppear {
            // The VM resolves the activity's template root (for the "All entries"
            // column) lazily from the forest cache; activityId is nil when the
            // entry has no activity (column hidden).
            vm.start(core: coreEnv.core, dataChange: dataChange,
                     entryId: entry.id, ownerId: entry.ownerId, activityId: entry.activityId)
        }
        .onReceive(vm.$attributes) { _ in rebuildOrderIfNeeded() }
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
        let byId = Dictionary(vm.attributes.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return orderedIds.compactMap { byId[$0] }.filter {
            searchText.isEmpty || $0.name.localizedCaseInsensitiveContains(searchText)
        }
    }

    /// Build the frozen order once the attribute list first arrives; afterward
    /// only append newly-created attributes at the end.
    private func rebuildOrderIfNeeded() {
        let all = vm.attributes
        guard !all.isEmpty else { return }
        let byName: (Attribute, Attribute) -> Bool = {
            $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
        }
        if !hasSnapshot {
            // Group order (frozen at open): attributes on this entry first, then
            // attributes on the activity (template), then the rest — each group
            // alphabetical.
            let onEntry = all.filter { vm.attachedIds.contains($0.id) }.sorted(by: byName)
            let onActivity = all
                .filter { !vm.attachedIds.contains($0.id) && vm.activityAttachedIds.contains($0.id) }
                .sorted(by: byName)
            let rest = all
                .filter { !vm.attachedIds.contains($0.id) && !vm.activityAttachedIds.contains($0.id) }
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
                GvCheckbox(checked: vm.attachedIds.contains(attr.id)) {
                    vm.toggle(attr.id)
                }
                .frame(width: 44)
                if hasActivity {
                    GvCheckbox(checked: vm.activityAttachedIds.contains(attr.id)) {
                        vm.toggleActivity(attr.id)
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
