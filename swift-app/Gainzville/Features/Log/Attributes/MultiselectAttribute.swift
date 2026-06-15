import SwiftUI

// Editor for multiselect attributes (Climb Tag, etc.). The pill stacks the
// selected option strings vertically with a top-right caret; tapping presents
// the full option list as a sheet on iOS / popover on macOS. Each tap toggles
// one option and commits immediately — no debounce, no pending state (the
// stored value drives the UI, like SelectAttribute).
//
// A multiselect value is an unordered set, so there is no exact/range axis and
// no RangePill. Display and storage order both follow the config's option
// order (a display affordance, not data): the pill and every committed value
// are the config options filtered to the selected ones. Deselecting the last
// option clears the value (stored as nothing, not an empty set).
struct MultiselectAttribute: View {
    let entry: Entry
    let pair: MultiselectAttributePair
    @EnvironmentObject private var forestVM: ForestViewModel

    @State private var isPresented = false

    // Measured height of one value line. Seeded so the initial padding is the
    // base `sm` (no first-frame jump) until the real line height arrives.
    @State private var lineHeight: CGFloat = GvSpacing.minAttributeHeight - 2 * GvSpacing.sm

    // Match the select pill's effective per-line breathing room. A select pill
    // centers a single line in `minAttributeHeight`, so its visible top/bottom
    // padding is `(minAttributeHeight - lineHeight) / 2`. Applying that as real
    // padding makes a multi-line multiselect pill just as airy and matches
    // select exactly on each platform (whose font line heights differ — e.g.
    // macOS lines are shorter, so select reserves more space there). Floored at
    // the base `sm`.
    private var rowVerticalPadding: CGFloat {
        max(GvSpacing.sm, (GvSpacing.minAttributeHeight - lineHeight) / 2)
    }

    /// The selected options in config order — the source of truth for both the
    /// pill and the editor checkmarks. Intersecting with `config.options` keeps
    /// the display canonical regardless of the stored vec's order.
    private var selected: [String] {
        let set = selectedSet
        return pair.config.options.filter { set.contains($0) }
    }

    private var selectedSet: Set<String> { Set(pair.actual ?? []) }

    var body: some View {
        AttributeRow(label: pair.name) {
            Button { isPresented = true } label: {
                pillContent
                    // Padding derived from the line height so the per-line
                    // breathing room matches a select pill on every platform.
                    .gvAttributePill(verticalPadding: rowVerticalPadding)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .platformPopover(isPresented: $isPresented) {
                optionsList
            }
        }
    }

    // The selected options stacked vertically, left-aligned. No chevron — like
    // Select, the whole pill is tappable. The min-width floor matches the other
    // value pills so an empty pill is the same width as an empty Select.
    private var pillContent: some View {
        VStack(alignment: .leading, spacing: GvSpacing.sm) {
            if selected.isEmpty {
                Text(gvEmptyPillText)
            } else {
                ForEach(selected, id: \.self) { Text($0) }
            }
        }
        .frame(minWidth: GvSpacing.minAttributeInputWidth, alignment: .leading)
        .background(lineHeightReader)
    }

    // A hidden one-line reference (same font as the rows) whose measured height
    // feeds `rowVerticalPadding`. Doesn't affect layout.
    private var lineHeightReader: some View {
        Text(verbatim: "Xg")
            .font(.attrField)
            .fixedSize()
            .hidden()
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { lineHeight = $0 }
    }

    private var optionsList: some View {
        MultiselectOptionsList(
            title: pair.name,
            options: pair.config.options,
            selected: selectedSet,
            onToggle: toggle,
            actions: sheetActions
        )
    }

    private var sheetActions: [AttributeBarAction] {
        .multiselect(
            // Clear only when there's a selection to clear, matching select.
            clear: selected.isEmpty ? nil : {
                forestVM.clearAttributeValue(
                    entryId: entry.id, attributeId: pair.attrId, field: .actual)
                isPresented = false
            },
            remove: {
                forestVM.removeAttribute(entryId: entry.id, attributeId: pair.attrId)
                isPresented = false
            }
        )
    }

    /// Toggle one option's membership and commit the whole set. The committed
    /// value is the config options filtered to the selection, so it is always
    /// config-ordered and duplicate-free. An empty selection clears the value.
    private func toggle(_ option: String) {
        var set = selectedSet
        if set.contains(option) {
            set.remove(option)
        } else {
            set.insert(option)
        }
        let ordered = pair.config.options.filter { set.contains($0) }
        if ordered.isEmpty {
            forestVM.clearAttributeValue(
                entryId: entry.id, attributeId: pair.attrId, field: .actual)
        } else {
            forestVM.updateAttributeValue(
                entryId: entry.id,
                attributeId: pair.attrId,
                field: .actual,
                value: .multiselect(ordered)
            )
        }
    }
}

// MARK: - Options list

// Mirrors SelectOptionsList, but every option can be checked and a tap toggles
// membership (no single-selection, no range endpoint switcher).
private struct MultiselectOptionsList: View {
    let title: String
    let options: [String]
    let selected: Set<String>
    let onToggle: (String) -> Void
    let actions: [AttributeBarAction]
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        #if os(iOS)
        content
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
            .presentationDetents([.medium, .large])
            .presentationContentInteraction(.scrolls)
        #else
        content
            .frame(minWidth: 220)
        #endif
    }

    private var content: some View {
        VStack(spacing: 0) {
            AttributeSheetBar(title: title, actions: actions, onDismiss: { dismiss() })
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(options, id: \.self) { option in
                        MultiselectOptionRow(
                            option: option,
                            isSelected: selected.contains(option),
                            action: { onToggle(option) }
                        )
                    }
                }
            }
        }
    }
}

private struct MultiselectOptionRow: View {
    let option: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack {
                Spacer()
                Text(option)
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextBright)
                Spacer()
            }
            .overlay(alignment: .trailing) {
                if isSelected {
                    Image(systemName: "checkmark")
                        .foregroundStyle(Color.gvLoggedBlue)
                }
            }
            .padding(.horizontal, GvSpacing.lg)
            .padding(.vertical, GvSpacing.lg)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
