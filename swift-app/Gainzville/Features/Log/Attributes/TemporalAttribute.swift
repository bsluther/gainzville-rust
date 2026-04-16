import SwiftUI

// MARK: - TemporalAttribute

struct TemporalAttribute: View {
    let temporal: FfiTemporal

    @State private var isExpanded = false
    // Local edit state — sourced from `temporal` on appear/change.
    // Stage 2: wire these bindings to a dispatch of UpdateEntryTemporal.
    @State private var editStart: Date?
    @State private var editEnd: Date?
    @State private var editDurationMs: UInt32?

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.lg) {
            temporalHeader
            if isExpanded {
                TemporalExpandedRows(
                    editStart: $editStart,
                    editEnd: $editEnd,
                    editDurationMs: $editDurationMs
                )
            }
        }
        .onAppear { syncEditState() }
        .onChange(of: temporal) { _, _ in syncEditState() }
    }

    private var temporalHeader: some View {
        Button { isExpanded.toggle() } label: {
            HStack(spacing: 0) {
                Text("Time")
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextSecondary)
                Image(systemName: "chevron.down")
                    .font(.caption)
                    .foregroundStyle(Color.gvTextSecondary)
                    .rotationEffect(.degrees(isExpanded ? 180 : 0))
                    .padding(.leading, GvSpacing.sm)
                Spacer()
                Text(temporalSummary(temporal))
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextSecondary)
            }
        }
        .buttonStyle(.plain)
    }

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
}

// MARK: - Expanded rows

private struct TemporalExpandedRows: View {
    @Binding var editStart: Date?
    @Binding var editEnd: Date?
    @Binding var editDurationMs: UInt32?

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.lg) {
            TemporalFieldRow(label: "Start") {
                DatePickerPill(date: $editStart, components: .date)
                DatePickerPill(date: $editStart, components: .hourAndMinute)
            }
            TemporalFieldRow(label: "End") {
                DatePickerPill(date: $editEnd, components: .date)
                DatePickerPill(date: $editEnd, components: .hourAndMinute)
            }
            TemporalFieldRow(label: "Duration") {
                DurationPickerPill(durationMs: $editDurationMs)
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
                .font(.gvCallout)
                .foregroundStyle(Color.gvTextSecondary)
                .padding(.leading, GvSpacing.md)
            Spacer()
            HStack(spacing: GvSpacing.md) {
                content
            }
        }
    }
}

// MARK: - Date / time picker pill

private struct DatePickerPill: View {
    @Binding var date: Date?
    let components: DatePickerComponents
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
        Button {
            if date == nil { date = Date() }
            isPresenting = true
        } label: {
            Text(displayText.isEmpty ? emptyPillText : displayText)
                .gvAttributePill()
        }
        .buttonStyle(.plain)
        #if os(iOS)
        .sheet(isPresented: $isPresenting) {
            DatePickerSheet(date: pickerDate, components: components)
        }
        #else
        .popover(isPresented: $isPresenting) {
            DatePickerPopover(date: pickerDate, components: components)
        }
        #endif
    }
}

// MARK: - Duration picker pill

private struct DurationPickerPill: View {
    @Binding var durationMs: UInt32?
    @State private var isPresenting = false
    @State private var editHours = 0
    @State private var editMinutes = 0
    @State private var editSeconds = 0

    private var displayText: String {
        durationMs.map(formatDuration) ?? ""
    }

    var body: some View {
        Button {
            loadFromBinding()
            isPresenting = true
        } label: {
            Text(displayText.isEmpty ? emptyPillText : displayText)
                .gvAttributePill()
        }
        .buttonStyle(.plain)
        #if os(iOS)
        .sheet(isPresented: $isPresenting) {
            DurationPickerSheet(
                hours: $editHours,
                minutes: $editMinutes,
                seconds: $editSeconds,
                onDone: { commitToBinding() }
            )
        }
        #else
        .popover(isPresented: $isPresenting) {
            DurationPickerPopover(
                hours: $editHours,
                minutes: $editMinutes,
                seconds: $editSeconds,
                onDone: { commitToBinding(); isPresenting = false }
            )
        }
        #endif
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

// MARK: - Platform: iOS pickers (sheets)

#if os(iOS)
private struct DatePickerSheet: View {
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

private struct DurationPickerSheet: View {
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

// MARK: - Platform: macOS pickers (popovers)

#if os(macOS)
private struct DatePickerPopover: View {
    @Binding var date: Date
    let components: DatePickerComponents

    var body: some View {
        DatePicker("", selection: $date, displayedComponents: components)
            .datePickerStyle(.graphical)
            .labelsHidden()
            .padding(GvSpacing.md)
    }
}

private struct DurationPickerPopover: View {
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
            Text("\(value)")
                .font(.gvTitle)
                .monospacedDigit()
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
    func gvAttributePill() -> some View {
        self
            .font(.gvBody)
            .foregroundStyle(Color.gvTextSecondary)
            .padding(.horizontal, GvSpacing.lg)
            .padding(.vertical, GvSpacing.sm)
            .background(Color.gvSurface)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(Color.gvDivider, lineWidth: 1)
            )
    }
}

/// Fixed-width whitespace so empty pills have a consistent minimum size.
private let emptyPillText = "\u{00a0}\u{00a0}\u{00a0}\u{00a0}\u{00a0}"

// MARK: - Formatting helpers

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
