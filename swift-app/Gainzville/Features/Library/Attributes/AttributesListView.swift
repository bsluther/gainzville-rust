import SwiftUI

struct AttributesListView: View {
    let attributes: [Attribute]
    @EnvironmentObject var attributesVM: AttributesViewModel
    @State private var showingCreate = false

    var body: some View {
        Group {
            if attributes.isEmpty {
                ContentUnavailableView(
                    "No Attributes",
                    systemImage: "list.bullet",
                    description: Text("Tap + to create your first attribute.")
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
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    showingCreate = true
                } label: {
                    Image(systemName: "plus")
                }
            }
        }
        .sheet(isPresented: $showingCreate) {
            CreateAttributeView { name, description, config in
                attributesVM.createAttribute(name: name, description: description, config: config)
            }
        }
    }
}

private extension AttributeConfig {
    var typeName: String {
        switch self {
        case .numeric:     return "Numeric"
        case .select:      return "Select"
        case .multiselect: return "Multiselect"
        case .mass:        return "Mass"
        case .length:      return "Length"
        case .text:        return "Text"
        }
    }
}
