import SwiftUI
internal import Combine
import Foundation

final class AttributeFocusModel: ObservableObject {
    // The focused attribute field's bar actions, built by the owning view via
    // the `[AttributeBarAction]` variants. Drives the container-level iOS
    // keyboard action bar. Set ONLY when a text field gains/loses keyboard
    // focus. nil when no attribute field has the keyboard.
    //
    // NOTE: the list is captured at focus time. If an action list ever needs
    // to change mid-edit (e.g. a Clear that appears once a value is typed),
    // the keyboard bar won't see it unless the owning view re-calls
    // `focus(actions:)`. No current list changes mid-edit (numeric/mass are
    // static; select/temporal don't use the keyboard surface), but this needs
    // resolution if that changes.
    @Published private(set) var actions: [AttributeBarAction]?

    // Which field owns the bar. When focus jumps directly between two
    // attribute fields, SwiftUI doesn't order the two onChange handlers — the
    // old field's clear can land after the new field's focus and empty the bar
    // mid-edit. Owner-checking clear() makes the stale call a no-op.
    private var owner: String?

    func focus(_ owner: String, actions: [AttributeBarAction]) {
        self.owner = owner
        self.actions = actions
    }

    func clear(_ owner: String) {
        guard self.owner == owner else { return }
        self.owner = nil
        actions = nil
    }
}
