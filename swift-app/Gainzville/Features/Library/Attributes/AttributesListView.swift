import SwiftUI

struct AttributesListView: View {
    let attributes: [FfiAttribute]

    var body: some View {
        if attributes.isEmpty {
            ContentUnavailableView(
                "No Attributes",
                systemImage: "list.bullet",
                description: Text("Attributes you create will appear here.")
            )
        } else {
            List(attributes, id: \.id) { attribute in
                NavigationLink(value: LibraryDestination.attribute(attribute)) {
                    VStack(alignment: .leading, spacing: GvSpacing.sm) {
                        Text(attribute.name)
                            .font(.gvBody)
                        Text(attribute.config.typeName)
                            .font(.gvCaption)
                            .foregroundStyle(Color.gvTextSecondary)
                    }
                    .padding(.vertical, GvSpacing.sm)
                }
            }
            .listStyle(.plain)
        }
    }
}

private extension FfiAttributeConfig {
    var typeName: String {
        switch self {
        case .numeric:  return "Numeric"
        case .select:   return "Select"
        case .mass:     return "Mass"
        }
    }
}
