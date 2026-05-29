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
    /// Right-pinned affordance: resignFirstResponder above the keyboard,
    /// `dismiss()` inside a sheet — same role, "dismiss keyboard ≡ close sheet".
    /// Uses the keyboard-dismiss glyph in both contexts; it's an imperfect
    /// symbol for the sheet case but kept identical for now (GV-36).
    let onDismiss: () -> Void

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

            Button(action: onDismiss) {
                Image(systemName: "keyboard.chevron.compact.down")
                    .foregroundStyle(Color.gvLoggedBlue)
                    .frame(width: 32, height: 32)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
        // Fixed height keeps the horizontal ScrollView from going vertically
        // greedy (which collapses the bar inside a free VStack, e.g. a sheet).
        .frame(maxWidth: .infinity)
        .frame(height: 36)
        .padding(.horizontal, GvSpacing.lg)
        .padding(.vertical, GvSpacing.sm)
    }

    // The buttons available for the focused attribute's kind. No-ops for the
    // spike. Mirrors the conditions in AttributeMenuContent (AttributeRow.swift).
    private var actions: [Action] {
        guard let kind else { return [] }
        var result: [Action] = []
        // Picker-based kinds have no keyboard, so backspace can't clear them —
        // they get an explicit Clear. Keyboard kinds (numeric/mass) clear via
        // backspace and so don't.
        if kind == .temporal || kind == .select {
            result.append(Action(label: "Clear", icon: "x.square"))
        }
        if kind == .mass {
            result.append(Action(label: "Units", icon: "ruler"))
        }
        // TODO: select should only have a Range action if the attribute is `ordered`.
        if kind == .numeric || kind == .mass || kind == .select {
            result.append(Action(label: "Range", icon: "arrow.left.and.right.square"))
        }
        if kind != .temporal {
            // Alt icon to try: "rectangle.badge.minus", "trash.fill"
            result.append(Action(label: "Remove", icon: "trash.fill", color: Color.red))
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
    let onDismiss: () -> Void

    var body: some View {
        VStack(spacing: GvSpacing.md) {
            // The whole header band (title + bar) is one neutral step lighter
            // than the sheet body (gvSurface over gvBackground), with a bottom
            // border, so it reads as a distinct header above the picker content.
            VStack(spacing: GvSpacing.md) {
                Text(title)
                    .font(.gvHeadline)
                    .foregroundStyle(Color.gvTextPrimary)
                    .padding(.top, GvSpacing.xl)
                AttributeActionBar(kind: kind, onDismiss: onDismiss)
            }
            .frame(maxWidth: .infinity)
//            .padding(.vertical, GvSpacing.lg)
//            .background(Color.gvSurface)
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

    func body(content: Content) -> some View {
        #if os(iOS)
        content.toolbar {
            ToolbarItemGroup(placement: .keyboard) {
                AttributeActionBar(kind: focusModel.keyboardKind) {
                    UIApplication.shared.sendAction(
                        #selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
                }
//                .background(.ultraThinMaterial, in: .capsule)
//                .containerRelativeFrame(.horizontal)
//                .frame(maxWidth: .infinity)
                .glassEffect(.clear, in: .capsule)
                .overlay(
                          Capsule().strokeBorder(.white.opacity(0.12), lineWidth: 0.5)
                      )
                .padding(.bottom, GvSpacing.lg)
                
            }
            // Hide the default background of the toolbar so can we pad away from the keyboard.
            .sharedBackgroundVisibility(.hidden)
            
        }
        #else
        content
        #endif
    }
}
