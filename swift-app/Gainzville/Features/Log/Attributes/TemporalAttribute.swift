import SwiftUI

// MARK: - Future work
//
// - Add per-field "unset" control (×-button or menu item) to clear start/end/duration.
//   macOS inline time field: nil state currently shows an empty pill; wiring should
//   preserve that — don't default-initialise to .now on load, only on user intent.
//
// Duration picker (long-term)
// - Replace the macOS stepper popover with an inline text field (e.g. "1h 30m" or
//   "1:30:00") backed by a custom NSFormatter. The stepper popover is good enough
//   for now; defer until the formatter approach is designed.
//
// Design / consistency
// - CalendarPickerMacOS and TimeFieldMacOS use NSViewRepresentable to clear system
//   backgrounds. Any new AppKit-backed pickers should follow the same pattern.
//
// Conflict resolution (nice-to-have)
// - After the user picks "Remove X" from the conflict alert, auto-open the picker
//   for the pending field instead of requiring a second tap.
//
// Root-entry / duration-only
// - Root entries (no parent) require at least a start or end time (Rust constraint).
//   Setting duration-only on a root entry will silently fail; deferred until there
//   is a clear UX for handling this (e.g. disable the duration pill, or automatically
//   pair it with a start time).

// MARK: - TemporalAttribute

private enum TemporalField { case start, end, duration }

struct TemporalAttribute: View {
    let entry: FfiEntry

    @EnvironmentObject private var forestVM: ForestViewModel
    @State private var isExpanded = false
    @State private var editStart: Date?
    @State private var editEnd: Date?
    @State private var editDurationMs: UInt32?
    @State private var debounceTask: Task<Void, Never>?
    @State private var showConflictAlert = false
    @State private var pendingField: TemporalField?

    private var temporal: FfiTemporal { entry.temporal }

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.lg) {
            temporalHeader
            if isExpanded {
                TemporalExpandedRows(
                    editStart: $editStart,
                    editEnd: $editEnd,
                    editDurationMs: $editDurationMs,
                    onBeforeEditStart: { gateEdit(for: .start) },
                    onBeforeEditEnd: { gateEdit(for: .end) },
                    onBeforeEditDuration: { gateEdit(for: .duration) }
                )
            }
        }
        .onAppear { syncEditState() }
        .onChange(of: temporal) { _, _ in syncEditState() }
        .onChange(of: editStart)      { _, _ in scheduleDebounce() }
        .onChange(of: editEnd)        { _, _ in scheduleDebounce() }
        .onChange(of: editDurationMs) { _, _ in scheduleDebounce() }
        .onChange(of: isExpanded)     { _, newValue in if !newValue { flushNow() } }
        .alert("Too many values set", isPresented: $showConflictAlert) {
            if editStart != nil && pendingField != .start {
                Button("Remove Start") { editStart = nil; pendingField = nil }
            }
            if editEnd != nil && pendingField != .end {
                Button("Remove End") { editEnd = nil; pendingField = nil }
            }
            if editDurationMs != nil && pendingField != .duration {
                Button("Remove Duration") { editDurationMs = nil; pendingField = nil }
            }
            Button("Cancel", role: .cancel) { pendingField = nil }
        } message: {
            Text("Only 2 of 3 time values can be set. Which would you like to remove?")
        }
    }

    private var temporalHeader: some View {
        Button { isExpanded.toggle() } label: {
            HStack(spacing: 0) {
                Text("Time")
                    .font(.attrLabel)
                    .foregroundStyle(Color.gvTextSecondary)
                Image(systemName: "chevron.down")
                    .font(.caption)
                    .foregroundStyle(Color.gvTextSecondary)
                    .rotationEffect(.degrees(isExpanded ? 180 : 0))
                    .padding(.leading, GvSpacing.entrySpacing)
                Spacer()
                Text(temporalSummary(temporal))
                    .font(.attrLabel.italic())
                    .foregroundStyle(Color.gvTextSecondary)
            }
            .contentShape(Rectangle())
            .frame(minHeight: GvSpacing.minAttributeHeight)
        }
        .buttonStyle(.plain)
    }

    // MARK: - Conflict gate

    /// Called before a pill opens its editor. Returns true if the edit is allowed,
    /// false if a conflict alert has been raised instead.
    private func gateEdit(for field: TemporalField) -> Bool {
        // Editing an already-set field is always allowed.
        switch field {
        case .start: if editStart != nil { return true }
        case .end:   if editEnd   != nil { return true }
        case .duration: if editDurationMs != nil { return true }
        }
        // Setting a new field when 2 are already set would over-specify the temporal.
        let setCount = [editStart != nil, editEnd != nil, editDurationMs != nil]
            .filter { $0 }.count
        if setCount >= 2 {
            pendingField = field
            showConflictAlert = true
            return false
        }
        return true
    }

    // MARK: - Sync / persist

    private func syncEditState() {
        switch temporal {
        case .none:
            editStart = nil; editEnd = nil; editDurationMs = nil
        case .start(let ms):
            editStart = msToDate(ms); editEnd = nil; editDurationMs = nil
        case .end(let ms):
            editStart = nil; editEnd = msToDate(ms); editDurationMs = nil
        case .duration(let ms):
            editStart = nil; editEnd = nil; editDurationMs = ms
        case .startAndEnd(let s, let e):
            editStart = msToDate(s); editEnd = msToDate(e); editDurationMs = nil
        case .startAndDuration(let s, let d):
            editStart = msToDate(s); editEnd = nil; editDurationMs = d
        case .durationAndEnd(let d, let e):
            editStart = nil; editEnd = msToDate(e); editDurationMs = d
        }
    }

    private func buildFfiTemporal() -> FfiTemporal {
        switch (editStart, editEnd, editDurationMs) {
        case (nil, nil, nil):                       return .none
        case (.some(let s), nil, nil):              return .start(start: dateToMs(s))
        case (nil, .some(let e), nil):              return .end(end: dateToMs(e))
        case (nil, nil, .some(let d)):              return .duration(duration: d)
        case (.some(let s), .some(let e), nil):     return .startAndEnd(start: dateToMs(s), end: dateToMs(e))
        case (.some(let s), nil, .some(let d)):     return .startAndDuration(start: dateToMs(s), durationMs: d)
        case (nil, .some(let e), .some(let d)):     return .durationAndEnd(durationMs: d, end: dateToMs(e))
        case (.some(let s), .some(let e), .some):   return .startAndEnd(start: dateToMs(s), end: dateToMs(e))
        }
    }

    private func scheduleDebounce() {
        debounceTask?.cancel()
        debounceTask = nil
        // Skip write if the edit state matches what's already stored.
        guard buildFfiTemporal() != entry.temporal else { return }
        debounceTask = Task {
            try? await Task.sleep(nanoseconds: 1_000_000_000)
            guard !Task.isCancelled else { return }
            await MainActor.run {
                forestVM.updateEntryTemporal(entry: entry, temporal: buildFfiTemporal())
            }
        }
    }

    private func flushNow() {
        debounceTask?.cancel()
        debounceTask = nil
        let newTemporal = buildFfiTemporal()
        guard newTemporal != entry.temporal else { return }
        forestVM.updateEntryTemporal(entry: entry, temporal: newTemporal)
    }
}

// MARK: - Expanded rows

private struct TemporalExpandedRows: View {
    @Binding var editStart: Date?
    @Binding var editEnd: Date?
    @Binding var editDurationMs: UInt32?
    var onBeforeEditStart: () -> Bool = { true }
    var onBeforeEditEnd: () -> Bool = { true }
    var onBeforeEditDuration: () -> Bool = { true }

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.lg) {
            TemporalFieldRow(label: "Start") {
                DatePickerPill(date: $editStart, components: .date, onBeforeEdit: onBeforeEditStart)
                DatePickerPill(date: $editStart, components: .hourAndMinute, onBeforeEdit: onBeforeEditStart)
            }
            TemporalFieldRow(label: "End") {
                DatePickerPill(date: $editEnd, components: .date, onBeforeEdit: onBeforeEditEnd)
                DatePickerPill(date: $editEnd, components: .hourAndMinute, onBeforeEdit: onBeforeEditEnd)
            }
            TemporalFieldRow(label: "Duration") {
                DurationPickerPill(durationMs: $editDurationMs, onBeforeEdit: onBeforeEditDuration)
            }
        }
    }
}

// MARK: - Field row layout

private struct TemporalFieldRow<Content: View>: View {
    let label: String
    private let content: Content

    init(label: String, @ViewBuilder content: () -> Content) {
        self.label = label
        self.content = content()
    }

    var body: some View {
        HStack(alignment: .center) {
            Text(label)
                .font(.attrLabel)
                .foregroundStyle(Color.entryTextSecondary)
                .padding(.leading, GvSpacing.lg)
            Spacer()
            HStack(spacing: GvSpacing.lg) {
                content
            }
        }
        .frame(minHeight: GvSpacing.minAttributeHeight)
    }
}

// MARK: - Date / time picker pill

private struct DatePickerPill: View {
    @Binding var date: Date?
    let components: DatePickerComponents
    var onBeforeEdit: () -> Bool = { true }
    @State private var isPresenting = false

    private var displayText: String {
        guard let date else { return "" }
        return components == .date ? formatDate(from: date) : formatTime(from: date)
    }

    // Non-optional binding for the picker; initialises to now when date is nil.
    private var pickerDate: Binding<Date> {
        Binding(
            get: { date ?? Date() },
            set: { date = $0 }
        )
    }

    var body: some View {
        #if os(macOS)
        if components == .hourAndMinute {
            if date != nil {
                TimeFieldMacOS(selection: pickerDate)
                    .fixedSize()
                    .padding(.leading, GvSpacing.sm)
                    .padding(.trailing, GvSpacing.lg)
                    .padding(.vertical, GvSpacing.sm)
                    .frame(minHeight: GvSpacing.minAttributeHeight)
                    .background(RoundedRectangle(cornerRadius: 8).fill(Color.gvSurface))
                    .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.gvDivider, lineWidth: 1))
            } else {
                Button {
                    guard onBeforeEdit() else { return }
                    date = Date()
                } label: {
                    Text(emptyPillText).gvAttributePill()
                }
                .buttonStyle(.plain)
            }
        } else {
            pillButton
        }
        #else
        pillButton
        #endif
    }

    private var pillButton: some View {
        Button {
            guard onBeforeEdit() else { return }
            if date == nil { date = Date() }
            isPresenting = true
        } label: {
            Text(displayText.isEmpty ? emptyPillText : displayText)
                .gvAttributePill()
        }
        .buttonStyle(.plain)
        .platformPopover(isPresented: $isPresenting) {
            #if os(iOS)
            DatePickerIOS(date: pickerDate, components: components)
            #else
            DatePickerMacOS(date: pickerDate, components: components)
            #endif
        }
    }
}

// MARK: - Duration picker pill

private struct DurationPickerPill: View {
    @Binding var durationMs: UInt32?
    var onBeforeEdit: () -> Bool = { true }
    @State private var isPresenting = false
    @State private var editHours = 0
    @State private var editMinutes = 0
    @State private var editSeconds = 0

    private var displayText: String {
        durationMs.map(formatDuration) ?? ""
    }

    var body: some View {
        Button {
            guard onBeforeEdit() else { return }
            loadFromBinding()
            isPresenting = true
        } label: {
            Text(displayText.isEmpty ? emptyPillText : displayText)
                .gvAttributePill()
        }
        .buttonStyle(.plain)
        .platformPopover(isPresented: $isPresenting) {
            #if os(iOS)
            DurationPickerIOS(
                hours: $editHours,
                minutes: $editMinutes,
                seconds: $editSeconds,
                onDone: { commitToBinding() }
            )
            #else
            DurationPickerMacOS(
                hours: $editHours,
                minutes: $editMinutes,
                seconds: $editSeconds,
                onDone: { commitToBinding(); isPresenting = false }
            )
            #endif
        }
    }

    private func loadFromBinding() {
        let total = (durationMs ?? 0) / 1000
        editHours = Int(total / 3600)
        editMinutes = Int((total % 3600) / 60)
        editSeconds = Int(total % 60)
    }

    private func commitToBinding() {
        let total = UInt32(editHours * 3600 + editMinutes * 60 + editSeconds)
        durationMs = total == 0 ? nil : total * 1000
    }
}

// MARK: - iOS pickers

#if os(iOS)
private struct DatePickerIOS: View {
    @Binding var date: Date
    let components: DatePickerComponents
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                if components == .date {
                    DatePicker("", selection: $date, displayedComponents: components)
                        .datePickerStyle(.graphical)
                        .labelsHidden()
                } else {
                    DatePicker("", selection: $date, displayedComponents: components)
                        .datePickerStyle(.wheel)
                        .labelsHidden()
                }
            }
            .padding()
            .navigationTitle(components == .date ? "Date" : "Time")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium])
    }
}

private struct DurationPickerIOS: View {
    @Binding var hours: Int
    @Binding var minutes: Int
    @Binding var seconds: Int
    let onDone: () -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            HStack(spacing: 0) {
                durationWheelColumn(range: 0..<24, label: "Hours", selection: $hours)
                durationWheelColumn(range: 0..<60, label: "Minutes", selection: $minutes)
                durationWheelColumn(range: 0..<60, label: "Seconds", selection: $seconds)
            }
            .navigationTitle("Duration")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { onDone(); dismiss() }
                }
            }
        }
        .presentationDetents([.medium])
    }
}

@ViewBuilder
private func durationWheelColumn(range: Range<Int>, label: String, selection: Binding<Int>) -> some View {
    VStack(spacing: GvSpacing.sm) {
        Picker(label, selection: selection) {
            ForEach(range, id: \.self) { n in
                Text("\(n)").tag(n)
            }
        }
        .pickerStyle(.wheel)
        .frame(maxWidth: .infinity)
        Text(label)
            .font(.gvCaption)
            .foregroundStyle(Color.gvTextSecondary)
    }
}
#endif

// MARK: - macOS pickers

#if os(macOS)
/// macOS time field backed by NSDatePicker with transparent background,
/// so custom pill styling can be applied without a double-box appearance.
private struct TimeFieldMacOS: NSViewRepresentable {
    @Binding var selection: Date

    func makeNSView(context: Context) -> NSDatePicker {
        let picker = NSDatePicker()
        picker.datePickerStyle = .textField
        picker.datePickerElements = .hourMinuteSecond
        picker.isBezeled = false
        picker.drawsBackground = false
        picker.textColor = NSColor(Color.gvAttributeField)
        picker.dateValue = selection
        picker.target = context.coordinator
        picker.action = #selector(Coordinator.dateChanged(_:))
        return picker
    }

    func updateNSView(_ picker: NSDatePicker, context: Context) {
        if picker.dateValue != selection { picker.dateValue = selection }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    class Coordinator: NSObject {
        var parent: TimeFieldMacOS
        init(_ p: TimeFieldMacOS) { parent = p }
        @objc func dateChanged(_ sender: NSDatePicker) { parent.selection = sender.dateValue }
    }
}

private struct DatePickerMacOS: View {
    @Binding var date: Date
    let components: DatePickerComponents

    var body: some View {
        CalendarPickerMacOS(selection: $date, components: components)
            .padding(GvSpacing.md)
    }
}

private struct DurationPickerMacOS: View {
    @Binding var hours: Int
    @Binding var minutes: Int
    @Binding var seconds: Int
    let onDone: () -> Void

    var body: some View {
        VStack(spacing: GvSpacing.lg) {
            HStack(spacing: GvSpacing.xl) {
                DurationStepperColumn(label: "Hours", value: $hours, range: 0...23)
                DurationStepperColumn(label: "Minutes", value: $minutes, range: 0...59)
                DurationStepperColumn(label: "Seconds", value: $seconds, range: 0...59)
            }
            Button("Done") { onDone() }
                .keyboardShortcut(.defaultAction)
        }
        .padding(GvSpacing.lg)
        .frame(minWidth: 240)
    }
}

private struct DurationStepperColumn: View {
    let label: String
    @Binding var value: Int
    let range: ClosedRange<Int>

    var body: some View {
        VStack(spacing: GvSpacing.sm) {
            TextField("", value: $value, format: .number)
                .font(.gvTitle)
                .monospacedDigit()
                .multilineTextAlignment(.center)
                .frame(minWidth: 40)
            Stepper("", value: $value, in: range)
                .labelsHidden()
            Text(label)
                .font(.gvCaption)
                .foregroundStyle(Color.gvTextSecondary)
        }
    }
}
#endif

// MARK: - AttributePill style

/// Shared pill style for attribute value display across all attribute types.
/// Apply with `.gvAttributePill()`.
extension View {
    func gvAttributePill(borderColor: Color = .entryTextSecondary) -> some View {
        self
            .font(.attrField)
            .foregroundStyle(Color.entryTextPrimary)
            .padding(.horizontal, GvSpacing.lg)
            .padding(.vertical, GvSpacing.sm)
            .frame(minHeight: GvSpacing.minAttributeHeight)
            .background(.clear)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(borderColor, lineWidth: 1)
            )
    }
}

/// Fixed-width whitespace so empty pills have a consistent minimum size.
private let emptyPillText = "\u{00a0}\u{00a0}\u{00a0}\u{00a0}\u{00a0}"

// MARK: - Formatting helpers

private func dateToMs(_ date: Date) -> Int64 {
    Int64(date.timeIntervalSince1970 * 1000)
}

private func msToDate(_ ms: Int64) -> Date {
    Date(timeIntervalSince1970: Double(ms) / 1000)
}

private func formatTime(from date: Date) -> String {
    date.formatted(date: .omitted, time: .shortened)
}

private func formatDate(from date: Date) -> String {
    date.formatted(.dateTime.month(.abbreviated).day())
}

private func formatDuration(_ ms: UInt32) -> String {
    let total = ms / 1000
    let h = total / 3600
    let m = (total % 3600) / 60
    let s = total % 60
    var parts: [String] = []
    if h > 0 { parts.append("\(h)h") }
    if m > 0 { parts.append("\(m)m") }
    if s > 0 || parts.isEmpty { parts.append("\(s)s") }
    return parts.joined(separator: " ")
}

private func temporalSummary(_ temporal: FfiTemporal) -> String {
    switch temporal {
    case .none: return ""
    case .start(let ms): return formatTime(from: msToDate(ms))
    case .end(let ms): return formatTime(from: msToDate(ms))
    case .duration(let ms): return formatDuration(ms)
    case .startAndEnd(let s, let e):
        return formatRange(startMs: s, endMs: e, durationMs: Int64(e) - Int64(s))
    case .startAndDuration(let s, let d):
        return formatRange(startMs: s, endMs: s + Int64(d), durationMs: Int64(d))
    case .durationAndEnd(let d, let e):
        return formatRange(startMs: e - Int64(d), endMs: e, durationMs: Int64(d))
    }
}

private func formatRange(startMs: Int64, endMs: Int64, durationMs: Int64) -> String {
    if durationMs < 60_000 {
        return formatTime(from: msToDate(startMs))
    }
    let start = msToDate(startMs)
    let end = msToDate(endMs)
    if Calendar.current.isDate(start, inSameDayAs: end) {
        return "\(formatTime(from: start)) – \(formatTime(from: end))"
    } else {
        return "\(formatDate(from: start)) – \(formatDate(from: end))"
    }
}
