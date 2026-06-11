import SwiftUI
#if os(macOS)
import AppKit
#endif

// Editor for numeric attributes (Reps, etc.). Mirrors TemporalAttribute's
// shadow-state + debounce pattern: edits are held in shadow strings (to allow
// partial input like "3."), validated and clamped at commit time, and
// dispatched as `UpdateAttributeValue` after a 1s pause or on focus loss.
//
// Range editing: the pill switches between one exact input and a min–max pair
// via the action bar's Range toggle. Presentation mode is derived from the
// stored value, with a local override covering the gap between toggling and
// the commit (or abandonment) that makes the stored value agree — there's no
// DB representation of an "empty range", so entering range mode writes nothing
// until both endpoints parse.
struct NumericAttribute: View {
    let entry: Entry
    let pair: NumericAttributePair
    @EnvironmentObject private var forestVM: ForestViewModel

    @State private var editMin: String = ""
    @State private var editMax: String = ""
    // Range/exact presentation override while the stored value disagrees.
    // nil = follow stored.
    @State private var modeOverride: Bool?
    @State private var debounceTask: Task<Void, Never>?
    @FocusState private var focusedField: RangeEndpoint?
    // macOS prototype (GV-36): show the action bar in a popover anchored to the
    // pill while it's focused, since macOS has no keyboard accessory.
    @State private var showActions = false
    // Set by Remove so the focus-loss handler skips flushNow() — flushing
    // would dispatch an update against the just-deleted row.
    @State private var pendingRemoval = false

    // Same attribute can be attached to multiple visible entries, so the focus
    // owner token needs both ids.
    private var focusToken: String { "\(entry.id)/\(pair.attrId)" }

    private var storedIsRange: Bool {
        if case .range = pair.actual { return true }
        return false
    }

    private var isRangeMode: Bool { modeOverride ?? storedIsRange }

    // The bar actions for this attribute, defined once and rendered identically
    // by both surfaces (iOS keyboard bar via AttributeBarPublisher, macOS popover).
    private var barActions: [AttributeBarAction] {
        .numeric(
            range: (active: isRangeMode, toggle: toggleRange),
            remove: {
                pendingRemoval = true
                forestVM.removeAttribute(entryId: entry.id, attributeId: pair.attrId)
                focusedField = nil
            }
        )
    }

    var body: some View {
        AttributeRow(label: pair.name) { field }
            .onAppear {
                syncEditState()
                #if os(macOS)
                AttributePopoverClicks.install()
                #endif
            }
            .onChange(of: pair.actual) { _, _ in
                // Skip while the user is editing — otherwise an upstream cache
                // refresh (post-commit) clobbers in-flight keystrokes.
                guard focusedField == nil else { return }
                modeOverride = nil
                syncEditState()
            }
            .onChange(of: editMin) { _, _ in scheduleDebounce() }
            .onChange(of: editMax) { _, _ in scheduleDebounce() }
            .attributeBarActions(
                token: focusToken,
                isFocused: focusedField != nil,
                actions: barActions
            )
            .onChange(of: focusedField) { _, focused in
                if focused == nil {
                    if pendingRemoval {
                        pendingRemoval = false
                        debounceTask?.cancel()
                        debounceTask = nil
                    } else if !flushNow(isBlur: true) {
                        // Nothing committed that changes the stored mode:
                        // abandon any in-flight range entry and resync. (After
                        // a range commit the refresh clears the override
                        // instead, so the pill doesn't flash exact while the
                        // write lands.)
                        modeOverride = nil
                        syncEditState()
                    }
                }
                #if os(macOS)
                showActions = focused != nil
                #endif
            }
            #if os(macOS)
            // Popover dismissal is the session boundary — except that transient
            // popovers consume the dismissing click, so a click on a text field
            // would otherwise close the popover, drop the click, and tear the
            // session down (the original range-toggle bug). Ask AppKit's
            // hit-testing where the consumed click landed and complete its
            // intent: focus that field (sessions hand over via the normal focus
            // path), no-op on the already-focused field (the popover stays
            // closed so the next click can place the caret), and end the
            // session only on a genuine click-away.
            .onChange(of: showActions) { _, shown in
                guard !shown else { return }
                guard let hit = AttributePopoverClicks.consumedTextFieldHit() else {
                    focusedField = nil
                    return
                }
                guard !AttributePopoverClicks.isFirstResponder(hit) else { return }
                DispatchQueue.main.async {
                    let ok = hit.window?.makeFirstResponder(hit) ?? false
                    if !ok { focusedField = nil }
                }
            }
            #endif
    }

    @ViewBuilder
    private var field: some View {
        RangePill(isRange: isRangeMode) {
            endpointField($editMin, .min)
        } maxInput: {
            endpointField($editMax, .max)
        }
        #if os(macOS)
        .popover(isPresented: $showActions, arrowEdge: .top) {
            AttributeSheetBar(
                title: pair.name,
                actions: barActions,
                onDismiss: { focusedField = nil }
            )
            .frame(width: 280)
        }
        #endif
    }

    private func endpointField(_ text: Binding<String>, _ endpoint: RangeEndpoint) -> some View {
        TextField("", text: text)
            .textFieldStyle(.plain)
            #if os(iOS)
            .keyboardType(pair.config.integer ? .numberPad : .decimalPad)
            #endif
            .multilineTextAlignment(.center)
            .focused($focusedField, equals: endpoint)
            .frame(minWidth: GvSpacing.minAttributeInputWidth)
            .gvSelectAllOnFocus(isFocused: focusedField == endpoint)
            .onSubmit {
                // Hardware-keyboard nicety: Enter in min advances to max.
                focusedField = (endpoint == .min && isRangeMode) ? .max : nil
            }
    }

    // MARK: - Range toggle

    private func toggleRange() {
        if isRangeMode {
            debounceTask?.cancel()
            debounceTask = nil
            modeOverride = false
            // Collapse to min: the live min field if it parses, else stored.
            if case .range(let lo, _) = pair.actual {
                let v = parseEndpoint(editMin) ?? lo
                editMin = format(v)
                forestVM.updateAttributeValue(
                    entryId: entry.id,
                    attributeId: pair.attrId,
                    field: .actual,
                    value: .numeric(.exact(v))
                )
            }
            editMax = ""
            if focusedField == .max { focusedField = .min }
        } else {
            modeOverride = true
            // Min inherits the exact value already in its field; max starts
            // empty — prefilling it too would let the debounce auto-commit a
            // degenerate "5 – 5" the user never typed. Entering the max is the
            // only sensible next action, so move focus there (keyboard focus
            // moves don't dismiss the macOS popover).
            editMax = ""
            if focusedField != nil { focusedField = .max }
        }
    }

    // MARK: - Sync cache → shadow

    private func syncEditState() {
        switch pair.actual {
        case nil:
            editMin = ""
            editMax = ""
        case .exact(let v):
            editMin = format(v)
            editMax = ""
        case .range(let lo, let hi):
            editMin = format(lo)
            editMax = format(hi)
        }
    }

    // MARK: - Commit shadow → cache

    private func scheduleDebounce() {
        debounceTask?.cancel()
        debounceTask = nil
        guard pendingCommit(isBlur: false) != nil else { return }
        debounceTask = Task {
            try? await Task.sleep(nanoseconds: 1_000_000_000)
            guard !Task.isCancelled else { return }
            await MainActor.run { flushNow(isBlur: false) }
        }
    }

    /// Commit the pending value, if any. Returns true when the dispatched
    /// commit creates or keeps a range — the blur handler uses this to avoid
    /// resetting presentation mode underneath an in-flight range write.
    @discardableResult
    private func flushNow(isBlur: Bool) -> Bool {
        debounceTask?.cancel()
        debounceTask = nil
        guard let value = pendingCommit(isBlur: isBlur) else { return false }
        forestVM.updateAttributeValue(
            entryId: entry.id,
            attributeId: pair.attrId,
            field: .actual,
            value: .numeric(value)
        )
        if case .range = value { return true }
        return false
    }

    /// The value a commit would write right now, or nil when there's nothing
    /// to commit: input that doesn't parse (mid-typing), an unchanged value,
    /// an incomplete range, or — during the debounce window only — an inverted
    /// range (min > max), which blur repairs by swapping instead.
    private func pendingCommit(isBlur: Bool) -> NumericValue? {
        if isRangeMode {
            guard var lo = parseEndpoint(editMin), var hi = parseEndpoint(editMax) else {
                return nil
            }
            if lo > hi {
                guard isBlur else { return nil }
                swap(&lo, &hi)
            }
            if case .range(let curLo, let curHi) = pair.actual, curLo == lo, curHi == hi {
                return nil
            }
            return .range(min: lo, max: hi)
        }
        guard let v = buildExact(), !sameAsCurrentExact(v) else { return nil }
        return .exact(v)
    }

    /// Parse one range endpoint from its shadow string, clamped and rounded.
    /// Empty or non-parseable → nil (an incomplete range never commits).
    private func parseEndpoint(_ raw: String) -> Double? {
        let trimmed = raw.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty,
              let parsed = Self.formatter.number(from: trimmed)?.doubleValue else {
            return nil
        }
        return clamp(parsed)
    }

    /// Build a clamped, optionally-rounded exact value from the min shadow string.
    /// - Empty input → 0 (per docs/attributes-design.md "Clear-value semantics" deferral).
    /// - Non-parseable input → nil (skip commit while user is mid-typing, e.g. "3.").
    private func buildExact() -> Double? {
        let trimmed = editMin.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { return 0 }
        guard let parsed = Self.formatter.number(from: trimmed)?.doubleValue else {
            return nil
        }
        return clamp(parsed)
    }

    private func clamp(_ value: Double) -> Double {
        var v = value
        if let lo = pair.config.min { v = Swift.max(v, lo) }
        if let hi = pair.config.max { v = Swift.min(v, hi) }
        if pair.config.integer { v = v.rounded() }
        return v
    }

    private func sameAsCurrentExact(_ new: Double) -> Bool {
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
