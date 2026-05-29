import SwiftUI
internal import Combine
import Foundation

final class AttributeFocusModel: ObservableObject {
    // Which attribute kind currently owns the keyboard (is first responder).
    // Drives the container-level iOS keyboard action bar. Set ONLY when a text
    // field gains/loses keyboard focus. nil when no attribute field has the
    // keyboard.
    @Published var keyboardKind: AttributeMenuKind?
}
