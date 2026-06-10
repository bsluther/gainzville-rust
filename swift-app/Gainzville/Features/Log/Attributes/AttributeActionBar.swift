import SwiftUI

// A single horizontal "action bar" surfacing per-attribute controls in a
// consistent place across contexts: above the keyboard for keyboard inputs
// (Numeric, Mass) and at the top of picker sheets (Select, Temporal).
//
// Layout: a horizontally scrolling row of text buttons on the leading side, a
// pinned dismiss affordance on the trailing side that never scrolls. The
// buttons read better as words than icons here — the controls (Remove
// attribute, Pick units, Range) aren't part of an established symbolic
// language, and there are few enough that text fits.
//
// SPIKE: every button except dismiss is a no-op. This exists to evaluate the
// mechanism (scroll row + pinned dismiss inside a `.keyboard` toolbar / sheet
// header) and the cross-context UX before any actions are wired. See GV-36.
struct AttributeActionBar: View {
    /// The focused attribute's kind. `nil` means no attribute is focused (e.g.
    /// a plain text field elsewhere in the same container) — only the dismiss
    /// affordance shows, preserving the old keyboard-Done behavior.
    var kind: AttributeMenuKind?
    /// Empty the focused field/value while keeping the attribute attached. When
    /// nil, the Clear button is hidden — the host supplies it only when clearing
    /// is meaningful and allowed (e.g. temporal Clear is withheld when it would
    /// leave a root entry with no start/end).
    var onClear: (() -> Void)? = nil
    /// Detach the attribute from the entry. When nil, the Remove button is hidden.
    var onRemove: (() -> Void)? = nil
    /// Resign first responder. Supplied only by the iOS keyboard bar, where
    /// there's no title row to host a close button; when set, a trailing dismiss
    /// affordance is shown. Sheet / popover presentations put the dismiss in
    /// their header toolbar instead (see AttributeSheetBar) and leave this nil.
    var onDismiss: (() -> Void)? = nil

    var body: some View {
        HStack(spacing: GvSpacing.md) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: GvSpacing.xl) {
                    ForEach(actions) { action in
                        Button(action: action.run) {
                            Label(action.label, systemImage: action.icon)
                                .font(.attrLabel)
                                .foregroundStyle(action.color)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, GvSpacing.sm)
                .frame(maxHeight: .infinity)
            }

            if let onDismiss {
                Button(action: onDismiss) {
                    Image(systemName: "keyboard.chevron.compact.down")
                        .foregroundStyle(Color.gvLoggedBlue)
                        .frame(width: 32, height: 32)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        // Fixed height keeps the horizontal ScrollView from going vertically
        // greedy (which collapses the bar inside a free VStack, e.g. a sheet).
        .frame(maxWidth: .infinity)
        .frame(height: 36)
        .padding(.horizontal, GvSpacing.lg)
        .padding(.vertical, GvSpacing.sm)
    }

    // The buttons available for the focused attribute's kind. Clear and Remove
    // are wired via the host-supplied closures and only appear when supplied;
    // Units and Range remain no-op placeholders (their backing features —
    // unit selection and exact/range switching — aren't built yet).
    private var actions: [Action] {
        guard let kind else { return [] }
        var result: [Action] = []
        // Picker-based kinds have no keyboard, so backspace can't clear them —
        // they get an explicit Clear. Keyboard kinds (numeric/mass) clear via
        // backspace and so don't.
        if (kind == .temporal || kind == .select), let onClear {
            result.append(Action(label: "Clear", icon: "x.square", run: onClear))
        }
        if kind == .mass {
            result.append(Action(label: "Units", icon: "ruler"))
        }
        // TODO: select should only have a Range action if the attribute is `ordered`.
        if kind == .numeric || kind == .mass || kind == .select {
            result.append(Action(label: "Range", icon: "arrow.left.and.right.square"))
        }
        if kind != .temporal, let onRemove {
            result.append(Action(label: "Remove", icon: "trash.fill", color: Color.red, run: onRemove))
        }
        return result
    }

    private struct Action: Identifiable {
        let label: String
        let icon: String
        var color: Color = Color.gvTextPrimary
        var run: () -> Void = {}
        var id: String { label }
    }
}

// Sheet presentation of the action bar: a large title with the bar framed by
// thin separators above and below. The keyboard surface uses AttributeActionBar
// directly (no title or separators).
struct AttributeSheetBar: View {
    let title: String
    let kind: AttributeMenuKind?
    var onClear: (() -> Void)? = nil
    var onRemove: (() -> Void)? = nil
    let onDismiss: () -> Void

    var body: some View {
        VStack(spacing: GvSpacing.md) {
            VStack(spacing: GvSpacing.md) {
                // Header toolbar: centered title with a trailing close button —
                // the close lives here (top-right) rather than inline in the bar.
                ZStack {
                    Text(title)
                        .font(.gvHeadline)
                        .foregroundStyle(Color.gvTextPrimary)
                    HStack {
                        Spacer()
                        Button(action: onDismiss) {
                            Image(systemName: "xmark")
                                .font(.title2)
                                .foregroundStyle(Color.gvTextPrimary)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        #if os(iOS)
                        .padding(.trailing, GvSpacing.lg)
                        #endif
                    }
                }
                #if os(macOS)
                .padding(.top, GvSpacing.md)
                #else
                .padding(.top, GvSpacing.xl)
                #endif
                .padding(.horizontal, GvSpacing.lg)

                // Actions only; the close button is in the toolbar above, so no
                // inline dismiss affordance.
                AttributeActionBar(kind: kind, onClear: onClear, onRemove: onRemove)
            }
            .frame(maxWidth: .infinity)
            GvMenuDivider()
                .padding(.bottom, GvSpacing.md)
        }
    }
}

// Attaches the action bar to the keyboard accessory area, context-aware via the
// shared focus model. A ViewModifier (not a plain `func`) so it can read the
// EnvironmentObject; apply once at the container level (LogView, LibraryView).
struct AttributeKeyboardBar: ViewModifier {
    @EnvironmentObject private var focusModel: AttributeFocusModel
    @EnvironmentObject private var forestVM: ForestViewModel

    func body(content: Content) -> some View {
        #if os(iOS)
        content.toolbar {
            ToolbarItemGroup(placement: .keyboard) {
                AttributeActionBar(
                    kind: focusModel.keyboardKind,
                    onRemove: keyboardRemove
                ) {
                    UIApplication.shared.sendAction(
                        #selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
                }
                .glassEffect(.clear, in: .capsule)
                .overlay(Capsule().strokeBorder(.white.opacity(0.12), lineWidth: 0.5))
                .padding(.bottom, GvSpacing.lg)
            }
            // Hide the default toolbar background so we can pad away from the keyboard.
            .sharedBackgroundVisibility(.hidden)
        }
        #else
        content
        #endif
    }

    #if os(iOS)
    // Remove the attribute whose field currently owns the keyboard, then dismiss
    // it (the field disappears with the attribute). nil — hiding Remove — when no
    // attribute field is focused.
    private var keyboardRemove: (() -> Void)? {
        guard let entryId = focusModel.focusedEntryId,
              let attributeId = focusModel.focusedAttributeId else { return nil }
        return {
            forestVM.removeAttribute(entryId: entryId, attributeId: attributeId)
            UIApplication.shared.sendAction(
                #selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
        }
    }
    #endif
}
