import SwiftUI

// Editor for numeric attributes (Reps, etc.). Mirrors TemporalAttribute's
// shadow-state + debounce pattern: edits are held in `editValue` (String, to
// allow partial input like "3."), validated and clamped at commit time, and
// dispatched as `UpdateAttributeValue` after a 1s pause or on focus loss.
struct NumericAttribute: View {
    let entry: FfiEntry
    let pair: FfiNumericAttributePair
    @EnvironmentObject private var forestVM: ForestViewModel

    @State private var editValue: String = ""
    @State private var debounceTask: Task<Void, Never>?
    @FocusState private var isKeyboardFocused: Bool
    @EnvironmentObject private var focusModel: AttributeFocusModel

    private var focus: AttributeFocus {
        .standard(entryId: entry.id, attrId: pair.attrId)
    }

    var body: some View {
        AttributeRow(label: pair.name, focus: focus) { field }
            .onAppear { syncEditState() }
            .onChange(of: pair.actual) { _, _ in
                // Skip while the user is editing — otherwise an upstream cache
                // refresh (post-commit) clobbers in-flight keystrokes.
                if !isKeyboardFocused { syncEditState() }
            }
            .onChange(of: editValue) { _, _ in scheduleDebounce() }
            .onChange(of: isKeyboardFocused) { _, focused in
                if focused { focusModel.focused = focus }
                if !focused { flushNow() }
            }
    }

    @ViewBuilder
    private var field: some View {
        let placeholder = rangePlaceholder()
        TextField(placeholder, text: $editValue)
            .textFieldStyle(.plain)
            #if os(iOS)
            .keyboardType(pair.config.integer ? .numberPad : .decimalPad)
            #endif
            .multilineTextAlignment(.center)
            .focused($isKeyboardFocused)
            .frame(minWidth: GvSpacing.minAttributeInputWidth)
            .gvAttributePill()
            .fixedSize(horizontal: true, vertical: false)
            .gvSelectAllOnFocus(isFocused: isKeyboardFocused)
            .onSubmit { flushNow() }
    }

    // For range-valued attributes, show "min – max" as placeholder behind the
    // empty editor. First commit replaces the range with `Exact` (per
    // docs/attributes-design.md "Range editing").
    private func rangePlaceholder() -> String {
        if case .range(let lo, let hi) = pair.actual {
            return "\(format(lo)) – \(format(hi))"
        }
        return ""
    }

    // MARK: - Sync cache → shadow

    private func syncEditState() {
        switch pair.actual {
        case nil, .range:
            editValue = ""
        case .exact(let v):
            editValue = format(v)
        }
    }

    // MARK: - Commit shadow → cache

    private func scheduleDebounce() {
        debounceTask?.cancel()
        debounceTask = nil
        guard let new = buildValue(), !sameAsCurrent(new) else { return }
        debounceTask = Task {
            try? await Task.sleep(nanoseconds: 1_000_000_000)
            guard !Task.isCancelled else { return }
            await MainActor.run { flushNow() }
        }
    }

    private func flushNow() {
        debounceTask?.cancel()
        debounceTask = nil
        guard let new = buildValue(), !sameAsCurrent(new) else { return }
        forestVM.updateAttributeValue(
            entryId: entry.id,
            attributeId: pair.attrId,
            field: .actual,
            value: .numeric(.exact(value: new))
        )
    }

    /// Build a clamped, optionally-rounded value from the shadow string.
    /// - Empty input → 0 (per docs/attributes-design.md "Clear-value semantics" deferral).
    /// - Non-parseable input → nil (skip commit while user is mid-typing, e.g. "3.").
    private func buildValue() -> Double? {
        let trimmed = editValue.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { return 0 }
        guard let parsed = Self.formatter.number(from: trimmed)?.doubleValue else {
            return nil
        }
        var v = parsed
        if let lo = pair.config.min { v = Swift.max(v, lo) }
        if let hi = pair.config.max { v = Swift.min(v, hi) }
        if pair.config.integer { v = v.rounded() }
        return v
    }

    private func sameAsCurrent(_ new: Double) -> Bool {
        if case .exact(let cur) = pair.actual { return cur == new }
        return false
    }

    // MARK: - Formatting

    private static let formatter: NumberFormatter = {
        let f = NumberFormatter()
        f.numberStyle = .decimal
        f.maximumFractionDigits = 6
        f.usesGroupingSeparator = false
        return f
    }()

    private func format(_ v: Double) -> String {
        if pair.config.integer {
            return String(Int(v.rounded()))
        }
        return Self.formatter.string(from: v as NSNumber) ?? String(v)
    }
}
