import SwiftUI

struct ActivityDetailView: View {
    let activity: FfiActivity

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: GvSpacing.xl) {
                GvDetailSection(title: "Name", actionIcon: "pencil", onAction: {}) {
                    Text(activity.name)
                        .font(.gvBody)
                        .foregroundStyle(Color.gvTextPrimary)
                }

                GvDetailSection(title: "Description", actionIcon: "pencil", onAction: {}) {
                    Text(activity.description ?? "No description")
                        .font(.gvBody)
                        .foregroundStyle(activity.description != nil ? Color.gvTextPrimary : Color.gvTextSecondary)
                }

                GvDetailSection(title: "Recent") {
                    Text("Coming soon")
                        .font(.gvBody)
                        .foregroundStyle(Color.gvTextSecondary)
                }

                GvDetailSection(title: "Categories") {
                    Text("Coming soon")
                        .font(.gvBody)
                        .foregroundStyle(Color.gvTextSecondary)
                }

                GvDetailSection(title: "Sub-Categories") {
                    Text("Coming soon")
                        .font(.gvBody)
                        .foregroundStyle(Color.gvTextSecondary)
                }

                GvDetailSection(title: "Attributes") {
                    Text("Coming soon")
                        .font(.gvBody)
                        .foregroundStyle(Color.gvTextSecondary)
                }
            }
            .padding(GvSpacing.xl)
        }
        .background(Color.gvBackground)
        .navigationTitle(activity.name)
    }
}
