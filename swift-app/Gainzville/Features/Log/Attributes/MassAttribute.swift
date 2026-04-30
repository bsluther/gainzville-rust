import SwiftUI

// Editor for mass (and, in the future, other measure-typed) attributes. Renders
// one number-input pill + short unit label per in-use unit. Empty fields render
// as empty placeholders, not "0", per the user-facing spec.
//
// Unit selection and conversion are deferred (see docs/attributes-design.md
// "Unit selection / conversion for measures") — the view shows the union of
// plan/actual measurement units and the attribute's `defaultUnits`, with
// `[Pound]` as the ultimate fallback.
struct MassAttribute: View {
    let entry: FfiEntry
    let pair: FfiMassAttributePair
    @EnvironmentObject private var forestVM: ForestViewModel

    @State private var values: [FfiMassUnit: String] = [:]
    @State private var debounceTask: Task<Void, Never>?
    @FocusState private var focusedUnit: FfiMassUnit?
    @EnvironmentObject private var focusModel: AttributeFocusModel

    private var focusId: AttributeFocusID {
        AttributeFocusID(entryId: entry.id, attributeId: pair.attrId)
    }

    private var isRowFocused: Bool {
        focusModel.focusedId == focusId
    }

    var body: some View {
        AttributeRow(label: pair.name, isFocused: isRowFocused, onFocus: { focusModel.focusedId = focusId }) {
            ForEach(unitsToShow, id: \.self) { unit in
                massField(unit: unit)
            }
        }
        .onAppear { syncEditState() }
        .onChange(of: pair.actual) { _, _ in
            // Skip mid-edit to avoid clobbering keystrokes.
            if focusedUnit == nil { syncEditState() }
        }
        .onChange(of: values) { _, _ in scheduleDebounce() }
        .onChange(of: focusedUnit) { _, newFocus in
            if newFocus != nil { focusModel.focusedId = focusId }
            if newFocus == nil { flushNow() }
        }
    }

    @ViewBuilder
    private func massField(unit: FfiMassUnit) -> some View {
        HStack(spacing: GvSpacing.sm) {
            TextField(rangePlaceholder(for: unit), text: bindingFor(unit))
                .textFieldStyle(.plain)
                #if os(iOS)
                .keyboardType(.decimalPad)
                #endif
                .multilineTextAlignment(.center)
                .focused($focusedUnit, equals: unit)
                .frame(minWidth: GvSpacing.minAttributeInputWidth)
                .gvAttributePill()
                .fixedSize(horizontal: true, vertical: false)
                .gvSelectAllOnFocus(isFocused: focusedUnit == unit)
                .onTapGesture { focusedUnit = unit; focusModel.focusedId = focusId }
                .onSubmit { flushNow() }
            Text(unit.shortLabel)
                .font(.attrLabel)
                .foregroundStyle(Color.entryTextSecondary)
                .fixedSize(horizontal: true, vertical: false)
        }
    }

    private func bindingFor(_ unit: FfiMassUnit) -> Binding<String> {
        Binding(
            get: { values[unit] ?? "" },
            set: { values[unit] = $0 }
        )
    }

    // MARK: - Units to show

    /// Stable union of units currently in plan/actual and the attribute's defaults,
    /// with `[Pound]` as the ultimate fallback. Stable across edits so a unit
    /// field doesn't disappear after the user fills in a value.
    private var unitsToShow: [FfiMassUnit] {
        var seen = Set<FfiMassUnit>()
        var result: [FfiMassUnit] = []
        for u in definedUnits() + pair.config.defaultUnits where !seen.contains(u) {
            seen.insert(u)
            result.append(u)
        }
        if result.isEmpty { return [.pound] }
        return result
    }

    /// Mirrors `MassAttributePair::defined_units()` from core: union of plan + actual units.
    private func definedUnits() -> [FfiMassUnit] {
        var seen = Set<FfiMassUnit>()
        var result: [FfiMassUnit] = []
        let measurements = (planMeasurements ?? []) + (actualMeasurements ?? [])
        for m in measurements where !seen.contains(m.unit) {
            seen.insert(m.unit)
            result.append(m.unit)
        }
        return result
    }

    private var planMeasurements: [FfiMassMeasurement]? {
        switch pair.plan {
        case .none: return nil
        case .exact(let m): return m
        case .range(let lo, _): return lo
        }
    }

    private var actualMeasurements: [FfiMassMeasurement]? {
        switch pair.actual {
        case .none: return nil
        case .exact(let m): return m
        case .range(let lo, _): return lo
        }
    }

    // MARK: - Range placeholder

    /// For range-valued mass, show "min – max" per unit as placeholder.
    /// Range collapses to Exact on first commit (see attributes-design.md).
    private func rangePlaceholder(for unit: FfiMassUnit) -> String {
        guard case .range(let mins, let maxes) = pair.actual else { return "" }
        let lo = mins.first(where: { $0.unit == unit })?.value
        let hi = maxes.first(where: { $0.unit == unit })?.value
        switch (lo, hi) {
        case (.some(let l), .some(let h)): return "\(format(l)) – \(format(h))"
        case (.some(let l), .none):        return format(l)
        case (.none, .some(let h)):        return format(h)
        case (.none, .none):               return ""
        }
    }

    // MARK: - Sync cache → shadow

    private func syncEditState() {
        var next: [FfiMassUnit: String] = [:]
        switch pair.actual {
        case .none, .range:
            // Empty / range: render empty fields. Range placeholder shows the value.
            for unit in unitsToShow {
                next[unit] = ""
            }
        case .exact(let measurements):
            for unit in unitsToShow {
                if let m = measurements.first(where: { $0.unit == unit }) {
                    next[unit] = format(m.value)
                } else {
                    next[unit] = ""
                }
            }
        }
        values = next
    }

    // MARK: - Commit shadow → cache

    private func scheduleDebounce() {
        debounceTask?.cancel()
        debounceTask = nil
        guard let new = buildMeasurements(), shouldCommit(new) else { return }
        debounceTask = Task {
            try? await Task.sleep(nanoseconds: 1_000_000_000)
            guard !Task.isCancelled else { return }
            await MainActor.run { flushNow() }
        }
    }

    private func flushNow() {
        debounceTask?.cancel()
        debounceTask = nil
        guard let new = buildMeasurements(), shouldCommit(new) else { return }
        forestVM.updateAttributeValue(
            entryId: entry.id,
            attributeId: pair.attrId,
            field: .actual,
            value: .mass(.exact(measurements: new))
        )
    }

    /// Build measurements from shadow values for displayed units.
    /// - Empty field: include `0` only if the unit is already part of the stored
    ///   exact value (so emptying clears-to-0); otherwise skip the unit, so an
    ///   un-touched empty field doesn't pollute a fresh value.
    /// - Non-parseable input: returns nil, skipping commit while user is mid-typing.
    private func buildMeasurements() -> [FfiMassMeasurement]? {
        var out: [FfiMassMeasurement] = []
        let currentExactUnits: Set<FfiMassUnit> = {
            if case .exact(let m) = pair.actual {
                return Set(m.map { $0.unit })
            }
            return []
        }()
        for unit in unitsToShow {
            let raw = (values[unit] ?? "").trimmingCharacters(in: .whitespaces)
            if raw.isEmpty {
                if currentExactUnits.contains(unit) {
                    out.append(FfiMassMeasurement(unit: unit, value: 0))
                }
                continue
            }
            guard let parsed = Self.formatter.number(from: raw)?.doubleValue else {
                return nil
            }
            out.append(FfiMassMeasurement(unit: unit, value: parsed))
        }
        return out
    }

    private func shouldCommit(_ new: [FfiMassMeasurement]) -> Bool {
        if new.isEmpty { return false }
        return !sameAsCurrent(new)
    }

    /// Order-insensitive comparison against the stored Exact value.
    private func sameAsCurrent(_ new: [FfiMassMeasurement]) -> Bool {
        guard case .exact(let cur) = pair.actual else { return false }
        if cur.count != new.count { return false }
        let curMap = Dictionary(uniqueKeysWithValues: cur.map { ($0.unit, $0.value) })
        let newMap = Dictionary(uniqueKeysWithValues: new.map { ($0.unit, $0.value) })
        return curMap == newMap
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

private extension FfiMassUnit {
    var shortLabel: String {
        switch self {
        case .gram:     return "g"
        case .kilogram: return "kg"
        case .pound:    return "lb"
        }
    }
}
