import SwiftUI
internal import Combine
import Foundation

final class AttributeFocusModel: ObservableObject {
    // Which attribute kind currently owns the keyboard (is first responder).
    // Drives the container-level iOS keyboard action bar. Set ONLY when a text
    // field gains/loses keyboard focus. nil when no attribute field has the
    // keyboard.
    @Published var keyboardKind: AttributeMenuKind?

    // The entry/attribute the focused field edits, set together with
    // `keyboardKind`. The keyboard action bar needs these to dispatch actions
    // (e.g. Remove), since it has no other handle on the focused attribute.
    @Published var focusedEntryId: String?
    @Published var focusedAttributeId: String?

    func focus(kind: AttributeMenuKind, entryId: String, attributeId: String) {
        keyboardKind = kind
        focusedEntryId = entryId
        focusedAttributeId = attributeId
    }

    func clear() {
        keyboardKind = nil
        focusedEntryId = nil
        focusedAttributeId = nil
    }
}
