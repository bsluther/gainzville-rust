import SwiftUI

struct AttributesListView: View {
    let attributes: [Attribute]

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
                        if let desc = attribute.description {
                            Text(desc)
                                .font(.gvCaption)
                                .foregroundStyle(Color.gvTextSecondary)
                        }
                        Text(attribute.config.typeName)
                            .font(.gvCaption)
                            .foregroundStyle(Color.gvTextSecondary)
                    }
                    .padding(.vertical, GvSpacing.sm)
                }
                .listRowBackground(Color.gvBackground)
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .background(Color.gvBackground)
        }
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
