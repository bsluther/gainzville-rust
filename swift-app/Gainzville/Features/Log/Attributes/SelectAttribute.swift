import SwiftUI

// Editor for select attributes (Outcome, YDS Grade, etc.). The pill shows the
// current selection; tapping presents the option list as a sheet on iOS /
// popover on macOS. Picking an option commits immediately — no debounce.
//
// Range editing (ordered selects only): the pill becomes two triggers (min –
// max) sharing one presentation, with an endpoint switcher at the top of the
// list showing which endpoint a pick sets. A pick commits as soon as both
// endpoints are known (picks that invert the option order swap at commit);
// until then nothing is written, and dismissing abandons the entry.
struct SelectAttribute: View {
    let entry: Entry
    let pair: SelectAttributePair
    @EnvironmentObject private var forestVM: ForestViewModel

    // Which endpoint the presented option list edits; nil = not presented.
    // Exact mode uses .min as "the" picker.
    @State private var presentedEndpoint: RangeEndpoint?
    // Range/exact presentation override while the stored value disagrees
    // (nil = follow stored), plus endpoints picked before the first complete
    // commit. Cleared when the presentation dismisses (the select editor's
    // session boundary) or, outside a session, when the stored value changes.
    @State private var modeOverride: Bool?
    @State private var pendingMin: String?
    @State private var pendingMax: String?

    private var storedIsRange: Bool {
        if case .range = pair.actual { return true }
        return false
    }

    private var isRangeMode: Bool { modeOverride ?? storedIsRange }

    /// Effective endpoint values: pending picks win, then the stored value.
    /// Reading min from an exact value is what prefills it on entering range
    /// mode (and doubles as the exact-mode selection).
    private var currentMin: String? {
        if let pendingMin { return pendingMin }
        switch pair.actual {
        case .range(let lo, _): return lo
        case .exact(let v): return v
        case nil: return nil
        }
    }

    private var currentMax: String? {
        if let pendingMax { return pendingMax }
        if case .range(_, let hi) = pair.actual { return hi }
        return nil
    }

    var body: some View {
        AttributeRow(label: pair.name) {
            RangePill(isRange: isRangeMode) {
                trigger(.min, text: currentMin)
            } maxInput: {
                trigger(.max, text: currentMax)
            }
            .platformPopover(isPresented: isPresenting) {
                optionsList
            }
        }
        .onChange(of: pair.actual) { _, _ in
            // Outside a session, the stored value drives everything.
            guard presentedEndpoint == nil else { return }
            modeOverride = nil
            pendingMin = nil
            pendingMax = nil
        }
        .onChange(of: presentedEndpoint) { _, presented in
            if presented == nil {
                // Dismissal ends the session: abandon any incomplete range entry.
                modeOverride = nil
                pendingMin = nil
                pendingMax = nil
            }
        }
    }

    private var isPresenting: Binding<Bool> {
        Binding(
            get: { presentedEndpoint != nil },
            set: { if !$0 { presentedEndpoint = nil } }
        )
    }

    private var optionsList: some View {
        SelectOptionsList(
            title: pair.name,
            options: pair.config.options,
            selection: presentedEndpoint == .max ? currentMax : currentMin,
            endpoint: isRangeMode ? $presentedEndpoint : nil,
            onPick: pick,
            actions: sheetActions
        )
    }

    private var sheetActions: [AttributeBarAction] {
        var range: (active: Bool, toggle: () -> Void)?
        if pair.config.ordered {
            range = (active: isRangeMode, toggle: toggleRange)
        }
        return .select(
            // Clear only when there's a value to clear, matching temporal.
            clear: pair.actual == nil ? nil : {
                forestVM.clearAttributeValue(
                    entryId: entry.id, attributeId: pair.attrId, field: .actual)
                presentedEndpoint = nil
            },
            range: range,
            remove: {
                forestVM.removeAttribute(entryId: entry.id, attributeId: pair.attrId)
                presentedEndpoint = nil
            }
        )
    }

    private func trigger(_ endpoint: RangeEndpoint, text: String?) -> some View {
        Button { presentedEndpoint = endpoint } label: {
            // Exact mode keeps the fixed-width empty placeholder; an unset
            // range endpoint renders as bare empty space per the sketch.
            Text(text ?? (isRangeMode ? "" : gvEmptyPillText))
                .frame(minWidth: GvSpacing.minAttributeInputWidth)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: - Range toggle

    private func toggleRange() {
        if isRangeMode {
            // Collapse to min (the live value when set; nothing to write when
            // the range was never committed).
            let collapse = currentMin
            modeOverride = false
            pendingMin = nil
            pendingMax = nil
            if storedIsRange, let v = collapse {
                commitExact(v)
            }
            presentedEndpoint = .min
        } else {
            modeOverride = true
            // Min prefills from the exact value, so the next pick is the max;
            // with no value at all, start at min.
            presentedEndpoint = currentMin != nil ? .max : .min
        }
    }

    // MARK: - Commit

    private func pick(_ option: String) {
        guard let endpoint = presentedEndpoint else { return }
        guard isRangeMode else {
            commitExact(option)
            presentedEndpoint = nil
            return
        }
        switch endpoint {
        case .min: pendingMin = option
        case .max: pendingMax = option
        }
        guard let lo = endpoint == .min ? option : currentMin,
              let hi = endpoint == .max ? option : currentMax else {
            // Incomplete: switch to the unset endpoint and keep picking.
            presentedEndpoint = endpoint == .min ? .max : .min
            return
        }
        commitRange(lo, hi)
        presentedEndpoint = nil
    }

    private func commitExact(_ option: String) {
        if case .exact(let cur) = pair.actual, cur == option { return }
        forestVM.updateAttributeValue(
            entryId: entry.id,
            attributeId: pair.attrId,
            field: .actual,
            value: .select(.exact(option))
        )
    }

    /// Commit a range, swapping endpoints that invert the option order (core
    /// rejects min > max; swapping preserves the user's intent where ignoring
    /// the pick would read as broken).
    private func commitRange(_ a: String, _ b: String) {
        let index = { (s: String) in pair.config.options.firstIndex(of: s) ?? 0 }
        let (lo, hi) = index(a) <= index(b) ? (a, b) : (b, a)
        if case .range(let curLo, let curHi) = pair.actual, curLo == lo, curHi == hi { return }
        forestVM.updateAttributeValue(
            entryId: entry.id,
            attributeId: pair.attrId,
            field: .actual,
            value: .select(.range(min: lo, max: hi))
        )
    }
}

// MARK: - Options list

private struct SelectOptionsList: View {
    let title: String
    let options: [String]
    let selection: String?
    /// Non-nil in range mode: the active endpoint, switchable from the list.
    let endpoint: Binding<RangeEndpoint?>?
    let onPick: (String) -> Void
    let actions: [AttributeBarAction]
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        #if os(iOS)
        VStack(spacing: 0) {
            sheetBar
            endpointSwitcher
            list
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
        .presentationDetents([.medium, .large])
        .presentationContentInteraction(.scrolls)
        #else
        VStack(spacing: 0) {
            sheetBar
            endpointSwitcher
            list
        }
        .frame(minWidth: 220)
        #endif
    }

    private var sheetBar: some View {
        AttributeSheetBar(
            title: title,
            actions: actions,
            onDismiss: { dismiss() }
        )
    }

    @ViewBuilder
    private var endpointSwitcher: some View {
        if let endpoint {
            HStack(spacing: GvSpacing.xxl) {
                endpointButton("Min", .min, endpoint)
                endpointButton("Max", .max, endpoint)
            }
            .padding(GvSpacing.md)
        }
    }

    private func endpointButton(
        _ label: String, _ value: RangeEndpoint, _ binding: Binding<RangeEndpoint?>
    ) -> some View {
        Button { binding.wrappedValue = value } label: {
            Text(label)
                .font(.gvHeadline)
                .foregroundStyle(
                    binding.wrappedValue == value ? Color.accentColor : Color.gvTextSecondary)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
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
