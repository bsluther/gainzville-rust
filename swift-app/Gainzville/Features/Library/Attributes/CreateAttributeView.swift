import SwiftUI

/// Create-attribute sheet. Collects name, description, type, and a type-specific
/// config, mirroring the detail card's section rhythm (GvDetailSection headers +
/// pill inputs) rather than a stock Form. The config section is hidden until a
/// type is chosen; changing the type after editing config confirms-and-clears it.
///
/// The picker lists all six attribute types; selecting one reveals that type's
/// config editor.
struct CreateAttributeView: View {
    /// name, optional description, validated config — built only when Create is enabled.
    var onCreate: (String, String?, AttributeConfig) -> Void

    @Environment(\.dismiss) private var dismiss

    // Universal fields (kept across type changes).
    @State private var name = ""
    @State private var description = ""

    // Type selection + picker/alert presentation.
    @State private var selectedType: AttributeTypeKind?
    @State private var showingTypePicker = false
    @State private var showingTypeChangeAlert = false
    @State private var pendingType: AttributeTypeKind?

    // Per-type config drafts. Cleared by `resetConfigDrafts()` on a type change.
    @State private var textDefault = ""
    @State private var textAutocomplete = false

    @State private var numMinText = ""
    @State private var numMaxText = ""
    @State private var numDefaultText = ""
    @State private var numInteger = false

    @State private var massUnit: MassUnit = .kilogram
    @State private var lengthUnit: LengthUnit = .meter
    @State private var showingUnitPicker = false

    @State private var selectOptions: [OptionDraft] = []
    @State private var selectOrdered = false
    @State private var selectDefault: String?
    @State private var showingSelectDefaultPicker = false

    @State private var multiOptions: [OptionDraft] = []
    @State private var multiDefault: Set<String> = []
    @State private var showingMultiDefaultPicker = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: GvSpacing.xl) {
                    GvDetailSection(title: "Name") { nameField }
                    GvDetailSection(title: "Description") { descriptionField }
                    GvDetailSection(title: "Type") { typeField }
                    if selectedType != nil {
                        GvDetailSection(title: "Config") {
                            VStack(alignment: .leading, spacing: GvSpacing.md) {
                                configEditor
                                if let message = configValidationMessage {
                                    Text(message)
                                        .font(.gvCaption)
                                        .foregroundStyle(Color.gvTextSecondary)
                                }
                            }
                            .padding(.top, GvSpacing.lg)
                        }
                    }
                }
                .padding(GvSpacing.xl)
                .gvReadableWidth(alignment: .topLeading)
            }
            // Keep the dark fill full-height under the keyboard; the ScrollView
            // content still insets/scrolls so the focused field stays visible.
            .background {
                Color.gvBackground.ignoresSafeArea(.keyboard, edges: .bottom)
            }
            .navigationTitle("New Attribute")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") { create() }
                        .disabled(!canCreate)
                }
            }
            .gvKeyboardDoneButton()
            .alert("Change type?", isPresented: $showingTypeChangeAlert) {
                Button("Change", role: .destructive) {
                    if let pendingType { applyType(pendingType) }
                    pendingType = nil
                }
                Button("Cancel", role: .cancel) { pendingType = nil }
            } message: {
                Text("This clears the config you entered for the current type. Name and description are kept.")
            }
        }
        .gvSheetChrome()
    }

    // MARK: - Universal fields

    private var nameField: some View {
        TextField("Name", text: $name)
            .textFieldStyle(.plain)
            .frame(maxWidth: .infinity, alignment: .leading)
            .gvAttributePill(borderColor: editableBorder)
    }

    private var descriptionField: some View {
        TextField("Description (optional)", text: $description, axis: .vertical)
            .textFieldStyle(.plain)
            .lineLimit(3...6)
            .frame(maxWidth: .infinity, alignment: .leading)
            .gvAttributePill(borderColor: editableBorder, verticalPadding: GvSpacing.md)
    }

    // MARK: - Type field + picker

    private var typeField: some View {
        Button {
            pendingType = nil  // clear any stale pick so it can't get stuck
            showingTypePicker = true
        } label: {
            HStack {
                Text(selectedType?.displayName ?? "Select type")
                    .foregroundStyle(selectedType == nil ? Color.gvTextSecondary : Color.entryTextPrimary)
                Spacer()
                Image(systemName: "chevron.down")
                    .font(.caption)
                    .foregroundStyle(Color.gvTextSecondary)
            }
            .frame(maxWidth: .infinity)
            .gvAttributePill(borderColor: editableBorder)
            // Make the whole pill (incl. the gap between text and chevron) the
            // tap target, not just the opaque text/chevron.
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .platformPopover(
            isPresented: $showingTypePicker,
            onDismiss: {
                // The picker has fully dismissed; now it's safe to present the
                // confirm alert (presenting it mid-dismissal can drop it). A pick
                // that didn't need confirming left pendingType nil.
                if pendingType != nil { showingTypeChangeAlert = true }
            },
            // macOS: default anchor (centered on the field's bounds), dropping
            // downward — arrowEdge .bottom puts the popover below the field.
            arrowEdge: .bottom
        ) {
            OptionPickerList(
                title: "Type",
                options: AttributeTypeKind.allCases.map(\.displayName),
                selection: selectedType?.displayName
            ) { picked in
                if let picked, let kind = AttributeTypeKind.allCases.first(where: { $0.displayName == picked }) {
                    selectType(kind)
                }
            }
        }
    }

    /// Pick a type. A no-op when unchanged; otherwise applies immediately unless
    /// the current type already has edited config — then we stash the pick and
    /// the picker's onDismiss raises the confirm alert once it has dismissed.
    private func selectType(_ kind: AttributeTypeKind) {
        showingTypePicker = false
        guard kind != selectedType else { return }
        if selectedType != nil && isConfigDirty {
            pendingType = kind
        } else {
            applyType(kind)
        }
    }

    private func applyType(_ kind: AttributeTypeKind) {
        selectedType = kind
        resetConfigDrafts()
    }

    private func resetConfigDrafts() {
        textDefault = ""
        textAutocomplete = false
        numMinText = ""
        numMaxText = ""
        numDefaultText = ""
        numInteger = false
        massUnit = .kilogram
        lengthUnit = .meter
        selectOptions = []
        selectOrdered = false
        selectDefault = nil
        multiOptions = []
        multiDefault = []
    }

    /// Whether the current type's config has been edited away from its reset state
    /// — gates the confirm-and-clear alert on a type change.
    private var isConfigDirty: Bool {
        switch selectedType {
        case .text:
            return !textDefault.isEmpty || textAutocomplete
        case .numeric:
            return !numMinText.isEmpty || !numMaxText.isEmpty
                || !numDefaultText.isEmpty || numInteger
        case .mass:
            return massUnit != .kilogram
        case .length:
            return lengthUnit != .meter
        case .select:
            return !effectiveSelectOptions.isEmpty || selectOrdered || selectDefault != nil
        case .multiselect:
            return !effectiveMultiOptions.isEmpty || !multiDefault.isEmpty
        default:
            return false
        }
    }

    // MARK: - Config editor

    @ViewBuilder
    private var configEditor: some View {
        switch selectedType {
        case .text:
            textConfigEditor
        case .numeric:
            numericConfigEditor
        case .mass:
            massConfigEditor
        case .length:
            lengthConfigEditor
        case .select:
            selectConfigEditor
        case .multiselect:
            multiselectConfigEditor
        case .none:
            EmptyView()
        }
    }

    private var textConfigEditor: some View {
        VStack(spacing: GvSpacing.xl) {
            ConfigRow(label: "Default") {
                TextField("None", text: $textDefault)
                    .textFieldStyle(.plain)
                    .multilineTextAlignment(.center)
                    .frame(minWidth: GvSpacing.minAttributeInputWidth)
                    .gvAttributePill(borderColor: editableBorder)
                    .fixedSize(horizontal: true, vertical: false)
            }
            ConfigRow(label: "Autocomplete") {
                checkbox(isOn: textAutocomplete) { textAutocomplete.toggle() }
            }
        }
    }

    private var numericConfigEditor: some View {
        VStack(spacing: GvSpacing.xl) {
            ConfigRow(label: "Min") { numberField($numMinText) }
            ConfigRow(label: "Max") { numberField($numMaxText) }
            ConfigRow(label: "Default") { numberField($numDefaultText) }
            ConfigRow(label: "Integer") {
                checkbox(isOn: numInteger) { numInteger.toggle() }
            }
        }
    }

    // Mass and Length each carry only a required default unit, so their editors
    // share one compact unit picker (no "None" — a unit is always set).

    private let massUnits: [MassUnit] = [.gram, .kilogram, .pound]
    private let lengthUnits: [LengthUnit] = [
        .millimeter, .centimeter, .meter, .kilometer,
        .inch, .foot, .yard, .mile,
    ]

    private var massConfigEditor: some View {
        ConfigRow(label: "Default unit") {
            unitPicker(selectedLabel: massLabel(massUnit), options: massUnits.map(massLabel)) { picked in
                if let unit = massUnits.first(where: { massLabel($0) == picked }) { massUnit = unit }
            }
        }
    }

    private var lengthConfigEditor: some View {
        ConfigRow(label: "Default unit") {
            unitPicker(selectedLabel: lengthLabel(lengthUnit), options: lengthUnits.map(lengthLabel)) { picked in
                if let unit = lengthUnits.first(where: { lengthLabel($0) == picked }) { lengthUnit = unit }
            }
        }
    }

    /// A compact pill that opens the option picker over `options`, reporting the
    /// chosen label back through `onPick`.
    private func unitPicker(
        selectedLabel: String,
        options: [String],
        onPick: @escaping (String) -> Void
    ) -> some View {
        Button { showingUnitPicker = true } label: {
            Text(selectedLabel)
                .frame(minWidth: GvSpacing.minAttributeInputWidth)
                .gvAttributePill(borderColor: editableBorder)
        }
        .buttonStyle(.plain)
        .platformPopover(isPresented: $showingUnitPicker) {
            OptionPickerList(title: "Default unit", options: options, selection: selectedLabel) { picked in
                if let picked { onPick(picked) }
                showingUnitPicker = false
            }
        }
    }

    private func massLabel(_ unit: MassUnit) -> String {
        switch unit {
        case .gram:     return "Grams"
        case .kilogram: return "Kilograms"
        case .pound:    return "Pounds"
        }
    }

    private func lengthLabel(_ unit: LengthUnit) -> String {
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

    private func numberField(_ text: Binding<String>) -> some View {
        TextField("None", text: text)
            .textFieldStyle(.plain)
            .multilineTextAlignment(.center)
            #if os(iOS)
            // numberPad has no minus key, so negative bounds aren't reachable on
            // iOS — same limitation as the detail view's numeric field.
            .keyboardType(numInteger ? .numberPad : .decimalPad)
            #endif
            .frame(minWidth: GvSpacing.minAttributeInputWidth)
            .gvAttributePill(borderColor: editableBorder)
            .fixedSize(horizontal: true, vertical: false)
    }

    private func checkbox(isOn: Bool, toggle: @escaping () -> Void) -> some View {
        Button(action: toggle) {
            Image(systemName: isOn ? "checkmark.square" : "square")
                .resizable().scaledToFit().frame(width: 20, height: 20)
                .foregroundStyle(Color.gvTextSecondary)
        }
        .buttonStyle(.plain)
    }

    // MARK: - Select

    private var selectConfigEditor: some View {
        VStack(alignment: .leading, spacing: GvSpacing.xl) {
            optionsBuilder($selectOptions)
            ConfigRow(label: "Ordered") {
                checkbox(isOn: selectOrdered) { selectOrdered.toggle() }
            }
            ConfigRow(label: "Default") { selectDefaultField }
        }
    }

    private var selectDefaultField: some View {
        Button { showingSelectDefaultPicker = true } label: {
            Text(validSelectDefault ?? "None")
                .frame(minWidth: GvSpacing.minAttributeInputWidth)
                .gvAttributePill(borderColor: editableBorder)
        }
        .buttonStyle(.plain)
        .platformPopover(isPresented: $showingSelectDefaultPicker) {
            OptionPickerList(
                title: "Default",
                options: effectiveSelectOptions,
                selection: validSelectDefault,
                includeNone: true
            ) { picked in
                selectDefault = picked
                showingSelectDefaultPicker = false
            }
        }
    }

    /// Options as core will store them: trimmed, with blank rows dropped.
    private var effectiveSelectOptions: [String] {
        selectOptions
            .map { $0.text.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }

    /// The chosen default, but only while it still names an existing option.
    private var validSelectDefault: String? {
        guard let chosen = selectDefault, effectiveSelectOptions.contains(chosen) else { return nil }
        return chosen
    }

    // MARK: - Multiselect

    private var multiselectConfigEditor: some View {
        VStack(alignment: .leading, spacing: GvSpacing.xl) {
            optionsBuilder($multiOptions)
            ConfigRow(label: "Default") { multiDefaultField }
        }
    }

    private var multiDefaultField: some View {
        Button { showingMultiDefaultPicker = true } label: {
            Text(multiDefaultLabel)
                .frame(minWidth: GvSpacing.minAttributeInputWidth)
                .gvAttributePill(borderColor: editableBorder)
        }
        .buttonStyle(.plain)
        .platformPopover(isPresented: $showingMultiDefaultPicker) {
            MultiPickerList(
                title: "Default",
                options: effectiveMultiOptions,
                selected: validMultiDefault,
                onToggle: { toggleMultiDefault($0) }
            )
        }
    }

    private var multiDefaultLabel: String {
        let chosen = effectiveMultiOptions.filter { validMultiDefault.contains($0) }
        return chosen.isEmpty ? "None" : chosen.joined(separator: ", ")
    }

    private func toggleMultiDefault(_ option: String) {
        if multiDefault.contains(option) { multiDefault.remove(option) }
        else { multiDefault.insert(option) }
    }

    /// Options as core will store them: trimmed, with blank rows dropped.
    private var effectiveMultiOptions: [String] {
        multiOptions
            .map { $0.text.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }

    /// The chosen default members, restricted to options that still exist.
    private var validMultiDefault: Set<String> {
        multiDefault.intersection(effectiveMultiOptions)
    }

    // MARK: - Options builder (Select + Multiselect)

    /// The "Options" label + the shared OptionsEditor. Create enables delete and
    /// drag-to-reorder; the detail card will later pass canDelete: false.
    private func optionsBuilder(_ options: Binding<[OptionDraft]>) -> some View {
        VStack(alignment: .leading, spacing: GvSpacing.md) {
            Text("Options")
                .font(.gvBody)
                .foregroundStyle(Color.gvTextSecondary)
            OptionsEditor(options: options, canDelete: true, canReorder: true, borderColor: editableBorder)
        }
    }

    // MARK: - Config building + validation

    /// What the current draft resolves to: a buildable config, a validation
    /// message, or unsupported (no type selected, or its editor isn't built yet).
    /// A `.ready` config always passes core's `config.validate()`.
    private var configState: ConfigDraftState {
        switch selectedType {
        case .text:
            // Mirror core's MAX_TEXT_LEN so an over-length default can't pass
            // Create only to be silently rejected by core. unicodeScalars.count
            // matches Rust's chars().count().
            if textDefault.unicodeScalars.count > maxTextDefaultLength {
                return .invalid("Default can be at most \(maxTextDefaultLength) characters.")
            }
            return .ready(.text(TextConfig(
                default: textDefault.isEmpty ? nil : textDefault,
                autocomplete: textAutocomplete
            )))
        case .numeric:
            do { return .ready(.numeric(try numericConfig())) }
            catch let error as ConfigError { return .invalid(error.message) }
            catch { return .invalid("Invalid configuration.") }
        case .mass:
            return .ready(.mass(MassConfig(defaultUnit: massUnit)))
        case .length:
            return .ready(.length(LengthConfig(defaultUnit: lengthUnit)))
        case .select:
            return selectConfigState()
        case .multiselect:
            return multiselectConfigState()
        case .none:
            return .unsupported
        }
    }

    /// Mirrors core's SelectConfig validation: at least one option, options
    /// unique. The default is kept only if it's still among the options.
    private func selectConfigState() -> ConfigDraftState {
        let options = effectiveSelectOptions
        if options.isEmpty { return .invalid("Add at least one option.") }
        if Set(options).count != options.count { return .invalid("Options must be unique.") }
        return .ready(.select(SelectConfig(
            options: options, ordered: selectOrdered, default: validSelectDefault
        )))
    }

    /// Mirrors core's MultiselectConfig validation: at least one option, options
    /// unique, each at most `maxMultiselectOptionLength` scalars. The default
    /// subset is kept in option order; empty means no default (None).
    private func multiselectConfigState() -> ConfigDraftState {
        let options = effectiveMultiOptions
        if options.isEmpty { return .invalid("Add at least one option.") }
        if Set(options).count != options.count { return .invalid("Options must be unique.") }
        if options.contains(where: { $0.unicodeScalars.count > maxMultiselectOptionLength }) {
            return .invalid("Options must be \(maxMultiselectOptionLength) characters or fewer.")
        }
        let chosen = options.filter { validMultiDefault.contains($0) }
        return .ready(.multiselect(MultiselectConfig(
            options: options, default: chosen.isEmpty ? nil : chosen
        )))
    }

    private let maxMultiselectOptionLength = 40
    private let maxTextDefaultLength = 10_000

    private var configValidationMessage: String? {
        if case .invalid(let message) = configState { return message }
        return nil
    }

    /// Mirrors core's NumericConfig validation: each bound/default finite, whole
    /// when `integer`, at most 2 decimals; min ≤ max; default within [min, max].
    /// Throws `ConfigError` with a user-facing message on the first violation.
    private func numericConfig() throws -> NumericConfig {
        func parse(_ raw: String, _ label: String) throws -> Double? {
            let trimmed = raw.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty { return nil }
            guard let v = Double(trimmed), v.isFinite else {
                throw ConfigError("\(label) must be a number.")
            }
            if numInteger && v.rounded(.towardZero) != v {
                throw ConfigError("\(label) must be a whole number.")
            }
            if !atMostTwoDecimals(v) {
                throw ConfigError("\(label) can have at most 2 decimal places.")
            }
            return v
        }

        let minV = try parse(numMinText, "Min")
        let maxV = try parse(numMaxText, "Max")
        let defV = try parse(numDefaultText, "Default")

        if let lo = minV, let hi = maxV, lo > hi {
            throw ConfigError("Min must be less than or equal to Max.")
        }
        if let d = defV {
            if let lo = minV, d < lo { throw ConfigError("Default must be greater than or equal to Min.") }
            if let hi = maxV, d > hi { throw ConfigError("Default must be less than or equal to Max.") }
        }

        return NumericConfig(min: minV, max: maxV, integer: numInteger, default: defV)
    }

    private func atMostTwoDecimals(_ v: Double) -> Bool {
        v.rounded(.towardZero) == v || (v * 100).rounded() / 100 == v
    }

    // MARK: - Create

    private var canCreate: Bool {
        guard !name.trimmingCharacters(in: .whitespaces).isEmpty else { return false }
        if case .ready = configState { return true }
        return false
    }

    private func create() {
        guard case .ready(let config) = configState else { return }
        let trimmedName = name.trimmingCharacters(in: .whitespaces)
        let trimmedDesc = description.trimmingCharacters(in: .whitespacesAndNewlines)
        onCreate(trimmedName, trimmedDesc.isEmpty ? nil : trimmedDesc, config)
        dismiss()
    }
}

/// Resolution of the create form's per-type config draft.
private enum ConfigDraftState {
    case ready(AttributeConfig)   // buildable; passes core validation
    case invalid(String)          // a user-facing reason Create is blocked
    case unsupported              // no type selected, or editor not built yet
}

private struct ConfigError: Error {
    let message: String
    init(_ message: String) { self.message = message }
}

// MARK: - Attribute type kind

/// The attribute types offered in the create picker. Distinct from
/// `AttributeConfig` (which carries config data) — this is the bare type
/// identity the user selects before any config exists. Whether a type can be
/// built yet is determined by `configState` (`.unsupported` for not-yet-built types).
private enum AttributeTypeKind: String, CaseIterable, Identifiable {
    case numeric, select, multiselect, mass, length, text

    var id: String { rawValue }

    var displayName: String {
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

// MARK: - Option picker

/// Modal string-option picker used by the type and default-unit fields. Styled
/// like the detail view's option pickers (centered rows, trailing checkmark;
/// sheet on iOS, popover on macOS). `onPick` reports nil only when `includeNone`
/// is set and the "None" row is chosen.
private struct OptionPickerList: View {
    let title: String
    let options: [String]
    let selection: String?
    var includeNone: Bool = false
    let onPick: (String?) -> Void

    var body: some View {
        #if os(iOS)
        NavigationStack { list.navigationTitle(title).navigationBarTitleDisplayMode(.inline) }
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

/// Multi-select variant of `OptionPickerList`: tapping a row toggles its
/// membership (checkmark) and the list stays open. Used for a multiselect
/// attribute's default subset.
private struct MultiPickerList: View {
    let title: String
    let options: [String]
    let selected: Set<String>
    let onToggle: (String) -> Void

    var body: some View {
        #if os(iOS)
        NavigationStack { list.navigationTitle(title).navigationBarTitleDisplayMode(.inline) }
            .presentationDetents([.medium, .large])
        #else
        list.padding(GvSpacing.md).frame(minWidth: 220)
        #endif
    }

    private var list: some View {
        ScrollView {
            VStack(spacing: 0) {
                ForEach(options, id: \.self) { option in
                    Button { onToggle(option) } label: {
                        HStack {
                            Spacer()
                            Text(option).font(.gvBody).foregroundStyle(Color.gvTextPrimary)
                            Spacer()
                        }
                        .overlay(alignment: .trailing) {
                            if selected.contains(option) {
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
        }
    }
}

// MARK: - Shared config-row layout

// Mirrors AttributeDetailView's ConfigRow so config edits read as the same
// visual language. Kept local for now; consolidate into the design system once
// Select/Multiselect land and the shared shape across create + detail is stable.
private struct ConfigRow<Control: View>: View {
    let label: String
    @ViewBuilder var control: () -> Control

    var body: some View {
        HStack {
            Text(label)
                .font(.gvBody)
                .foregroundStyle(Color.gvTextSecondary)
            Spacer()
            control()
        }
    }
}

// Border tone for the editable input pills. Dimmer than the detail view's
// editable border (gvNeutral400) — the create form has no read-only pills to
// contrast against, so a more subdued border reads better here.
private let editableBorder = Color.gvNeutral550
