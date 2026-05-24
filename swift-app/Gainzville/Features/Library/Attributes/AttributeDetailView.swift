import SwiftUI

struct AttributeDetailView: View {
    let attribute: Attribute

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: GvSpacing.xl) {
                GvDetailSection(title: "Name", actionIcon: "pencil", onAction: {}) {
                    Text(attribute.name)
                        .font(.gvBody)
                        .foregroundStyle(Color.gvTextPrimary)
                }

                GvDetailSection(title: "Description", actionIcon: "pencil", onAction: {}) {
                    Text(attribute.description ?? "No description")
                        .font(.gvBody)
                        .foregroundStyle(attribute.description != nil ? Color.gvTextPrimary : Color.gvTextSecondary)
                }

                GvDetailSection(title: "Type") {
                    Text(attribute.config.typeName)
                        .font(.gvBody)
                        .foregroundStyle(Color.gvTextPrimary)
                }

                GvDetailSection(title: "Config") {
                    Text("Coming soon")
                        .font(.gvBody)
                        .foregroundStyle(Color.gvTextSecondary)
                }
            }
            .padding(GvSpacing.xl)
        }
        .background(Color.gvBackground)
        .navigationTitle(attribute.name)
    }
}

private extension AttributeConfig {
    var typeName: String {
        switch self {
        case .numeric:  return "Numeric"
        case .select:   return "Select"
        case .mass:     return "Mass"
        }
    }
}
