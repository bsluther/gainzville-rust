import SwiftUI

// Editor for numeric attributes (Reps, etc.). Mirrors TemporalAttribute's
// shadow-state + debounce pattern: edits are held in `editValue` (String, to
// allow partial input like "3."), validated and clamped at commit time, and
// dispatched as `UpdateAttributeValue` after a 1s pause or on focus loss.
struct NumericAttribute: View {
    let entry: Entry
    let pair: NumericAttributePair
    @EnvironmentObject private var forestVM: ForestViewModel

    @State private var editValue: String = ""
    @State private var debounceTask: Task<Void, Never>?
    @FocusState private var isKeyboardFocused: Bool
    @EnvironmentObject private var focusModel: AttributeFocusModel
    // macOS prototype (GV-36): show the action bar in a popover anchored to the
    // field while it's focused, since macOS has no keyboard accessory.
    @State private var showActions = false
    // Set by Remove so the focus-loss handler skips flushNow() — flushing
    // would dispatch an update against the just-deleted row.
    @State private var pendingRemoval = false

    // Same attribute can be attached to multiple visible entries, so the focus
    // owner token needs both ids.
    private var focusToken: String { "\(entry.id)/\(pair.attrId)" }

    // The bar actions for this attribute, defined once and rendered identically
    // by both surfaces (iOS keyboard bar via the focus model, macOS popover).
    private var barActions: [AttributeBarAction] {
        .numeric(remove: {
            pendingRemoval = true
            forestVM.removeAttribute(entryId: entry.id, attributeId: pair.attrId)
            isKeyboardFocused = false
        })
    }

    var body: some View {
        AttributeRow(label: pair.name) { field }
            .onAppear { syncEditState() }
            .onChange(of: pair.actual) { _, _ in
                // Skip while the user is editing — otherwise an upstream cache
                // refresh (post-commit) clobbers in-flight keystrokes.
                if !isKeyboardFocused { syncEditState() }
            }
            .onChange(of: editValue) { _, _ in scheduleDebounce() }
            .onChange(of: isKeyboardFocused) { _, focused in
                if focused {
                    focusModel.focus(focusToken, actions: barActions)
                } else {
                    focusModel.clear(focusToken)
                    if pendingRemoval {
                        pendingRemoval = false
                        debounceTask?.cancel()
                        debounceTask = nil
                    } else {
                        flushNow()
                    }
                }
                #if os(macOS)
                showActions = focused
                #endif
            }
            #if os(macOS)
            // The popover's lifecycle IS the editing session: when it closes for
            // any reason (clicking an inert area outside the field dismisses the
            // popover), end editing too so keyboard focus doesn't linger.
            .onChange(of: showActions) { _, shown in
                if !shown { isKeyboardFocused = false }
            }
            #endif
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
            .onSubmit { isKeyboardFocused = false }
            #if os(macOS)
            .popover(isPresented: $showActions, arrowEdge: .top) {
                AttributeSheetBar(
                    title: pair.name,
                    actions: barActions,
                    onDismiss: { isKeyboardFocused = false }
                )
                .frame(width: 280)
            }
            #endif
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
            value: .numeric(.exact(new))
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
