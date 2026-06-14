import SwiftUI
import UniformTypeIdentifiers

// Distinguishes the log from activity-template editing. EntryView and its
// subviews render identically except at a few chrome points (temporal editor,
// completion checkbox, day-root drop). The context lives in the Environment so
// it propagates automatically through the recursive ChildrenSection — template
// children inherit `.template` without threading a parameter through every init.
enum EntryContext {
    case log
    case template

    var isTemplate: Bool { self == .template }
}

private struct EntryContextKey: EnvironmentKey {
    static let defaultValue: EntryContext = .log
}

extension EnvironmentValues {
    var entryContext: EntryContext {
        get { self[EntryContextKey.self] }
        set { self[EntryContextKey.self] = newValue }
    }
}

// We take the full Entry rather than just an id even though `vm.entryJoin`
// will provide its own copy of the entry. The forest gives us the entry as
// the unit of iteration, and rendering needs structural/temporal fields
// (position, temporal, isSequence, etc.) immediately on first render —
// before the per-entry subscription has populated the cache. Passing the
// full struct avoids a flash of empty layout while the EntryJoin loads,
// and EntryJoin then supplies the joined data (activity, attributes) that
// the forest doesn't carry.
struct EntryView: View {
    let entry: Entry
    // Only root EntryViews receive this from LogView; child EntryViews omit it
    // so EntryDropDelegate falls into its forbidding mode and blocks the
    // day-root drop indicator from activating over their bodies.
    var onDayRootDrop: ((Entry) -> Void)? = nil
    @EnvironmentObject var forestVM: ForestViewModel
    @EnvironmentObject var coreEnv: CoreEnv
    @EnvironmentObject var dataChange: DataChange
    @EnvironmentObject var dragState: DragState
    @StateObject private var vm = EntryViewModel()
    // For sets cards: joined data of the selected member, whose identity and
    // attributes are what the card displays (the sequence level is hidden).
    @StateObject private var selectedMemberVM = EntryViewModel()
    @State private var isExpanded = false
    // nil tracks the last member, so a "+" duplicate (appended at the end)
    // becomes selected automatically when the forest refreshes.
    @State private var selectedMemberId: String?

    private var setsMembers: [Entry] {
        entry.displayAsSets ? forestVM.children(of: entry.id) : []
    }

    private var selectedMember: Entry? {
        setsMembers.first { $0.id == selectedMemberId } ?? setsMembers.last
    }

    // Sets cards style as their members: a set of scalars reads as one scalar
    // entry (filled, subtle border), a set of sequences keeps the sequence
    // look. Display only — drag/drop keys off the real model fields.
    private var displaysAsSequence: Bool {
        entry.displayAsSets ? setsMembers.contains { $0.isSequence } : entry.isSequence
    }

    // Single source of truth lives in core (`EntryJoin::display_name`),
    // surfaced via `EntryJoin.displayName`. Empty string covers the
    // single render frame between view appear and the subscription
    // populating the cache; SwiftUI swaps in the real value immediately.
    // Sets sequences are anonymous; their cards title as the selected member.
    var displayName: String {
        (entry.displayAsSets ? selectedMemberVM : vm).entryJoin?.displayName ?? ""
    }

    var body: some View {
        VStack(spacing: 0) {
            EntryHeader(
                entry: entry,
                attributeTarget: entry.displayAsSets ? (selectedMember ?? entry) : entry,
                displayName: displayName,
                activityName: (entry.displayAsSets ? selectedMemberVM : vm).entryJoin?.activity?.name,
                isExpanded: isExpanded,
                onToggle: { isExpanded.toggle() }
            )
            if isExpanded {
                if entry.displayAsSets {
                    SetsBody(
                        sequence: entry,
                        members: setsMembers,
                        selectedMember: selectedMember,
                        selectedMemberId: $selectedMemberId,
                        memberAttributes: selectedMemberVM.entryJoin?.attributes ?? []
                    )
                } else {
                    EntryBody(entry: entry, attributes: vm.entryJoin?.attributes ?? [])
                }
            }
        }
        // Pin to the parent's proposal. Over-eager value content can't inflate
        // the card because AttributeRow falls back to stacking pills vertically
        // (via ViewThatFits) when they don't fit horizontally. (A flexible frame
        // alone does NOT cap — it grows to fit an oversized child — which is why
        // the earlier maxWidth-only fix regressed.) Content overflow is clipped
        // by the rounded clipShape inside entryContainerStyle; we deliberately do
        // NOT add a rectangular .clipped() here — it would trim the border
        // overlay's centered stroke on the straight edges but not at the corners
        // (where the path curves inward), making the corners look ~2x too thick.
        .frame(maxWidth: .infinity, alignment: .leading)
        .entryContainerStyle(isSequence: displaysAsSequence)
        // Drop delegate: forwards to day-root for root scalars; forbids drops
        // (blocking the day-root indicator) for everything else. See
        // EntryDragDrop.swift for the full hit-test layering rationale.
        .onDrop(of: [UTType.plainText], delegate: EntryDropDelegate(
            entry: entry,
            dragState: dragState,
            onDayRootDrop: onDayRootDrop
        ))
        .onAppear {
            // Pre-marked by actions that swap this card's identity (e.g.
            // ConvertToSets replacing an entry with its new sets sequence).
            if forestVM.consumePendingExpanded(entry.id) {
                isExpanded = true
            }
            vm.start(core: coreEnv.core, dataChange: dataChange, entryId: entry.id)
        }
        // Runs on appear and whenever the selection (or membership) changes;
        // start is idempotent per id and re-targets across ids.
        .task(id: selectedMember?.id) {
            if let memberId = selectedMember?.id {
                selectedMemberVM.start(core: coreEnv.core, dataChange: dataChange, entryId: memberId)
            }
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
    let entry: Entry
    // The entry whose attributes the card displays and the menu edits — the
    // selected set member on sets cards, the entry itself otherwise.
    let attributeTarget: Entry
    let displayName: String
    let activityName: String?
    let isExpanded: Bool
    let onToggle: () -> Void
    @State private var isMenuPresented = false
    @EnvironmentObject private var forestVM: ForestViewModel
    @EnvironmentObject private var dragState: DragState
    @Environment(\.entryContext) private var entryContext

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

            // Templates never show the completion checkbox (they can't be
            // completed); show the menu affordance instead.
            if entry.isSequence || isExpanded || entryContext.isTemplate {
                Button { isMenuPresented = true } label: {
                    Image(systemName: "ellipsis")
                        .rotationEffect(.degrees(90))
                        .foregroundStyle(Color.gvTextSecondary)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .platformPopover(isPresented: $isMenuPresented) {
                    EntryMenuContent(entry: entry, attributeTarget: attributeTarget, entryName: displayName, activityName: activityName, isPresented: $isMenuPresented)
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
    let entry: Entry
    let attributes: [AttributePair]
    @Environment(\.entryContext) private var entryContext

    // The footer supplies the bottom padding: the +Entry button (sequences) or
    // the completion checkbox (log scalars). A template scalar shows neither, so
    // pad explicitly here — only in that case, to avoid stacking padding.
    private var needsBottomPadding: Bool {
        entryContext.isTemplate && !entry.isSequence
    }

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.entrySpacing) {
            // Templates have no timeline — a duration is all they carry — so they
            // show a flat Duration row in place of the collapsible Time editor.
            if entryContext.isTemplate {
                DurationAttribute(entry: entry)
            } else {
                TemporalAttribute(entry: entry)
            }
            AttributesSection(entry: entry, attributes: attributes)
            if entry.isSequence {
                ChildrenSection(parent: entry)
            }
            EntryFooter(entry: entry)
        }
        .padding(.horizontal, GvSpacing.entrySpacing)
        .padding(.top, GvSpacing.entrySpacing)
        .padding(.bottom, needsBottomPadding ? GvSpacing.entrySpacing : 0)
    }
}

// MARK: - Children

private struct ChildrenSection: View {
    let parent: Entry
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
        let position: Position?
        let predId: String?
        let succId: String?
        let child: Entry?

        static func dropTarget(position: Position, predId: String?, succId: String?) -> Slot {
            Slot(id: "drop-\(predId ?? "start")-\(succId ?? "end")", position: position, predId: predId, succId: succId, child: nil)
        }

        static func childSlot(_ entry: Entry) -> Slot {
            Slot(id: "child-\(entry.id)", position: nil, predId: nil, succId: nil, child: entry)
        }
    }

    private func buildSlots(parentId: String, children: [Entry]) -> [Slot] {
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

// MARK: - Sets

// Body for a display_as_sets sequence. The sequence level is hidden: the card
// shows the Sets picker, the sequence's one Time row (the sequence owns the
// timeline slot; member temporal stays hidden), then the SELECTED member's
// attributes, children, and footer. Set numbers are the members' sibling order.
private struct SetsBody: View {
    let sequence: Entry
    let members: [Entry]
    let selectedMember: Entry?
    @Binding var selectedMemberId: String?
    let memberAttributes: [AttributePair]
    @Environment(\.entryContext) private var entryContext

    // Matches the card border, which is keyed off the members the same way
    // (see EntryView.displaysAsSequence).
    private var separatorColor: Color {
        members.contains { $0.isSequence } ? .entrySequenceBorder : .entryScalarBorder
    }

    // The selected member's footer supplies the bottom padding (its +Entry
    // button or completion checkbox). A template scalar member shows neither,
    // so pad explicitly in that case — mirrors EntryBody.needsBottomPadding.
    private var needsBottomPadding: Bool {
        entryContext.isTemplate && !(selectedMember?.isSequence ?? false)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.entrySpacing) {
            // Time first: it describes the whole sequence of members, so it sits
            // hierarchically above the per-set picker. Only logs have a timeline;
            // a template sequence carries no time of its own (its durations live
            // per-member, below), so it shows no sequence-level temporal row.
            if !entryContext.isTemplate {
                TemporalAttribute(entry: sequence)
            }
            SetsControl(members: members, selectedMemberId: $selectedMemberId)
            // Separates the sequence's sets control row from the per-set rows below.
            Rectangle()
                .fill(separatorColor)
                .frame(height: GvSpacing.entryScalarBorderWidth)
                .padding(.vertical, GvSpacing.md)
            if let member = selectedMember {
                // Re-key on the member so editor state (focus, in-progress
                // edits) never carries across set switches.
                Group {
                    // Per-set duration: the sequence owns start/end (its Time row
                    // above); each member owns only its duration — the hang, plank,
                    // or sprint time. Duration-only, so it can't over-determine.
                    DurationAttribute(entry: member)
                    AttributesSection(entry: member, attributes: memberAttributes)
                    if member.isSequence {
                        ChildrenSection(parent: member)
                    }
                    EntryFooter(entry: member)
                }
                .id(member.id)
            }
        }
        .padding(.horizontal, GvSpacing.entrySpacing)
        .padding(.top, GvSpacing.entrySpacing)
        .padding(.bottom, needsBottomPadding ? GvSpacing.entrySpacing : 0)
    }
}

// The attribute-like "Sets" row: numbered pills in sibling order, "+"
// duplicates the LAST member and appends (selection follows the new set),
// "−" deletes the SELECTED member and selects its predecessor. "−" disables
// at one member; core also rejects removing a sets sequence's last member.
private struct SetsControl: View {
    let members: [Entry]
    @Binding var selectedMemberId: String?
    @EnvironmentObject private var forestVM: ForestViewModel

    private var selectedIndex: Int {
        members.firstIndex { $0.id == selectedMemberId } ?? members.count - 1
    }

    var body: some View {
        AttributeRow(label: "Sets") {
            HStack(spacing: GvSpacing.lg) {
                ForEach(Array(members.enumerated()), id: \.element.id) { index, member in
                    Button {
                        selectedMemberId = member.id
                    } label: {
                        Text("\(index + 1)")
                            .font(.attrField)
                            .foregroundStyle(index == selectedIndex ? Color.entryTextPrimary : Color.entryTextSecondary)
                            .frame(minWidth: 28, minHeight: 28)
                            .background(
                                RoundedRectangle(cornerRadius: 8)
                                    .fill(index == selectedIndex ? Color.gvNeutral800 : .clear)
                            )
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                Button(action: addSet) {
                    Image(systemName: "plus")
                        .foregroundStyle(Color.entryTextPrimary)
                        .frame(width: 28, height: 28)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                Button(action: removeSelected) {
                    Image(systemName: "minus")
                        .foregroundStyle(members.count > 1 ? Color.entryTextPrimary : Color.entryTextSecondary)
                        .frame(width: 28, height: 28)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(members.count <= 1)
            }
        }
    }

    private func addSet() {
        guard let last = members.last else { return }
        forestVM.duplicateEntry(entry: last)
        // nil tracks the last member, which is the new copy after refresh.
        selectedMemberId = nil
    }

    private func removeSelected() {
        guard members.count > 1 else { return }
        let index = selectedIndex
        selectedMemberId = index > 0 ? members[index - 1].id : members[1].id
        forestVM.deleteEntry(entry: members[index])
    }
}

// MARK: - Footer

private struct EntryFooter: View {
    let entry: Entry
    @EnvironmentObject private var forestVM: ForestViewModel
    @Environment(\.entryContext) private var entryContext
    @State private var isCreatePresented = false

    var body: some View {
        if entry.isSequence {
            HStack {
                Spacer()
                Button { isCreatePresented = true } label: {
                    Image(systemName: "plus")
                        .font(.attrField)
                        .fontWeight(.semibold)
                        .foregroundStyle(Color.gvNeutral350)
                        .padding(GvSpacing.md)
                        .background(
                            RoundedRectangle(cornerRadius: GvSpacing.entryScalarCornerRadius)
                                .fill(Color.entryScalarBackground)
                        )
                        .overlay(
                            RoundedRectangle(cornerRadius: GvSpacing.entryScalarCornerRadius)
                                .stroke(Color.entryScalarBorder, lineWidth: GvSpacing.entryScalarBorderWidth)
                        )
                        .contentShape(RoundedRectangle(cornerRadius: GvSpacing.entryScalarCornerRadius))
                }
                .buttonStyle(.plain)
            }
            .padding(.bottom, GvSpacing.entrySpacing)
            .sheet(isPresented: $isCreatePresented) {
                CreateEntrySheet(isPresented: $isCreatePresented) { activityId, name, isSeq in
                    forestVM.createChildEntry(in: entry, activityId: activityId, name: name, isSequence: isSeq)
                    isCreatePresented = false
                }
            }
        } else if !entryContext.isTemplate {
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
    let entry: Entry
    // Attribute-scoped items (Edit attributes, View activity) target this
    // entry — the selected set member on sets cards, where the card shows the
    // member's attributes and the sequence is anonymous.
    let attributeTarget: Entry
    let entryName: String
    let activityName: String?
    @Binding var isPresented: Bool
    @EnvironmentObject private var forestVM: ForestViewModel

    var body: some View {
        let isRoot = entry.position == nil
        // Core rejects duplicating or converting an activity template root
        // (both would break the unique-template-root rule) — don't offer them.
        let isTemplateRoot = entry.isTemplate && isRoot
        NavigationStack {
            ScrollView {
                VStack(spacing: GvSpacing.md) {
                    // Group 1 — workflow
                    if !isTemplateRoot {
                        GvMenuRow("Duplicate", icon: "doc.on.doc") {
                            forestVM.duplicateEntry(entry: entry)
                        }
                        if entry.displayAsSets {
                            GvMenuRow("Break out", icon: "rectangle.stack") {
                                forestVM.setDisplayAsSets(entryId: entry.id, displayAsSets: false)
                            }
                        } else {
                            GvMenuRow("With sets", icon: "rectangle.stack.badge.plus") {
                                forestVM.convertToSets(entry: entry)
                            }
                        }
                    }
                    if entry.isSequence {
                        GvMenuRow("Add entry", icon: "plus.circle")
                    }

                    GvMenuDivider()

                    // Group 2 — attributes
                    NavigationLink {
                        EditAttributesView(entry: attributeTarget, entryName: entryName, hasActivity: attributeTarget.activityId != nil, isPresented: $isPresented)
                    } label: {
                        HStack(spacing: GvSpacing.lg) {
                            Image(systemName: "slider.horizontal.3").frame(width: 20)
                            Text("Edit attributes").font(.gvBody)
                            Spacer()
                            Image(systemName: "chevron.right")
                                .font(.caption)
                                .foregroundStyle(Color.gvTextSecondary)
                        }
                        .foregroundStyle(Color.gvTextPrimary)
                        .padding(.horizontal, GvSpacing.lg)
                        .padding(.vertical, GvSpacing.lg)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)

                    // Group 3 — conditional navigation
                    if attributeTarget.activityId != nil || !isRoot {
                        GvMenuDivider()
                        if attributeTarget.activityId != nil {
                            GvMenuRow("View activity", icon: "figure.run")
                        }
                        if !isRoot {
                            GvMenuRow("Move to time", icon: "clock")
                        }
                    }

                    GvMenuDivider()

                    // Group 4 — destructive
                    if entry.isSequence {
                        GvMenuRow("Delete recursive", icon: "trash.fill", isDestructive: true) {
                            forestVM.deleteEntry(entry: entry)
                        }
                        GvMenuRow("Delete unbox", icon: "arrow.up.backward.and.arrow.down.forward", isDestructive: true)
                    } else {
                        GvMenuRow("Delete", icon: "trash", isDestructive: true) {
                            forestVM.deleteEntry(entry: entry)
                        }
                    }
                }
                .padding(GvSpacing.md)
            }
            #if os(iOS)
            .toolbar(.hidden, for: .navigationBar)
            #endif
        }
        #if os(iOS)
        .presentationDetents([.medium, .large])
        // Pin the corner radius so the manual border below matches the sheet
        // edge, and supply the sheet's fill + border via presentationBackground.
        // A plain dark fill (content .background or presentationBackground(Color))
        // hides the native lighter rim, so we draw it ourselves with strokeBorder
        // (inset fully inside, like the entry-card border). Applied at the sheet
        // root so it's consistent across this menu and the pushed EditAttributesView.
        .presentationCornerRadius(36)
        .presentationBackground {
            Color.gvBackground
                .overlay(
                    RoundedRectangle(cornerRadius: 36, style: .continuous)
                        .strokeBorder(.white.opacity(0.12), lineWidth: 0.5)
                )
        }
        #endif
    }
}

// MARK: - Attributes

private struct AttributesSection: View {
    let entry: Entry
    let attributes: [AttributePair]

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
                    case .length(let p):  LengthAttribute(entry: entry, pair: p)
                    case .text(let p):    TextAttribute(entry: entry, pair: p)
                    }
                }
            }
        }
    }
}
