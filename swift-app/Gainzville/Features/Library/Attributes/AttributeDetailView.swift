import SwiftUI
internal import Combine

// View model for AttributeDetailView. Subscribes to the attribute by id so
// edits (dispatched as UpdateAttribute) reflect live, and exposes a single
// `apply(_:)` for the config editors to push an AttributeChange.
@MainActor
final class AttributeDetailViewModel: ObservableObject {
    @Published private(set) var attribute: Attribute?

    private var core: GainzvilleCore?
    private var attributeId: String = ""
    private var subscription: FfiQuerySubscription?
    private var cancellable: AnyCancellable?
    private var started = false

    func start(core: GainzvilleCore?, dataChange: DataChange, attributeId: String) {
        guard !started, let core else { return }
        started = true
        self.core = core
        self.attributeId = attributeId
        subscription = try? core.subscribeQuery(
            query: .findAttributeById(FindAttributeById(attributeId: attributeId)))
        refresh()
        cancellable = dataChange.didChange.sink { [weak self] in self?.refresh() }
    }

    private func refresh() {
        guard let core else { return }
        if case .findAttributeById(let attr) =
            core.readQuery(query: .findAttributeById(FindAttributeById(attributeId: attributeId))) {
            attribute = attr
        }
    }

    func apply(_ change: AttributeChange) {
        guard let core else { return }
        try? core.runAction(action: .updateAttribute(UpdateAttribute(
            actorId: SYSTEM_ACTOR_ID, attributeId: attributeId, change: change)))
    }
}

struct AttributeDetailView: View {
    let attribute: Attribute

    @EnvironmentObject private var coreEnv: CoreEnv
    @EnvironmentObject private var dataChange: DataChange
    @StateObject private var vm = AttributeDetailViewModel()

    // Prefer the live value; fall back to the value passed in for the first
    // frame before the subscription populates.
    private var live: Attribute { vm.attribute ?? attribute }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: GvSpacing.xl) {
                GvDetailSection(title: "Name") {
                    Text(live.name)
                        .font(.gvBody)
                        .foregroundStyle(Color.gvTextPrimary)
                }

                GvDetailSection(title: "Description") {
                    Text(live.description ?? "No description")
                        .font(.gvBody)
                        .foregroundStyle(live.description != nil ? Color.gvTextPrimary : Color.gvTextSecondary)
                }

                GvDetailSection(title: "Type") {
                    Text(live.config.typeName)
                        .font(.gvBody)
                        .foregroundStyle(Color.gvTextPrimary)
                }

                GvDetailSection(title: "Config") {
                    configEditor
                        .padding(.top, GvSpacing.lg)
                }
            }
            .padding(GvSpacing.xl)
            .gvReadableWidth(alignment: .topLeading)
        }
        .background(Color.gvBackground)
        .navigationTitle(live.name)
        .onAppear {
            vm.start(core: coreEnv.core, dataChange: dataChange, attributeId: attribute.id)
        }
    }

    @ViewBuilder
    private var configEditor: some View {
        switch live.config {
        case .numeric(let cfg):
            NumericConfigEditor(config: cfg) { vm.apply(.numeric(.setDefault($0))) }
        case .select(let cfg):
            SelectConfigEditor(config: cfg) { vm.apply(.select(.setDefault($0))) }
        case .multiselect(let cfg):
            MultiselectConfigEditor(config: cfg)
        case .mass(let cfg):
            MassConfigEditor(config: cfg) { vm.apply(.mass(.setDefaultUnit($0))) }
        case .length(let cfg):
            LengthConfigEditor(config: cfg) { vm.apply(.length(.setDefaultUnit($0))) }
        case .text(let cfg):
            TextConfigEditor(
                config: cfg,
                onSetDefault: { vm.apply(.text(.setDefault($0))) },
                onSetAutocomplete: { vm.apply(.text(.setAutocomplete($0))) }
            )
        }
    }
}

// MARK: - Shared config-row layout

/// Label on the left, a value control on the right — mirrors the log's
/// attribute-row rhythm so config reads as the same visual language.
private struct ConfigRow<Control: View>: View {
    let label: String
    @ViewBuilder var control: () -> Control

    var body: some View {
        HStack {
            Text(label)
                .font(.gvBody)
                .foregroundStyle(Color.gvTextPrimary)
            Spacer()
            control()
        }
    }
}

// Editable config controls get the brighter border; read-only ones recede.
private let editableBorder = Color.gvNeutral400

/// Read-only pill, styled like an editable one but non-interactive — used for
/// config fields that aren't editable in this phase (min, max, options).
private struct ReadOnlyPill: View {
    let text: String
    var body: some View {
        Text(text.isEmpty ? gvEmptyPillText : text)
            .frame(minWidth: GvSpacing.minAttributeInputWidth)
            .gvAttributePill()
    }
}

// MARK: - Numeric

private struct NumericConfigEditor: View {
    let config: NumericConfig
    let onSetDefault: (Double?) -> Void

    var body: some View {
        VStack(spacing: GvSpacing.xl) {
            ConfigRow(label: "Default") {
                NumericDefaultField(config: config, onCommit: onSetDefault)
            }
            ConfigRow(label: "Min") { ReadOnlyPill(text: format(config.min)) }
            ConfigRow(label: "Max") { ReadOnlyPill(text: format(config.max)) }
            ConfigRow(label: "Integer") {
                // Read-only display of the integer flag, same checkbox glyph as
                // the editable controls (no toggle action wired in this phase).
                Image(systemName: config.integer ? "checkmark.square" : "square")
                    .resizable().scaledToFit().frame(width: 20, height: 20)
                    .foregroundStyle(Color.entryTextSecondary)
            }
        }
    }

    private func format(_ v: Double?) -> String {
        guard let v else { return "—" }
        return config.integer ? String(Int(v.rounded())) : String(v)
    }
}

/// Editable numeric default. Clamps to the config bounds and rounds for integer
/// attributes on commit so the dispatched change is always valid; empty clears
/// the default (None).
private struct NumericDefaultField: View {
    let config: NumericConfig
    let onCommit: (Double?) -> Void

    @State private var text: String = ""
    @FocusState private var focused: Bool

    var body: some View {
        TextField("None", text: $text)
            .textFieldStyle(.plain)
            .multilineTextAlignment(.center)
            #if os(iOS)
            .keyboardType(config.integer ? .numberPad : .decimalPad)
            #endif
            .focused($focused)
            .frame(minWidth: GvSpacing.minAttributeInputWidth)
            .gvAttributePill(borderColor: editableBorder)
            .fixedSize(horizontal: true, vertical: false)
            // numberPad/decimalPad have no return key, so onSubmit never fires;
            // the Done button resigns first responder, which commits via the
            // focus-loss handler below.
            .gvKeyboardDoneButton()
            .onAppear { text = format(config.default) }
            .onChange(of: config.default) { _, _ in if !focused { text = format(config.default) } }
            .onChange(of: focused) { _, isFocused in if !isFocused { commit() } }
            .onSubmit { commit() }
    }

    private func commit() {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty {
            if config.default != nil { onCommit(nil) }
            return
        }
        guard var v = Double(trimmed) else {
            text = format(config.default)  // unparseable — revert
            return
        }
        if let lo = config.min { v = Swift.max(v, lo) }
        if let hi = config.max { v = Swift.min(v, hi) }
        // Core rejects defaults beyond 2 decimal places, so round before
        // dispatch — a rejected write would fail silently.
        if config.integer {
            v = v.rounded()
        } else {
            v = (v * 100).rounded() / 100
        }
        if v != config.default { onCommit(v) }
        text = format(v)
    }

    private func format(_ v: Double?) -> String {
        guard let v else { return "" }
        return config.integer ? String(Int(v.rounded())) : String(v)
    }
}

// MARK: - Select

private struct SelectConfigEditor: View {
    let config: SelectConfig
    let onSetDefault: (String?) -> Void

    @State private var isPicking = false

    var body: some View {
        VStack(spacing: GvSpacing.xl) {
            ConfigRow(label: "Default") {
                Button { isPicking = true } label: {
                    Text(config.default ?? "None")
                        .frame(minWidth: GvSpacing.minAttributeInputWidth)
                        .gvAttributePill(borderColor: editableBorder)
                }
                .buttonStyle(.plain)
                .platformPopover(isPresented: $isPicking) {
                    DefaultOptionList(
                        options: config.options,
                        selection: config.default,
                        onPick: { picked in onSetDefault(picked); isPicking = false }
                    )
                }
            }
            // Options are read-only in this phase (adding/renaming is a later,
            // additive-only edit).
            ConfigRow(label: "Options") {
                Text(config.options.isEmpty ? "None" : config.options.joined(separator: ", "))
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextPrimary)
                    .multilineTextAlignment(.trailing)
            }
            ConfigRow(label: "Ordered") {
                Image(systemName: config.ordered ? "checkmark.square" : "square")
                    .resizable().scaledToFit().frame(width: 20, height: 20)
                    .foregroundStyle(Color.entryTextSecondary)
            }
        }
    }
}

// Read-only in this phase: multiselect config editing (default, options) is
// deferred, so there is no change action to wire up. Mirrors the select
// editor's layout without the interactive default picker.
private struct MultiselectConfigEditor: View {
    let config: MultiselectConfig

    var body: some View {
        VStack(spacing: GvSpacing.xl) {
            ConfigRow(label: "Default") {
                Text(defaultLabel)
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextPrimary)
                    .multilineTextAlignment(.trailing)
            }
            ConfigRow(label: "Options") {
                Text(config.options.isEmpty ? "None" : config.options.joined(separator: ", "))
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextPrimary)
                    .multilineTextAlignment(.trailing)
            }
        }
    }

    private var defaultLabel: String {
        guard let selected = config.default, !selected.isEmpty else { return "None" }
        return selected.joined(separator: ", ")
    }
}

/// Option picker. Offers a "None" row to clear the default unless the config
/// requires a value (e.g. mass's default unit).
private struct DefaultOptionList: View {
    let options: [String]
    let selection: String?
    var includeNone = true
    let onPick: (String?) -> Void

    var body: some View {
        #if os(iOS)
        NavigationStack { list.navigationTitle("Default").navigationBarTitleDisplayMode(.inline) }
            .presentationDetents([.medium, .large])
        #else
        list.padding(GvSpacing.md).frame(minWidth: 220)
        #endif
    }

    private var list: some View {
        ScrollView {
            VStack(spacing: 0) {
                if includeNone {
                    row(label: "None", value: nil, isSelected: selection == nil)
                }
                ForEach(options, id: \.self) { option in
                    row(label: option, value: option, isSelected: option == selection)
                }
            }
        }
    }

    private func row(label: String, value: String?, isSelected: Bool) -> some View {
        Button { onPick(value) } label: {
            HStack {
                Spacer()
                Text(label).font(.gvBody).foregroundStyle(Color.gvTextPrimary)
                Spacer()
            }
            .overlay(alignment: .trailing) {
                if isSelected {
                    Image(systemName: "checkmark").foregroundStyle(Color.gvLoggedBlue)
                }
            }
            .padding(.horizontal, GvSpacing.lg)
            .padding(.vertical, GvSpacing.lg)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Mass

private struct MassConfigEditor: View {
    let config: MassConfig
    let onSetUnit: (MassUnit) -> Void

    @State private var isPicking = false

    // Stable display order for the unit options.
    private let allUnits: [MassUnit] = [.gram, .kilogram, .pound]

    var body: some View {
        ConfigRow(label: "Default unit") {
            Button { isPicking = true } label: {
                Text(label(for: config.defaultUnit))
                    .frame(minWidth: GvSpacing.minAttributeInputWidth)
                    .gvAttributePill(borderColor: editableBorder)
            }
            .buttonStyle(.plain)
            .platformPopover(isPresented: $isPicking) {
                // No "None" row: a mass config always has a default unit.
                DefaultOptionList(
                    options: allUnits.map(label(for:)),
                    selection: label(for: config.defaultUnit),
                    includeNone: false,
                    onPick: { picked in
                        if let unit = allUnits.first(where: { label(for: $0) == picked }),
                           unit != config.defaultUnit {
                            onSetUnit(unit)
                        }
                        isPicking = false
                    }
                )
            }
        }
    }

    private func label(for unit: MassUnit) -> String {
        switch unit {
        case .gram:     return "Grams"
        case .kilogram: return "Kilograms"
        case .pound:    return "Pounds"
        }
    }
}

// MARK: - Length

/// Clone of `MassConfigEditor` for length attributes — a default-unit picker
/// over the eight length units (metric then imperial).
private struct LengthConfigEditor: View {
    let config: LengthConfig
    let onSetUnit: (LengthUnit) -> Void

    @State private var isPicking = false

    private let allUnits: [LengthUnit] = [
        .millimeter, .centimeter, .meter, .kilometer,
        .inch, .foot, .yard, .mile,
    ]

    var body: some View {
        ConfigRow(label: "Default unit") {
            Button { isPicking = true } label: {
                Text(label(for: config.defaultUnit))
                    .frame(minWidth: GvSpacing.minAttributeInputWidth)
                    .gvAttributePill(borderColor: editableBorder)
            }
            .buttonStyle(.plain)
            .platformPopover(isPresented: $isPicking) {
                // No "None" row: a length config always has a default unit.
                DefaultOptionList(
                    options: allUnits.map(label(for:)),
                    selection: label(for: config.defaultUnit),
                    includeNone: false,
                    onPick: { picked in
                        if let unit = allUnits.first(where: { label(for: $0) == picked }),
                           unit != config.defaultUnit {
                            onSetUnit(unit)
                        }
                        isPicking = false
                    }
                )
            }
        }
    }

    private func label(for unit: LengthUnit) -> String {
        switch unit {
        case .millimeter: return "Millimeters"
        case .centimeter: return "Centimeters"
        case .meter:      return "Meters"
        case .kilometer:  return "Kilometers"
        case .inch:       return "Inches"
        case .foot:       return "Feet"
        case .yard:       return "Yards"
        case .mile:       return "Miles"
        }
    }
}

// MARK: - Text

private struct TextConfigEditor: View {
    let config: TextConfig
    let onSetDefault: (String?) -> Void
    let onSetAutocomplete: (Bool) -> Void

    var body: some View {
        VStack(spacing: GvSpacing.xl) {
            ConfigRow(label: "Default") {
                TextDefaultField(config: config, onCommit: onSetDefault)
            }
            // Checkbox glyph like the read-only "Ordered"/"Integer" flags, but
            // tappable and in the brighter primary tone since autocomplete is
            // editable (the read-only flags use the receded secondary tone).
            ConfigRow(label: "Autocomplete") {
                Button {
                    onSetAutocomplete(!config.autocomplete)
                } label: {
                    Image(systemName: config.autocomplete ? "checkmark.square" : "square")
                        .resizable().scaledToFit().frame(width: 20, height: 20)
                        .foregroundStyle(Color.gvTextPrimary)
                }
                .buttonStyle(.plain)
            }
        }
    }
}

/// Editable text default — empty clears it (None); stored verbatim otherwise.
private struct TextDefaultField: View {
    let config: TextConfig
    let onCommit: (String?) -> Void

    @State private var text: String = ""
    @FocusState private var focused: Bool

    var body: some View {
        TextField("None", text: $text)
            .textFieldStyle(.plain)
            .multilineTextAlignment(.center)
            .focused($focused)
            .frame(minWidth: GvSpacing.minAttributeInputWidth)
            .gvAttributePill(borderColor: editableBorder)
            .fixedSize(horizontal: true, vertical: false)
            .onAppear { text = config.default ?? "" }
            .onChange(of: config.default) { _, _ in if !focused { text = config.default ?? "" } }
            .onChange(of: focused) { _, isFocused in if !isFocused { commit() } }
            .onSubmit { commit() }
    }

    private func commit() {
        if text.isEmpty {
            if config.default != nil { onCommit(nil) }
            return
        }
        if text != config.default { onCommit(text) }
    }
}

private extension AttributeConfig {
    var typeName: String {
        switch self {
        case .numeric:     return "Numeric"
        case .select:      return "Select"
        case .multiselect: return "Multiselect"
        case .mass:        return "Mass"
        case .length:      return "Length"
        case .text:        return "Text"
        }
    }
}
