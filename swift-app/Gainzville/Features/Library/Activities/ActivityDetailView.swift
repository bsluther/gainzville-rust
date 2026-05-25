import SwiftUI

struct ActivityDetailView: View {
    let activity: Activity

    @EnvironmentObject private var forestVM: ForestViewModel

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

                // The template is edited with the same EntryView used in the log
                // (duration-only temporal, no completion). Attaching/detaching
                // attribute values here configures the activity's defaults.
                GvDetailSection(title: "Template") {
                    if let root = forestVM.templateRoot(activityId: activity.id) {
                        VStack(alignment: .leading, spacing: GvSpacing.md) {
                            EntryView(entry: root)
                                .environment(\.entryContext, .template)
                            // Toggle the template root between a single (scalar)
                            // entry and a sequence of child entries. Switching to
                            // scalar deletes any children.
                            HStack {
                                Text("Sequence")
                                    .font(.gvBody)
                                    .foregroundStyle(Color.gvTextSecondary)
                                Spacer()
                                GvCheckbox(checked: root.isSequence) {
                                    forestVM.setIsSequence(entryId: root.id, isSequence: !root.isSequence)
                                }
                            }
                        }
                    } else {
                        Text("No template")
                            .font(.gvBody)
                            .foregroundStyle(Color.gvTextSecondary)
                    }
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
            }
            .padding(GvSpacing.xl)
        }
        .background(Color.gvBackground)
        .navigationTitle(activity.name)
    }
}
