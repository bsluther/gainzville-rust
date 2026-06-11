import SwiftUI
internal import Combine
import Foundation

final class AttributeFocusModel: ObservableObject {
    // The focused attribute field's bar actions, built by the owning view via
    // the `[AttributeBarAction]` variants. Drives the container-level iOS
    // keyboard action bar. nil when no attribute field has the keyboard.
    //
    // The list is a snapshot; mid-session changes to an action's presentation
    // state (e.g. the Range checkbox) reach the bar via `update()`. Hosts
    // shouldn't call focus/update/clear directly — apply
    // `.attributeBarActions(token:isFocused:actions:)` (below), which owns the
    // conversation and re-publishes when the actions' value-state changes.
    @Published private(set) var actions: [AttributeBarAction]?

    // Which field owns the bar. When focus jumps directly between two
    // attribute fields, SwiftUI doesn't order the two onChange handlers — the
    // old field's clear can land after the new field's focus and empty the bar
    // mid-edit. Owner-checking clear() and update() makes stale calls no-ops.
    private var owner: String?

    func focus(_ owner: String, actions: [AttributeBarAction]) {
        self.owner = owner
        self.actions = actions
    }

    func update(_ owner: String, actions: [AttributeBarAction]) {
        guard self.owner == owner else { return }
        self.actions = actions
    }

    func clear(_ owner: String) {
        guard self.owner == owner else { return }
        self.owner = nil
        actions = nil
    }
}

// Declares "while focused, the keyboard bar shows these actions". The modifier
// owns the focus-model conversation: publish on focus, re-publish when the
// actions' value-state changes (the closure-blind ==), clear on blur.
//
// Re-publication needs no manual dependency bookkeeping: `actions` is
// recomputed on every body evaluation of the host, so any state feeding it
// arrives here as a new value to diff — the same render-time reconstruction
// that keeps closure props fresh in a parent/child relationship, re-established
// across the focus-model channel.
struct AttributeBarPublisher: ViewModifier {
    let token: String
    let isFocused: Bool
    let actions: [AttributeBarAction]
    @EnvironmentObject private var focusModel: AttributeFocusModel

    func body(content: Content) -> some View {
        content
            .onChange(of: isFocused) { _, focused in
                if focused {
                    focusModel.focus(token, actions: actions)
                } else {
                    focusModel.clear(token)
                }
            }
            .onChange(of: actions) { _, new in
                if isFocused { focusModel.update(token, actions: new) }
            }
    }
}

extension View {
    func attributeBarActions(
        token: String, isFocused: Bool, actions: [AttributeBarAction]
    ) -> some View {
        modifier(AttributeBarPublisher(token: token, isFocused: isFocused, actions: actions))
    }
}
