internal import Combine
import SwiftUI

// Floating autocomplete for text attributes. The suggestion list can't live
// inside the entry card — its rounded clipShape clips it, and the completion
// checkbox draws over it. So the focused field publishes its bounds + matches
// up the view tree via a preference, and `LogView` (outside every card clip,
// above every entry) renders the list floating in place.

/// Published by the focused text field; read by the `LogView` overlay host.
struct AutocompleteRequest: Equatable {
    /// Stable per-field id ("entryId/attributeId") that matches a `PendingPick`
    /// back to the field which should apply it.
    let fieldKey: String
    let suggestions: [String]
    let anchor: Anchor<CGRect>
}

struct AutocompleteRequestKey: PreferenceKey {
    static let defaultValue: AutocompleteRequest? = nil
    // At most one field is focused, so keep the first non-nil contribution.
    static func reduce(value: inout AutocompleteRequest?, nextValue: () -> AutocompleteRequest?) {
        value = value ?? nextValue()
    }
}

/// Routes a suggestion tap (in the `LogView` overlay) back to the field that
/// owns it; the field applies it through its own `pick()` (text + blur +
/// commit). A value channel, not a closure, since closures can't ride a
/// preference and the field — not the overlay — owns the edit state.
@MainActor
final class AutocompleteCoordinator: ObservableObject {
    @Published var pendingPick: PendingPick?

    struct PendingPick: Equatable {
        let fieldKey: String
        let value: String
    }
}

/// The floating list of prior values, rendered by the `LogView` overlay at the
/// focused field's anchor.
struct AutocompleteSuggestionList: View {
    let suggestions: [String]
    let onPick: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.md) {
            ForEach(suggestions, id: \.self) { suggestion in
                Button {
                    onPick(suggestion)
                } label: {
                    Text(suggestion)
                        .font(.attrField)
                        .foregroundStyle(Color.entryTextPrimary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(GvSpacing.md)
        // One step lighter than the scalar entry background (gvNeutral900) so the
        // floating box reads as raised above the entries it covers.
        .background(Color.gvNeutral850)
        // Corner radius + border match the input pill (gvAttributePill uses 8).
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(Color.entryTextSecondary, lineWidth: 1)
        )
        // Lift it off the content it floats over.
        .shadow(color: .black.opacity(0.35), radius: 6, y: 2)
    }
}
