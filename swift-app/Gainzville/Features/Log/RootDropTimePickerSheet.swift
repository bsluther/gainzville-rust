import SwiftUI
#if os(macOS)
import AppKit
#endif

/// A pending drop onto the day's root, awaiting the user's choice of start time.
struct PendingRootDrop: Identifiable {
    let id = UUID()
    let entry: Entry
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
            // fixedSize so the field hugs its intrinsic width and centers in the
            // sheet, rather than stretching wide and stranding the steppers far
            // from the digits.
            TimeStepperFieldMacOS(selection: $selected)
                .fixedSize()
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
        .tint(.gvBlue500)
        #endif
    }
}

#if os(macOS)
/// macOS time entry as an editable text field with up/down steppers
/// (NSDatePicker's `.textFieldAndStepper`) — the standard macOS idiom for
/// choosing a time, and less fiddly than dragging the analog clock hand.
/// Hour+minute only (no seconds), matching the iOS `.hourAndMinute` wheel.
private struct TimeStepperFieldMacOS: NSViewRepresentable {
    @Binding var selection: Date

    func makeNSView(context: Context) -> NSDatePicker {
        let picker = NSDatePicker()
        picker.datePickerStyle = .textFieldAndStepper
        picker.datePickerElements = .hourMinute
        picker.setContentHuggingPriority(.required, for: .horizontal)
        picker.dateValue = selection
        picker.target = context.coordinator
        picker.action = #selector(Coordinator.dateChanged(_:))
        return picker
    }

    func updateNSView(_ picker: NSDatePicker, context: Context) {
        if picker.dateValue != selection { picker.dateValue = selection }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject {
        var parent: TimeStepperFieldMacOS
        init(_ parent: TimeStepperFieldMacOS) { self.parent = parent }
        @objc func dateChanged(_ sender: NSDatePicker) { parent.selection = sender.dateValue }
    }
}
#endif
