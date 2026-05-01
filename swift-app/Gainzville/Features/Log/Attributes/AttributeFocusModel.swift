import SwiftUI
internal import Combine
import Foundation

// Identifies which attribute row currently owns the focused-state UI affordance
// (the gear icon, future per-attribute menu). Standard rows focus as a unit;
// the temporal "attribute" focuses at the sub-field level.
enum AttributeFocus: Hashable {
    case standard(entryId: String, attrId: String)
    case temporalStart(entryId: String)
    case temporalEnd(entryId: String)
    case temporalDuration(entryId: String)
}

final class AttributeFocusModel: ObservableObject {
    @Published var focused: AttributeFocus?
}
