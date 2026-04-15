import SwiftUI

struct ActivitiesListView: View {
    let activities: [FfiActivity]

    var body: some View {
        if activities.isEmpty {
            ContentUnavailableView(
                "No Activities",
                systemImage: "figure.run",
                description: Text("Activities you create will appear here.")
            )
        } else {
            List(activities, id: \.id) { activity in
                NavigationLink(value: LibraryDestination.activity(activity)) {
                    VStack(alignment: .leading, spacing: GvSpacing.sm) {
                        Text(activity.name)
                            .font(.gvBody)
                        if let desc = activity.description {
                            Text(desc)
                                .font(.gvCaption)
                                .foregroundStyle(Color.gvTextSecondary)
                        }
                    }
                    .padding(.vertical, GvSpacing.sm)
                }
            }
            .listStyle(.plain)
        }
    }
}
