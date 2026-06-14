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
// The bar is pure presentation: it renders whatever `[AttributeBarAction]`
// list the host hands it. Which buttons a given attribute kind shows is
// decided by the variant builders below — hosts build their list once and
// feed it to every surface they own, so a kind looks the same on the keyboard
// bar, a sheet, or a popover.

/// One entry in the Units menu: a unit's display label, whether it's the one
/// currently shown, and the action selecting it. Kept as plain labels +
/// closures so the bar stays measure-agnostic (mass today, distance later).
struct UnitOption: Identifiable, Equatable {
    let label: String
    let isSelected: Bool
    let select: () -> Void

    var id: String { label }

    // Same equality contract as the bar actions: presentation only.
    static func == (a: Self, b: Self) -> Bool {
        a.label == b.label && a.isSelected == b.isSelected
    }
}

/// The bar's vocabulary: presentation (label/icon/color) lives here; behavior
/// arrives as associated closures. `units` renders as a menu of its options
/// rather than a plain button. `range` is a checkbox-style toggle between
/// exact and range editing; `active` is presentation state and participates
/// in `==` so the keyboard bar re-publishes when the mode flips mid-session.
enum AttributeBarAction: Identifiable {
    case clear(() -> Void)
    case units(options: [UnitOption])
    case range(active: Bool, toggle: () -> Void)
    case remove(() -> Void)

    var id: String { label }

    var label: String {
        switch self {
        case .clear: "Clear"
        case .units: "Units"
        case .range: "Range"
        case .remove: "Remove"
        }
    }

    var icon: String {
        switch self {
        case .clear: "x.square"
        case .units: "ruler"
        case .range(let active, _): active ? "checkmark.square" : "square"
        case .remove: "trash.fill"
        }
    }

    var color: Color {
        if case .remove = self { return Color.red }
        return Color.gvTextPrimary
    }

    func run() {
        switch self {
        case .clear(let f), .remove(let f): f()
        case .range(_, let toggle): toggle()
        case .units: break // Rendered as a Menu; selection runs per-option.
        }
    }
}

// Equality covers presentation state only — closures are deliberately excluded.
// Behavior stays fresh regardless (closures capture live @State storage); this
// == answers exactly one question: does the keyboard bar need re-publishing?
// (See AttributeBarPublisher in AttributeFocusModel.swift.)
extension AttributeBarAction: Equatable {
    static func == (a: Self, b: Self) -> Bool {
        switch (a, b) {
        case (.clear, .clear), (.remove, .remove):
            return true
        case (.units(let x), .units(let y)):
            return x == y
        case (.range(let x, _), .range(let y, _)):
            return x == y
        default:
            return false
        }
    }
}

// The per-kind matrix. Each signature is the policy: numeric/measure clear via
// backspace so they take no `clear`; picker kinds (select/temporal) have no
// keyboard, so they get an explicit Clear — supplied only when there's
// something to clear; temporal is intrinsic to the entry so it takes no
// `remove`.
extension Array where Element == AttributeBarAction {
    static func numeric(
        range: (active: Bool, toggle: () -> Void),
        remove: @escaping () -> Void
    ) -> Self {
        [.range(active: range.active, toggle: range.toggle), .remove(remove)]
    }

    // Shared by all measure types (mass, length): a Units menu, a Range toggle,
    // and Remove. The unit options are unit-type-agnostic (`UnitOption`), so one
    // factory serves every measure.
    static func measure(
        units: [UnitOption],
        range: (active: Bool, toggle: () -> Void),
        remove: @escaping () -> Void
    ) -> Self {
        [
            .units(options: units),
            .range(active: range.active, toggle: range.toggle),
            .remove(remove),
        ]
    }

    // Text clears via backspace (no keyboard-less picker), so like numeric it
    // takes no `clear` — just Remove.
    static func text(remove: @escaping () -> Void) -> Self {
        [.remove(remove)]
    }

    // `range` is supplied only when the attribute is `ordered` — unordered
    // selects have no range values (core rejects them).
    static func select(
        clear: (() -> Void)?,
        range: (active: Bool, toggle: () -> Void)?,
        remove: @escaping () -> Void
    ) -> Self {
        (clear.map { [.clear($0)] } ?? [])
            + (range.map { [.range(active: $0.active, toggle: $0.toggle)] } ?? [])
            + [.remove(remove)]
    }

    static func temporal(clear: (() -> Void)?) -> Self {
        clear.map { [.clear($0)] } ?? []
    }
}

struct AttributeActionBar: View {
    /// The focused attribute's actions, built by the host via the
    /// `[AttributeBarAction]` variants. Empty means no attribute is focused
    /// (e.g. a plain text field elsewhere in the same container) — only the
    /// dismiss affordance shows, preserving the old keyboard-Done behavior.
    let actions: [AttributeBarAction]
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
                        if case .units(let options) = action {
                            // A system Menu gives the unit list anchored
                            // presentation + dismissal for free on both the
                            // iOS keyboard bar and the macOS popover.
                            Menu {
                                ForEach(options) { option in
                                    Button(action: option.select) {
                                        if option.isSelected {
                                            Label(option.label, systemImage: "checkmark")
                                        } else {
                                            Text(option.label)
                                        }
                                    }
                                }
                            } label: {
                                Label(action.label, systemImage: action.icon)
                                    .font(.attrLabel)
                                    .foregroundStyle(action.color)
                            }
                            .menuIndicator(.hidden)
                            .buttonStyle(.plain)
                        } else {
                            Button(action: action.run) {
                                Label(action.label, systemImage: action.icon)
                                    .font(.attrLabel)
                                    .foregroundStyle(action.color)
                            }
                            .buttonStyle(.plain)
                        }
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
}

// Sheet presentation of the action bar: a large title with the bar framed by
// thin separators above and below. The keyboard surface uses AttributeActionBar
// directly (no title or separators).
struct AttributeSheetBar: View {
    let title: String
    let actions: [AttributeBarAction]
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
                AttributeActionBar(actions: actions)
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

    func body(content: Content) -> some View {
        #if os(iOS)
        content.toolbar {
            ToolbarItemGroup(placement: .keyboard) {
                AttributeActionBar(actions: focusModel.actions ?? []) {
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
}
