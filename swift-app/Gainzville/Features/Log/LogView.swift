import SwiftUI
import UniformTypeIdentifiers

struct LogView: View {
    @EnvironmentObject var forestVM: ForestViewModel
    @EnvironmentObject var logDayStore: LogDayStore
    @EnvironmentObject var attributeFocus: AttributeFocusModel
    @EnvironmentObject var dragState: DragState
    @State private var isCreatePresented = false
    @State private var isDayDropTargeted = false
    @State private var pendingRootDrop: PendingRootDrop?

    var body: some View {
        let dayRoots = forestVM.rootsIn(logDay: logDayStore.logDay)
        Group {
            if dayRoots.isEmpty {
                ContentUnavailableView(
                    "No Entries",
                    systemImage: "list.bullet.rectangle",
                    description: Text("Entries you log will appear here.")
                )
            } else {
                ScrollView {
                    VStack(spacing: GvSpacing.lg) {
                        ForEach(dayRoots, id: \.id) { entry in
                            EntryView(entry: entry)
                        }
                    }
                    .padding(.horizontal, GvSpacing.lg)
                    .padding(.vertical, GvSpacing.lg)
                    .containerRelativeFrame(.horizontal)
                    // Tap-outside-to-clear: catches taps in horizontal padding,
                    // between entries, and below the last entry. Row taps win
                    // over this background because SwiftUI delivers to the
                    // deepest gesture.
                    .background(
                        Color.clear
                            .contentShape(Rectangle())
                            .onTapGesture { attributeFocus.focused = nil }
                    )
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        // gvBackground at the back, with a Color.primary tint layer above it
        // when a drag is hovering — auto-adapts (darken in light mode, lighten
        // in dark mode). Both sit below entry content via a single .background.
        .background(
            ZStack {
                Color.gvBackground
                Color.primary.opacity(isDayDropTargeted ? 0.05 : 0)
                    .animation(.easeInOut(duration: 0.12), value: isDayDropTargeted)
            }
            .allowsHitTesting(false)
        )
        .onDrop(of: [UTType.plainText], delegate: DayRootDropDelegate(
            dragState: dragState,
            isTargeted: $isDayDropTargeted,
            onDrop: { entry in
                pendingRootDrop = PendingRootDrop(entry: entry, day: logDayStore.logDay)
            }
        ))
        .sheet(item: $pendingRootDrop) { drop in
            RootDropTimePickerSheet(
                day: drop.day,
                initialTime: forestVM.suggestedRootInsertionTime(for: drop.day) ?? drop.day.start,
                onConfirm: { time in
                    forestVM.moveEntryToRoot(drop.entry, startTime: time)
                    pendingRootDrop = nil
                },
                onCancel: { pendingRootDrop = nil }
            )
        }
        .toolbar {
            ToolbarItem(placement: .principal) {
                LogDateHeader(logDay: $logDayStore.logDay)
            }
            ToolbarItem(placement: .primaryAction) {
                Button {
                    isCreatePresented = true
                } label: {
                    Image(systemName: "plus")
                }
            }
        }
        .sheet(isPresented: $isCreatePresented) {
            CreateEntrySheet(isPresented: $isCreatePresented) { activityId, name, isSequence in
                forestVM.createRootEntry(
                    activityId: activityId,
                    name: name,
                    isSequence: isSequence,
                    for: logDayStore.logDay
                )
                isCreatePresented = false
            }
        }
        .gvKeyboardDoneButton()
    }
}

// MARK: - Date header

private struct LogDateHeader: View {
    @Binding var logDay: LogDay
    @State private var isCalendarPresented = false

    private var dateBinding: Binding<Date> {
        Binding(
            get: { logDay.start },
            set: { logDay = .forLocalDate($0) }
        )
    }

    var body: some View {
        HStack(spacing: GvSpacing.xl) {
            Button {
                logDay = logDay.previous()
            } label: {
                Image(systemName: "chevron.left")
                    .foregroundStyle(Color.gvTextSecondary)
                    .padding(GvSpacing.md)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            Button { isCalendarPresented = true } label: {
                VStack(spacing: 1) {
                    Text(logDay.start.formatted(.dateTime.year()))
                        .font(.gvCaption)
                        .foregroundStyle(Color.gvTextSecondary)
                    Text(logDay.start.formatted(.dateTime.month(.abbreviated).day()))
                        .font(.gvBody)
                        .foregroundStyle(Color.gvTextPrimary)
                }
            }
            .buttonStyle(.plain)
            .platformPopover(isPresented: $isCalendarPresented) {
                LogCalendarPickerContent(date: dateBinding)
            }

            Button {
                logDay = logDay.next()
            } label: {
                Image(systemName: "chevron.right")
                    .foregroundStyle(Color.gvTextSecondary)
                    .padding(GvSpacing.md)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }
}

// MARK: - Calendar picker content

private struct LogCalendarPickerContent: View {
    @Binding var date: Date
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        #if os(iOS)
        NavigationStack {
            DatePicker("", selection: $date, displayedComponents: .date)
                .datePickerStyle(.graphical)
                .labelsHidden()
                .padding(GvSpacing.md)
                .navigationTitle("Select Date")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { dismiss() }
                    }
                }
        }
        .presentationDetents([.medium])
        #else
        CalendarPickerMacOS(selection: $date)
            .padding(GvSpacing.md)
        #endif
    }
}

#Preview {
    NavigationStack {
        LogView()
            .environmentObject(ForestViewModel())
            .environmentObject(LogDayStore())
            .environmentObject(DragState())
    }
}
