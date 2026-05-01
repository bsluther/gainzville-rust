import SwiftUI

/// A pending drop onto the day's root, awaiting the user's choice of start time.
struct PendingRootDrop: Identifiable {
    let id = UUID()
    let entry: FfiEntry
    let day: LogDay
}

/// Modal time picker shown after a successful day-root drop.
///
/// The date is anchored to the dropped day (read-only, shown in the title);
/// only time-of-day is editable. On confirm, the caller dispatches the move.
struct RootDropTimePickerSheet: View {
    let day: LogDay
    let initialTime: Date
    let onConfirm: (Date) -> Void
    let onCancel: () -> Void
    @State private var selected: Date

    init(
        day: LogDay,
        initialTime: Date,
        onConfirm: @escaping (Date) -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.day = day
        self.initialTime = initialTime
        self.onConfirm = onConfirm
        self.onCancel = onCancel
        _selected = State(initialValue: Self.timeOnDay(day: day, time: initialTime))
    }

    /// Pin `time`'s hour+minute onto `day.start`, ignoring `time`'s date components.
    private static func timeOnDay(day: LogDay, time: Date) -> Date {
        let calendar = Calendar.current
        let comps = calendar.dateComponents([.hour, .minute], from: time)
        return calendar.date(
            bySettingHour: comps.hour ?? 12,
            minute: comps.minute ?? 0,
            second: 0,
            of: day.start
        ) ?? day.start
    }

    private var titleText: String {
        "Choose a time on " + day.start.formatted(.dateTime.month(.abbreviated).day())
    }

    var body: some View {
        #if os(iOS)
        NavigationStack {
            VStack(spacing: GvSpacing.lg) {
                // Title sits in content (not the navigation bar) so a long
                // formatted date isn't truncated between the toolbar buttons.
                Text(titleText)
                    .font(.system(size: 16))
                    .foregroundStyle(Color.gvTextPrimary)
                    .frame(maxWidth: .infinity, alignment: .center)
                DatePicker("", selection: $selected, displayedComponents: .hourAndMinute)
                    .datePickerStyle(.wheel)
                    .labelsHidden()
                Spacer()
            }
            .padding(GvSpacing.lg)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", action: onCancel)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Confirm") { onConfirm(selected) }
                }
            }
        }
        .presentationDetents([.medium])
        #else
        VStack(spacing: GvSpacing.lg) {
            Text(titleText)
                .font(.gvBody)
                .foregroundStyle(Color.gvTextPrimary)
            CalendarPickerMacOS(selection: $selected, components: .hourAndMinute)
                .padding(GvSpacing.md)
            HStack {
                Button("Cancel", action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button("Confirm") { onConfirm(selected) }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(GvSpacing.lg)
        .frame(minWidth: 320)
        #endif
    }
}
