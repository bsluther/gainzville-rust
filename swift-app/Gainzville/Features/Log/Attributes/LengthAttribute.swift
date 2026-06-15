import SwiftUI

// Editor for length/distance attributes. A clone of `MassAttribute`: a value is
// a single measurement in a single unit, the pill renders in `displayUnit` (the
// stored value's unit, else the attribute's default), and empty fields render
// as empty placeholders, not "0".
//
// Unit switching: the bar's Units menu re-expresses the current value via
// core's conversion (`LengthValue::converted_to` over FFI). Range editing: the
// pill switches between one exact input and a min–max pair via the action bar's
// Range toggle; both endpoints share the value's unit. See `MassAttribute` for
// the full rationale on the shadow-state / debounce / override machinery —
// this file is intentionally identical except for the measure type.
struct LengthAttribute: View {
    let entry: Entry
    let pair: LengthAttributePair
    @EnvironmentObject private var forestVM: ForestViewModel

    @State private var minValue: String = ""
    @State private var maxValue: String = ""
    // Range/exact presentation override while the stored value disagrees.
    // nil = follow stored.
    @State private var modeOverride: Bool?
    // Unit picked from the bar while no stored value carries it yet (the unit
    // lives on the value, so an empty field has nowhere durable to put the
    // choice). The next committed value adopts it; abandoning the edit session
    // drops it back to the stored/default unit. nil = follow stored.
    @State private var unitOverride: LengthUnit?
    @State private var debounceTask: Task<Void, Never>?
    @FocusState private var focusedField: RangeEndpoint?
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

    /// Mirrors `LengthAttributePair::display_unit()` from core — the actual
    /// value's unit, else the plan's, else the config default — with the
    /// session's picked-but-uncommitted unit layered on top.
    private var displayUnit: LengthUnit {
        unitOverride ?? pair.actual?.unit ?? pair.plan?.unit ?? pair.config.defaultUnit
    }

    // The bar actions for this attribute, defined once and rendered identically
    // by both surfaces (iOS keyboard bar via AttributeBarPublisher, macOS popover).
    private var barActions: [AttributeBarAction] {
        .measure(
            units: LengthUnit.pickerCases.map { unit in
                UnitOption(
                    label: unit.menuLabel,
                    isSelected: unit == displayUnit,
                    select: { selectUnit(unit) }
                )
            },
            range: (active: isRangeMode, toggle: toggleRange),
            remove: {
                pendingRemoval = true
                forestVM.removeAttribute(entryId: entry.id, attributeId: pair.attrId)
                focusedField = nil
            }
        )
    }

    var body: some View {
        AttributeRow(label: pair.name) {
            lengthField
        }
        .onAppear {
            syncEditState()
            #if os(macOS)
            AttributePopoverClicks.install()
            #endif
        }
        .onChange(of: pair.actual) { _, _ in
            // Skip mid-edit to avoid clobbering keystrokes.
            guard focusedField == nil else { return }
            modeOverride = nil
            unitOverride = nil
            syncEditState()
        }
        .onChange(of: minValue) { _, _ in scheduleDebounce() }
        .onChange(of: maxValue) { _, _ in scheduleDebounce() }
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
                    // Nothing committed: abandon any in-flight range entry and
                    // resync. (When a commit was dispatched, its refresh
                    // resyncs instead — resyncing here would flash the stale
                    // stored value until the write lands.)
                    modeOverride = nil
                    unitOverride = nil
                    syncEditState()
                }
            }
        }
    }

    @ViewBuilder
    private var lengthField: some View {
        HStack(spacing: GvSpacing.sm) {
            RangePill(isRange: isRangeMode) {
                endpointField(.min)
            } maxInput: {
                endpointField(.max)
            }
            Text(displayUnit.shortLabel)
                .font(.attrLabel)
                .foregroundStyle(Color.entryTextSecondary)
                .fixedSize(horizontal: true, vertical: false)
        }
        // macOS (GV-36): anchor the action-bar popover to the pill. Closing it
        // (click-away/Enter) ends editing.
        #if os(macOS)
        .popover(
            isPresented: Binding(
                get: { focusedField != nil },
                // Popover dismissal is the session boundary — but transient
                // popovers consume the dismissing click, so complete the
                // click's intent instead of tearing the session down when it
                // landed on a text field (see AttributePopoverClicks and the
                // numeric editor's dismissal handler for the full story).
                set: { shown in
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
            ),
            arrowEdge: .top
        ) {
            AttributeSheetBar(
                title: pair.name,
                actions: barActions,
                onDismiss: { focusedField = nil }
            )
            .frame(width: 280)
        }
        #endif
    }

    private func endpointField(_ endpoint: RangeEndpoint) -> some View {
        TextField("", text: binding(endpoint))
            .textFieldStyle(.plain)
            #if os(iOS)
            .keyboardType(.decimalPad)
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

    private func binding(_ endpoint: RangeEndpoint) -> Binding<String> {
        Binding(
            get: { endpoint == .min ? minValue : maxValue },
            set: {
                if endpoint == .min { minValue = $0 } else { maxValue = $0 }
            }
        )
    }

    // MARK: - Range toggle

    private func toggleRange() {
        if isRangeMode {
            debounceTask?.cancel()
            debounceTask = nil
            modeOverride = false
            // Collapse to min: the live min field where it parses, else the
            // stored range min. Nothing to write when the range was never
            // committed.
            if case .range(let unit, let lo, _) = pair.actual {
                let collapsed = LengthMeasurement(unit: unit, value: parse(minValue) ?? lo)
                minValue = format(collapsed.value)
                forestVM.updateAttributeValue(
                    entryId: entry.id,
                    attributeId: pair.attrId,
                    field: .actual,
                    value: .length(.exact(collapsed))
                )
            }
            maxValue = ""
            if focusedField == .max { focusedField = .min }
        } else {
            modeOverride = true
            // The min field keeps the exact value already in it; the max field
            // starts empty — prefilling it too would let the debounce
            // auto-commit a degenerate "x – x" the user never typed. Entering
            // the max is the natural next action, so move focus there
            // (keyboard focus moves don't dismiss the macOS popover).
            maxValue = ""
            if focusedField != nil { focusedField = .max }
        }
    }

    // MARK: - Unit selection

    /// Switch the editor (and any current value) to `unit`. The value being
    /// re-united is whatever the user sees: a parseable pending edit when one
    /// exists, else the stored actual. Conversion happens in core
    /// (`LengthValue::converted_to` via FFI), rounded to the 2-decimal cap.
    /// Plan values are left alone for now.
    private func selectUnit(_ unit: LengthUnit) {
        guard unit != displayUnit else { return }
        debounceTask?.cancel()
        debounceTask = nil
        let current: LengthValue?
        switch pendingCommit(isBlur: true) {
        case .set(let value): current = value
        // An emptied field has nothing to convert; the clear itself still
        // dispatches on blur via the normal flush path.
        case .clear: current = nil
        case nil: current = pair.actual
        }
        unitOverride = unit
        guard let current else { return }
        let converted = current.converted(to: unit)
        // Reflect the converted magnitudes immediately — the mid-edit guard
        // skips the write's refresh sync while a field is focused. The
        // debounce these writes schedule is benign: when it fires, the
        // dispatch below has already made it a no-op (or an identical write).
        switch converted {
        case .exact(let m):
            minValue = format(m.value)
            maxValue = ""
        case .range(_, let lo, let hi):
            minValue = format(lo)
            maxValue = format(hi)
        }
        forestVM.updateAttributeValue(
            entryId: entry.id,
            attributeId: pair.attrId,
            field: .actual,
            value: .length(converted)
        )
    }

    // MARK: - Sync cache → shadow

    private func syncEditState() {
        switch pair.actual {
        case nil:
            minValue = ""
            maxValue = ""
        case .exact(let m):
            minValue = format(m.value)
            maxValue = ""
        case .range(_, let lo, let hi):
            minValue = format(lo)
            maxValue = format(hi)
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

    /// Commit the pending write, if any. Returns true when a write was
    /// dispatched — the blur handler skips its abandon-resync then, since the
    /// write's refresh will resync the shadow (and clear the mode override)
    /// once it lands; resyncing eagerly would flash the stale stored value.
    @discardableResult
    private func flushNow(isBlur: Bool) -> Bool {
        debounceTask?.cancel()
        debounceTask = nil
        switch pendingCommit(isBlur: isBlur) {
        case nil:
            return false
        case .clear:
            forestVM.clearAttributeValue(entryId: entry.id, attributeId: pair.attrId, field: .actual)
            return true
        case .set(let value):
            forestVM.updateAttributeValue(
                entryId: entry.id,
                attributeId: pair.attrId,
                field: .actual,
                value: .length(value)
            )
            return true
        }
    }

    /// An emptied exact field clears the stored value (it does not write 0);
    /// non-parseable input commits nothing while the user is mid-typing.
    private func pendingCommit(isBlur: Bool) -> PendingWrite<LengthValue>? {
        if isRangeMode {
            guard let range = buildRange(isBlur: isBlur) else { return nil }
            return .set(range)
        }
        if minValue.trimmingCharacters(in: .whitespaces).isEmpty {
            return pair.actual == nil ? nil : .clear
        }
        guard let parsed = parse(minValue) else { return nil }
        let new = LengthMeasurement(unit: displayUnit, value: parsed)
        guard !sameAsCurrentExact(new) else { return nil }
        return .set(.exact(new))
    }

    /// Build the range from the field pair, or nil when there's nothing to
    /// commit (either field empty or non-parseable mid-typing). An inverted
    /// pair (min > max) holds the commit during the debounce window and swaps
    /// at blur.
    private func buildRange(isBlur: Bool) -> LengthValue? {
        guard var lo = parse(minValue), var hi = parse(maxValue) else { return nil }
        if lo > hi {
            guard isBlur else { return nil }
            swap(&lo, &hi)
        }
        guard !sameAsCurrentRange(unit: displayUnit, min: lo, max: hi) else { return nil }
        return .range(unit: displayUnit, min: lo, max: hi)
    }

    private func parse(_ raw: String) -> Double? {
        let trimmed = raw.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty,
              let parsed = Self.formatter.number(from: trimmed)?.doubleValue else {
            return nil
        }
        // Core rejects magnitudes beyond 2 decimal places, so round before
        // dispatch — a rejected write would fail silently.
        return (parsed * 100).rounded() / 100
    }

    private func sameAsCurrentExact(_ new: LengthMeasurement) -> Bool {
        guard case .exact(let cur) = pair.actual else { return false }
        return cur == new
    }

    private func sameAsCurrentRange(unit: LengthUnit, min: Double, max: Double) -> Bool {
        guard case .range(let curUnit, let curMin, let curMax) = pair.actual else { return false }
        return curUnit == unit && curMin == min && curMax == max
    }

    // MARK: - Formatting

    private static let formatter: NumberFormatter = {
        let f = NumberFormatter()
        f.numberStyle = .decimal
        f.maximumFractionDigits = 2
        f.usesGroupingSeparator = false
        return f
    }()

    private func format(_ v: Double) -> String {
        Self.formatter.string(from: v as NSNumber) ?? String(v)
    }
}

private extension LengthValue {
    var unit: LengthUnit {
        switch self {
        case .exact(let m): return m.unit
        case .range(let unit, _, _): return unit
        }
    }

    /// Method-style sugar over the FFI free function (uniffi can't attach
    /// methods to data enums); the conversion itself runs in core.
    func converted(to unit: LengthUnit) -> LengthValue {
        lengthValueConvertedTo(value: self, unit: unit)
    }
}

private extension LengthUnit {
    // The generated enum isn't CaseIterable (conformance can't be synthesized
    // outside the declaring file), so the menu order is spelled out here:
    // metric ascending, then imperial ascending.
    static let pickerCases: [LengthUnit] = [
        .millimeter, .centimeter, .meter, .kilometer,
        .inch, .foot, .yard, .mile,
    ]

    var menuLabel: String {
        switch self {
        case .millimeter: return "Millimeters (mm)"
        case .centimeter: return "Centimeters (cm)"
        case .meter:      return "Meters (m)"
        case .kilometer:  return "Kilometers (km)"
        case .inch:       return "Inches (in)"
        case .foot:       return "Feet (ft)"
        case .yard:       return "Yards (yd)"
        case .mile:       return "Miles (mi)"
        }
    }

    var shortLabel: String {
        switch self {
        case .millimeter: return "mm"
        case .centimeter: return "cm"
        case .meter:      return "m"
        case .kilometer:  return "km"
        case .inch:       return "in"
        case .foot:       return "ft"
        case .yard:       return "yd"
        case .mile:       return "mi"
        }
    }
}
