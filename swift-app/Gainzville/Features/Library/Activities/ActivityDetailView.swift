import SwiftUI

struct ActivityDetailView: View {
    let activity: FfiActivity

    var body: some View {
        List {
            Section {
                if let desc = activity.description {
                    Text(desc)
                        .foregroundStyle(Color.gvTextSecondary)
                }
            }

            Section("Info") {
                LabeledContent("ID", value: activity.id)
                LabeledContent("Owner", value: activity.ownerId)
                if let sourceId = activity.sourceActivityId {
                    LabeledContent("Source activity", value: sourceId)
                }
            }
        }
        .navigationTitle(activity.name)
    }
}
