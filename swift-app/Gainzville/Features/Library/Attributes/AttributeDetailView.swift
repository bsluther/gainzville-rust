import SwiftUI

struct AttributeDetailView: View {
    let attribute: FfiAttribute

    var body: some View {
        List {
            Section("Type") {
                LabeledContent("Kind", value: attribute.config.typeName)
            }

            Section("Info") {
                LabeledContent("ID", value: attribute.id)
                LabeledContent("Owner", value: attribute.ownerId)
            }
        }
        .navigationTitle(attribute.name)
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
