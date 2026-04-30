import SwiftUI

// Editor for select attributes (Outcome, YDS Grade, etc.). The pill shows the
// current selection (or "min – max" for range values, read-only); tapping
// presents the option list as a sheet on iOS / popover on macOS. Picking an
// option commits `Exact(option)` immediately — no debounce.
struct SelectAttribute: View {
    let entry: FfiEntry
    let pair: FfiSelectAttributePair
    @EnvironmentObject private var forestVM: ForestViewModel
    @State private var isPresenting = false

    var body: some View {
        AttributeRow(label: pair.name) {
            Button { isPresenting = true } label: {
                Text(displayText.isEmpty ? gvEmptyPillText : displayText)
                    .frame(minWidth: GvSpacing.minAttributeInputWidth)
                    .gvAttributePill()
            }
            .buttonStyle(.plain)
            .platformPopover(isPresented: $isPresenting) {
                SelectOptionsList(
                    title: pair.name,
                    options: pair.config.options,
                    selection: currentSelection,
                    onPick: { picked in
                        commit(picked)
                        isPresenting = false
                    }
                )
            }
        }
    }

    private var displayText: String {
        switch pair.actual {
        case .none: return ""
        case .exact(let v): return v
        case .range(let lo, let hi): return "\(lo) – \(hi)"
        }
    }

    /// The currently-selected option, used to highlight a row in the picker.
    /// Range values have no single selection — they collapse to Exact on first pick.
    private var currentSelection: String? {
        if case .exact(let v) = pair.actual { return v }
        return nil
    }

    private func commit(_ option: String) {
        if case .exact(let cur) = pair.actual, cur == option { return }
        forestVM.updateAttributeValue(
            entryId: entry.id,
            attributeId: pair.attrId,
            field: .actual,
            value: .select(.exact(value: option))
        )
    }
}

// MARK: - Options list

private struct SelectOptionsList: View {
    let title: String
    let options: [String]
    let selection: String?
    let onPick: (String) -> Void

    var body: some View {
        #if os(iOS)
        NavigationStack {
            list
                .navigationTitle(title)
                .navigationBarTitleDisplayMode(.inline)
        }
        .presentationDetents([.medium, .large])
        #else
        list.padding(GvSpacing.md).frame(minWidth: 220)
        #endif
    }

    private var list: some View {
        ScrollView {
            VStack(spacing: 0) {
                ForEach(options, id: \.self) { option in
                    SelectOptionRow(
                        option: option,
                        isSelected: option == selection,
                        action: { onPick(option) }
                    )
                }
            }
        }
    }
}

private struct SelectOptionRow: View {
    let option: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack {
                Spacer()
                Text(option)
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextPrimary)
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
