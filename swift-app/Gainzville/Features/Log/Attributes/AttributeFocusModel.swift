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
    // Tap-focus: which row shows the focused-state affordance (gear). Set by
    // taps on any row; cleared by tap-outside.
    @Published var focused: AttributeFocus?

    // Which attribute kind currently owns the keyboard (is first responder).
    // Drives the container-level keyboard action bar. Set ONLY when a text
    // field gains/loses keyboard focus — NOT by taps — so tapping another
    // attribute's label while editing doesn't swap the bar out from under you.
    // nil when no attribute field has the keyboard.
    @Published var keyboardKind: AttributeMenuKind?
}
