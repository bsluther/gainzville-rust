import SwiftUI

// We take the full FfiEntry rather than just an id even though `vm.entryJoin`
// will provide its own copy of the entry. The forest gives us the entry as
// the unit of iteration, and rendering needs structural/temporal fields
// (position, temporal, isSequence, etc.) immediately on first render —
// before the per-entry subscription has populated the cache. Passing the
// full struct avoids a flash of empty layout while the EntryJoin loads,
// and EntryJoin then supplies the joined data (activity, attributes) that
// the forest doesn't carry.
struct EntryView: View {
    let entry: FfiEntry
    @EnvironmentObject var forestVM: ForestViewModel
    @EnvironmentObject var coreEnv: CoreEnv
    @EnvironmentObject var dataChange: DataChange
    @EnvironmentObject var attributeFocus: AttributeFocusModel
    @StateObject private var vm = EntryViewModel()
    @State private var isExpanded = false

    // Single source of truth lives in core (`EntryJoin::display_name`),
    // surfaced via `FfiEntryJoin.displayName`. Empty string covers the
    // single render frame between view appear and the subscription
    // populating the cache; SwiftUI swaps in the real value immediately.
    var displayName: String {
        vm.entryJoin?.displayName ?? ""
    }

    var body: some View {
        VStack(spacing: 0) {
            EntryHeader(
                entry: entry,
                displayName: displayName,
                isExpanded: isExpanded,
                onToggle: {
                    attributeFocus.focused = nil
                    isExpanded.toggle()
                }
            )
            if isExpanded {
                EntryBody(entry: entry, attributes: vm.entryJoin?.attributes ?? [])
            }
        }
        // Pin to the parent's proposal so over-eager content (TextField
        // minWidths, long pill text, deeply nested children) gets clipped by
        // the container style rather than pushing the entry past the screen.
        .frame(maxWidth: .infinity, alignment: .leading)
        .entryContainerStyle(isSequence: entry.isSequence)
        // Tap-outside-to-clear inside an entry: the entry's colored background
        // swallows taps before they reach LogView's clear-background, so we
        // also clear at the entry level. AttributeRow's own .onTapGesture wins
        // for taps on rows (SwiftUI delivers to the deepest gesture); inner
        // Buttons (header, ellipsis, footer +Entry, checkbox) consume their
        // own taps without firing this one.
        .contentShape(Rectangle())
        .onTapGesture {
            attributeFocus.focused = nil
        }
        .onAppear {
            vm.start(core: coreEnv.core, dataChange: dataChange, entryId: entry.id)
        }
    }
}

// MARK: - Container styling

extension View {
    func entryContainerStyle(isSequence: Bool) -> some View {
        let borderWidth = isSequence ? GvSpacing.entrySequenceBorderWidth : GvSpacing.entryScalarBorderWidth
        return self
            .background(isSequence ? Color.entrySequenceBackground : Color.entryScalarBackground)
            .clipShape(RoundedRectangle(cornerRadius: GvSpacing.entryCornerRadius))
            .overlay(
                RoundedRectangle(cornerRadius: GvSpacing.entryCornerRadius)
                    .stroke(isSequence ? Color.entrySequenceBorder : Color.entryScalarBorder, lineWidth: borderWidth)
            )
    }
}

// MARK: - Header

private struct EntryHeader: View {
    let entry: FfiEntry
    let displayName: String
    let isExpanded: Bool
    let onToggle: () -> Void
    @State private var isMenuPresented = false
    @EnvironmentObject private var forestVM: ForestViewModel
    @EnvironmentObject private var dragState: DragState

    var body: some View {
        HStack(spacing: 0) {
            Button(action: onToggle) {
                Text(displayName)
                    .font(.gvBody)
                    .foregroundStyle(Color.entryTextPrimary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.vertical, GvSpacing.entrySpacing)
                    .padding(.leading, GvSpacing.entrySpacing)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            if entry.isSequence || isExpanded {
                Button { isMenuPresented = true } label: {
                    Image(systemName: "ellipsis")
                        .rotationEffect(.degrees(90))
                        .foregroundStyle(Color.gvTextSecondary)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .platformPopover(isPresented: $isMenuPresented) {
                    EntryMenuContent(entry: entry)
                }
            } else {
                FillCheckbox(checked: entry.isComplete, onToggle: {
                    forestVM.updateEntryCompletion(entry: entry, isComplete: !entry.isComplete)
                })
            }
        }
        .onDrag {
            dragState.draggedEntry = entry
            return NSItemProvider(object: entry.id as NSString)
        } preview: {
            EntryDragPreview(displayName: displayName)
        }
    }
}

// MARK: - Body (shown when expanded)

private struct EntryBody: View {
    let entry: FfiEntry
    let attributes: [FfiAttributePair]

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.entrySpacing) {
            TemporalAttribute(entry: entry)
            AttributesSection(entry: entry, attributes: attributes)
            if entry.isSequence {
                ChildrenSection(parent: entry)
            }
            EntryFooter(entry: entry)
        }
        .padding(.horizontal, GvSpacing.entrySpacing)
        .padding(.top, GvSpacing.entrySpacing)
    }
}

// MARK: - Children

private struct ChildrenSection: View {
    let parent: FfiEntry
    @EnvironmentObject private var forestVM: ForestViewModel

    var body: some View {
        let children = forestVM.children(of: parent.id)
        let slots = buildSlots(parentId: parent.id, children: children)
        if !slots.isEmpty {
            VStack(spacing: 0) {
                ForEach(slots) { slot in
                    if let position = slot.position {
                        DropTarget(position: position, predId: slot.predId, succId: slot.succId)
                    }
                    if let child = slot.child {
                        EntryView(entry: child)
                    }
                }
            }
        }
    }

    // Identity is pred/succ-based for drop targets, entry-id-based for children.
    // This prevents SwiftUI from transferring isTargeted @State across slots that
    // shift positions after a drop + forest refresh.
    private struct Slot: Identifiable {
        let id: String
        let position: FfiPosition?
        let predId: String?
        let succId: String?
        let child: FfiEntry?

        static func dropTarget(position: FfiPosition, predId: String?, succId: String?) -> Slot {
            Slot(id: "drop-\(predId ?? "start")-\(succId ?? "end")", position: position, predId: predId, succId: succId, child: nil)
        }

        static func childSlot(_ entry: FfiEntry) -> Slot {
            Slot(id: "child-\(entry.id)", position: nil, predId: nil, succId: nil, child: entry)
        }
    }

    private func buildSlots(parentId: String, children: [FfiEntry]) -> [Slot] {
        var slots: [Slot] = []
        let count = children.count
        for i in 0...count {
            let predId = i > 0 ? children[i - 1].id : nil
            let succId = i < count ? children[i].id : nil
            if let position = forestVM.positionBetween(parentId: parentId, predId: predId, succId: succId) {
                slots.append(.dropTarget(position: position, predId: predId, succId: succId))
            }
            if i < count {
                slots.append(.childSlot(children[i]))
            }
        }
        return slots
    }
}

// MARK: - Footer

private struct EntryFooter: View {
    let entry: FfiEntry
    @EnvironmentObject private var forestVM: ForestViewModel
    @State private var isCreatePresented = false

    var body: some View {
        if entry.isSequence {
            HStack {
                Button("+ Entry") { isCreatePresented = true }
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextSecondary)
                    .buttonStyle(.plain)
                    .padding(.vertical, GvSpacing.entrySpacing)
                Spacer()
            }
            .sheet(isPresented: $isCreatePresented) {
                CreateEntrySheet(isPresented: $isCreatePresented) { activityId, name, isSeq in
                    forestVM.createChildEntry(in: entry, activityId: activityId, name: name, isSequence: isSeq)
                    isCreatePresented = false
                }
            }
        } else {
            HStack {
                Spacer()
                FillCheckbox(checked: entry.isComplete, onToggle: {
                    forestVM.updateEntryCompletion(entry: entry, isComplete: !entry.isComplete)
                })
            }
            .padding(.trailing, -GvSpacing.entrySpacing)
        }
    }
}

// MARK: - Fill checkbox

private struct FillCheckbox: View {
    let checked: Bool
    var onToggle: () -> Void = {}

    var body: some View {
        Button(action: onToggle) {
            ZStack {
                RoundedRectangle(cornerRadius: 4)
                    .stroke(Color.gvLoggedBlue, lineWidth: 1.5)
                    .frame(width: 20, height: 20)
                if checked {
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Color.gvLoggedBlue)
                        .frame(width: 12, height: 12)
                }
            }
            .frame(width: 44, height: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Context menu

private struct EntryMenuContent: View {
    let entry: FfiEntry
    @EnvironmentObject private var forestVM: ForestViewModel

    var body: some View {
        let isRoot = entry.position == nil
        ScrollView {
            VStack(spacing: GvSpacing.md) {
                // Group 1 — workflow
                EntryMenuRow("Duplicate", icon: "doc.on.doc")
                EntryMenuRow("Add set", icon: "rectangle.stack.badge.plus")
                if entry.isSequence {
                    EntryMenuRow("Add entry", icon: "plus.circle")
                }

                EntryMenuDivider()

                // Group 2 — attributes
                EntryMenuRow("Add attribute", icon: "tag")
                EntryMenuRow("Edit attributes", icon: "slider.horizontal.3")

                // Group 3 — conditional navigation
                if entry.activityId != nil || !isRoot {
                    EntryMenuDivider()
                    if entry.activityId != nil {
                        EntryMenuRow("View activity", icon: "figure.run")
                    }
                    if !isRoot {
                        EntryMenuRow("Move to time", icon: "clock")
                    }
                }

                EntryMenuDivider()

                // Group 4 — destructive
                if entry.isSequence {
                    EntryMenuRow("Delete recursive", icon: "trash.fill", isDestructive: true) {
                        forestVM.deleteEntry(entry: entry)
                    }
                    EntryMenuRow("Delete unbox", icon: "arrow.up.backward.and.arrow.down.forward", isDestructive: true)
                } else {
                    EntryMenuRow("Delete", icon: "trash", isDestructive: true) {
                        forestVM.deleteEntry(entry: entry)
                    }
                }
            }
            .padding(GvSpacing.md)
        }
        #if os(iOS)
        .presentationDetents([.medium])
        #endif
    }
}

private struct EntryMenuRow: View {
    let label: String
    var icon: String? = nil
    var isDestructive: Bool = false
    var action: () -> Void = {}
    @Environment(\.dismiss) private var dismiss

    init(_ label: String, icon: String? = nil, isDestructive: Bool = false, action: @escaping () -> Void = {}) {
        self.label = label
        self.icon = icon
        self.isDestructive = isDestructive
        self.action = action
    }

    var body: some View {
        Button {
            dismiss()
            action()
        } label: {
            HStack(spacing: GvSpacing.lg) {
                if let icon {
                    Image(systemName: icon)
                        .frame(width: 20)
                }
                Text(label)
                    .font(.gvBody)
                Spacer()
            }
            .foregroundStyle(isDestructive ? Color.red : Color.gvTextPrimary)
            .padding(.horizontal, GvSpacing.lg)
            .padding(.vertical, GvSpacing.lg)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

private struct EntryMenuDivider: View {
    var body: some View {
        Rectangle()
            .fill(Color.gvNeutral800)
            .frame(height: 0.5)
    }
}

// MARK: - Attributes

private struct AttributesSection: View {
    let entry: FfiEntry
    let attributes: [FfiAttributePair]

    var body: some View {
        // ASCII name sort is a placeholder; see docs/attributes-design.md
        // "Per-entry attribute order" for the long-term plan.
        let sorted = attributes.sorted { $0.name < $1.name }
        if !sorted.isEmpty {
            VStack(alignment: .leading, spacing: GvSpacing.lg) {
                ForEach(sorted) { pair in
                    switch pair {
                    case .numeric(let p): NumericAttribute(entry: entry, pair: p)
                    case .select(let p):  SelectAttribute(entry: entry, pair: p)
                    case .mass(let p):    MassAttribute(entry: entry, pair: p)
                    }
                }
            }
        }
    }
}
