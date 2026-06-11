import SwiftUI

// Editor for mass (and, in the future, other measure-typed) attributes. Renders
// one pill + short unit label per in-use unit. Empty fields render as empty
// placeholders, not "0", per the user-facing spec.
//
// Unit selection and conversion are deferred (see docs/attributes-design.md
// "Unit selection / conversion for measures") — the view shows the union of
// plan/actual measurement units and the attribute's `defaultUnits`, with
// `[Pound]` as the ultimate fallback.
//
// Range editing: each unit's pill switches between one exact input and a
// min–max pair via the action bar's Range toggle. `MassValue.range` is one
// value, so the commit is whole-value: every unit with content must have a
// complete, parseable pair (per-unit inverted pairs hold during the debounce
// and swap at blur). Mode is derived from the stored value with a local
// override covering the gap between toggling and the first commit.
struct MassAttribute: View {
    let entry: Entry
    let pair: MassAttributePair
    @EnvironmentObject private var forestVM: ForestViewModel

    private struct FieldKey: Hashable {
        let unit: MassUnit
        let endpoint: RangeEndpoint
    }

    @State private var minValues: [MassUnit: String] = [:]
    @State private var maxValues: [MassUnit: String] = [:]
    // Range/exact presentation override while the stored value disagrees.
    // nil = follow stored.
    @State private var modeOverride: Bool?
    @State private var debounceTask: Task<Void, Never>?
    @FocusState private var focusedField: FieldKey?
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
        .mass(
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
            ForEach(unitsToShow, id: \.self) { unit in
                massField(unit: unit)
            }
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
            syncEditState()
        }
        .onChange(of: minValues) { _, _ in scheduleDebounce() }
        .onChange(of: maxValues) { _, _ in scheduleDebounce() }
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
                    // Nothing committed that changes the stored mode: abandon
                    // any in-flight range entry and resync. (After a range
                    // commit the refresh clears the override instead, so the
                    // pills don't flash exact while the write lands.)
                    modeOverride = nil
                    syncEditState()
                }
            }
        }
    }

    @ViewBuilder
    private func massField(unit: MassUnit) -> some View {
        HStack(spacing: GvSpacing.sm) {
            RangePill(isRange: isRangeMode) {
                endpointField(unit, .min)
            } maxInput: {
                endpointField(unit, .max)
            }
            Text(unit.shortLabel)
                // Monospaced + padded labels give every unit a consistent width
                // so the pills line up across rows regardless of unit length.
                .font(.attrLabel.monospaced())
                .foregroundStyle(Color.entryTextSecondary)
                .fixedSize(horizontal: true, vertical: false)
        }
        // macOS (GV-36): anchor the action-bar popover to the unit's pill
        // (driven off the focused field's unit so the arrow points at the pill,
        // not the whole row). Closing it (click-away/Enter) ends editing.
        #if os(macOS)
        .popover(
            isPresented: Binding(
                get: { focusedField?.unit == unit },
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

    private func endpointField(_ unit: MassUnit, _ endpoint: RangeEndpoint) -> some View {
        TextField("", text: binding(unit, endpoint))
            .textFieldStyle(.plain)
            #if os(iOS)
            .keyboardType(.decimalPad)
            #endif
            .multilineTextAlignment(.center)
            .focused($focusedField, equals: FieldKey(unit: unit, endpoint: endpoint))
            .frame(minWidth: GvSpacing.minAttributeInputWidth)
            .gvSelectAllOnFocus(isFocused: focusedField == FieldKey(unit: unit, endpoint: endpoint))
            .onSubmit {
                // Hardware-keyboard nicety: Enter in min advances to max.
                focusedField = (endpoint == .min && isRangeMode)
                    ? FieldKey(unit: unit, endpoint: .max)
                    : nil
            }
    }

    private func binding(_ unit: MassUnit, _ endpoint: RangeEndpoint) -> Binding<String> {
        Binding(
            get: { endpoint == .min ? (minValues[unit] ?? "") : (maxValues[unit] ?? "") },
            set: {
                if endpoint == .min { minValues[unit] = $0 } else { maxValues[unit] = $0 }
            }
        )
    }

    // MARK: - Units to show

    /// Stable union of units currently in plan/actual and the attribute's defaults,
    /// with `[Pound]` as the ultimate fallback. Stable across edits so a unit
    /// field doesn't disappear after the user fills in a value.
    private var unitsToShow: [MassUnit] {
        var seen = Set<MassUnit>()
        var result: [MassUnit] = []
        for u in definedUnits() + pair.config.defaultUnits where !seen.contains(u) {
            seen.insert(u)
            result.append(u)
        }
        if result.isEmpty { return [.pound] }
        return result
    }

    /// Mirrors `MassAttributePair::defined_units()` from core: union of plan + actual units.
    private func definedUnits() -> [MassUnit] {
        var seen = Set<MassUnit>()
        var result: [MassUnit] = []
        let measurements = (planMeasurements ?? []) + (actualMeasurements ?? [])
        for m in measurements where !seen.contains(m.unit) {
            seen.insert(m.unit)
            result.append(m.unit)
        }
        return result
    }

    private var planMeasurements: [MassMeasurement]? {
        switch pair.plan {
        case .none: return nil
        case .exact(let m): return m
        case .range(let lo, _): return lo
        }
    }

    private var actualMeasurements: [MassMeasurement]? {
        switch pair.actual {
        case .none: return nil
        case .exact(let m): return m
        case .range(let lo, _): return lo
        }
    }

    // MARK: - Range toggle

    private func toggleRange() {
        if isRangeMode {
            debounceTask?.cancel()
            debounceTask = nil
            modeOverride = false
            // Collapse to min: live min fields where they parse, stored values
            // elsewhere. Nothing to write when the range was never committed.
            if case .range(let lo, _) = pair.actual {
                let collapsed = collapseMinMeasurements(fallback: lo)
                for m in collapsed { minValues[m.unit] = format(m.value) }
                forestVM.updateAttributeValue(
                    entryId: entry.id,
                    attributeId: pair.attrId,
                    field: .actual,
                    value: .mass(.exact(collapsed))
                )
            }
            maxValues = [:]
            if let f = focusedField, f.endpoint == .max {
                focusedField = FieldKey(unit: f.unit, endpoint: .min)
            }
        } else {
            modeOverride = true
            // Min fields keep the exact values already in them; max fields
            // start empty — prefilling them too would let the debounce
            // auto-commit a degenerate "x – x" the user never typed. Entering
            // the max is the natural next action, so move focus there
            // (keyboard focus moves don't dismiss the macOS popover).
            maxValues = [:]
            if let unit = focusedField?.unit {
                focusedField = FieldKey(unit: unit, endpoint: .max)
            }
        }
    }

    private func collapseMinMeasurements(fallback: [MassMeasurement]) -> [MassMeasurement] {
        var out: [MassMeasurement] = []
        for unit in unitsToShow {
            if let v = parse(minValues[unit] ?? "") {
                out.append(MassMeasurement(unit: unit, value: v))
            } else if let f = fallback.first(where: { $0.unit == unit }) {
                out.append(f)
            }
        }
        return out.isEmpty ? fallback : out
    }

    // MARK: - Sync cache → shadow

    private func syncEditState() {
        var nextMin: [MassUnit: String] = [:]
        var nextMax: [MassUnit: String] = [:]
        let fill = { (side: [MassMeasurement], unit: MassUnit) -> String in
            side.first(where: { $0.unit == unit }).map { self.format($0.value) } ?? ""
        }
        for unit in unitsToShow {
            switch pair.actual {
            case nil:
                nextMin[unit] = ""
                nextMax[unit] = ""
            case .exact(let measurements):
                nextMin[unit] = fill(measurements, unit)
                nextMax[unit] = ""
            case .range(let lo, let hi):
                nextMin[unit] = fill(lo, unit)
                nextMax[unit] = fill(hi, unit)
            }
        }
        minValues = nextMin
        maxValues = nextMax
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
            value: .mass(value)
        )
        if case .range = value { return true }
        return false
    }

    private func pendingCommit(isBlur: Bool) -> MassValue? {
        if isRangeMode {
            return buildRange(isBlur: isBlur)
        }
        guard let new = buildExactMeasurements(), !new.isEmpty, !sameAsCurrentExact(new) else {
            return nil
        }
        return .exact(new)
    }

    /// Build the whole range from shadow pairs, or nil when there's nothing to
    /// commit. `MassValue.range` is one value, so this is all-or-nothing:
    /// - A unit with both fields empty that isn't part of the stored range is
    ///   skipped (an untouched default unit shouldn't block the commit).
    /// - Every other unit must have a complete, parseable pair.
    /// - An inverted pair (min > max) holds the commit during the debounce
    ///   window and swaps at blur — same-unit comparison, so no conversion
    ///   is involved (core defers cross-unit ordering checks entirely).
    private func buildRange(isBlur: Bool) -> MassValue? {
        let storedRangeUnits: Set<MassUnit> = {
            if case .range(let lo, let hi) = pair.actual {
                return Set((lo + hi).map { $0.unit })
            }
            return []
        }()
        var mins: [MassMeasurement] = []
        var maxes: [MassMeasurement] = []
        for unit in unitsToShow {
            let rawMin = (minValues[unit] ?? "").trimmingCharacters(in: .whitespaces)
            let rawMax = (maxValues[unit] ?? "").trimmingCharacters(in: .whitespaces)
            if rawMin.isEmpty, rawMax.isEmpty, !storedRangeUnits.contains(unit) {
                continue
            }
            guard var lo = parse(rawMin), var hi = parse(rawMax) else { return nil }
            if lo > hi {
                guard isBlur else { return nil }
                swap(&lo, &hi)
            }
            mins.append(MassMeasurement(unit: unit, value: lo))
            maxes.append(MassMeasurement(unit: unit, value: hi))
        }
        guard !mins.isEmpty, !sameAsCurrentRange(mins, maxes) else { return nil }
        return .range(min: mins, max: maxes)
    }

    /// Build exact measurements from the min-side shadow values.
    /// - Empty field: include `0` only if the unit is already part of the stored
    ///   exact value (so emptying clears-to-0); otherwise skip the unit, so an
    ///   un-touched empty field doesn't pollute a fresh value.
    /// - Non-parseable input: returns nil, skipping commit while user is mid-typing.
    private func buildExactMeasurements() -> [MassMeasurement]? {
        var out: [MassMeasurement] = []
        let currentExactUnits: Set<MassUnit> = {
            if case .exact(let m) = pair.actual {
                return Set(m.map { $0.unit })
            }
            return []
        }()
        for unit in unitsToShow {
            let raw = (minValues[unit] ?? "").trimmingCharacters(in: .whitespaces)
            if raw.isEmpty {
                if currentExactUnits.contains(unit) {
                    out.append(MassMeasurement(unit: unit, value: 0))
                }
                continue
            }
            guard let parsed = parse(raw) else { return nil }
            out.append(MassMeasurement(unit: unit, value: parsed))
        }
        return out
    }

    private func parse(_ raw: String) -> Double? {
        let trimmed = raw.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return nil }
        return Self.formatter.number(from: trimmed)?.doubleValue
    }

    private func sameAsCurrentExact(_ new: [MassMeasurement]) -> Bool {
        guard case .exact(let cur) = pair.actual else { return false }
        return measurementsEqual(cur, new)
    }

    private func sameAsCurrentRange(_ mins: [MassMeasurement], _ maxes: [MassMeasurement]) -> Bool {
        guard case .range(let curLo, let curHi) = pair.actual else { return false }
        return measurementsEqual(curLo, mins) && measurementsEqual(curHi, maxes)
    }

    /// Order-insensitive comparison of measurement lists.
    private func measurementsEqual(_ a: [MassMeasurement], _ b: [MassMeasurement]) -> Bool {
        guard a.count == b.count else { return false }
        let aMap = Dictionary(uniqueKeysWithValues: a.map { ($0.unit, $0.value) })
        let bMap = Dictionary(uniqueKeysWithValues: b.map { ($0.unit, $0.value) })
        return aMap == bMap
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
        Self.formatter.string(from: v as NSNumber) ?? String(v)
    }
}

private extension MassUnit {
    var shortLabel: String {
        switch self {
        // Pad single-char units to two chars so, under a monospaced font, every
        // unit label occupies the same width.
        case .gram:     return "g "
        case .kilogram: return "kg"
        case .pound:    return "lb"
        }
    }
}
