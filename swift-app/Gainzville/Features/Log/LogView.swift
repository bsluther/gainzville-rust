import SwiftUI

struct LogView: View {
    @EnvironmentObject var forestVM: ForestViewModel
    @EnvironmentObject var logDayStore: LogDayStore
    @State private var isCreatePresented = false

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
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Color.gvBackground)
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
    }
}
