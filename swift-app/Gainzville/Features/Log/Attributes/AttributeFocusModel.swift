import SwiftUI
internal import Combine
import Foundation

struct AttributeFocusID: Hashable {
    let entryId: String
    let attributeId: String
    let subField: String?
    
    init(entryId: String, attributeId: String, subField: String? = nil) {
        self.entryId = entryId
        self.attributeId = attributeId
        self.subField = subField
    }
}

class AttributeFocusModel: ObservableObject {
    @Published var focusedId: AttributeFocusID?
}
