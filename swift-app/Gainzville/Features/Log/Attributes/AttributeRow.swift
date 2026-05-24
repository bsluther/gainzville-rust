import SwiftUI

// Shared layout for attribute-style rows: label on the left, custom input on the
// right, consistent min-height across temporal and attribute editors. The row
// owns its focus state — pass the focus identifier and the row reads/writes the
// shared AttributeFocusModel directly.
enum AttributeMenuKind {
    case numeric
    case mass
    case select
    case temporal
}

struct AttributeRow<Content: View>: View {
    let label: String
    let focus: AttributeFocus
    let kind: AttributeMenuKind
    let indent: CGFloat
    private let content: Content
    @EnvironmentObject private var focusModel: AttributeFocusModel
    @State private var isMenuPresented = false

    init(label: String, focus: AttributeFocus, kind: AttributeMenuKind, indent: CGFloat = 0, @ViewBuilder content: () -> Content) {
        self.label = label
        self.focus = focus
        self.kind = kind
        self.indent = indent
        self.content = content()
    }

    private var isFocused: Bool { focusModel.focused == focus }

    var body: some View {
        // Top alignment so that when value pills wrap to multiple lines the
        // label/gear group stays anchored at the first row.
        HStack(alignment: .top, spacing: 0) {
            // Label + gear, sized to content, on the left.
            HStack(alignment: .center, spacing: 0) {
                Text(label)
                    .font(.attrLabel)
                    .foregroundStyle(Color.entryTextSecondary)
                    .padding(.leading, indent)

                // Reserved 20×20 slot — gear is always laid out, only its opacity
                // toggles, so showing/hiding doesn't shift the row.
                Button {
                    focusModel.focused = focus
                    isMenuPresented = true
                } label: {
                    Image(systemName: "gearshape")
                        .foregroundStyle(Color.gvTextSecondary)
                        .opacity(isFocused ? 1 : 0)
                        .frame(width: 20, height: 20)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .padding(.leading, GvSpacing.sm)
                .platformPopover(isPresented: $isMenuPresented) {
                    AttributeMenuContent(kind: kind)
                }
            }
            .frame(minHeight: GvSpacing.minAttributeHeight)

            // Value pills lay out in a row; if they don't fit the remaining
            // width, fall back to stacking vertically (right-aligned) rather
            // than wrapping mid-row. ViewThatFits measures each candidate
            // against the proposed width and picks the first that fits — this is
            // also what stops fixed-size pills from inflating the entry. The
            // maxWidth:.infinity frame claims the leftover horizontal space and
            // right-pushes the content, mirroring the prior Spacer-pushed look.
            ViewThatFits(in: .horizontal) {
                HStack(spacing: GvSpacing.lg) { content }
                VStack(alignment: .trailing, spacing: GvSpacing.lg) { content }
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .frame(minHeight: GvSpacing.minAttributeHeight)
        .contentShape(Rectangle())
        .onTapGesture {
            focusModel.focused = focus
        }
    }
}

private struct AttributeMenuContent: View {
    let kind: AttributeMenuKind

    var body: some View {
        ScrollView {
            VStack(spacing: GvSpacing.md) {
                GvMenuRow("Clear", icon: "xmark.circle")

                if kind != .temporal {
                    GvMenuDivider()
                    GvMenuRow("Remove attribute", icon: "trash", isDestructive: true)
                }

                if kind == .mass {
                    GvMenuDivider()
                    GvMenuRow("Pick units", icon: "ruler")
                }
            }
            .padding(GvSpacing.md)
        }
        #if os(iOS)
        .presentationDetents([.medium])
        #endif
    }
}

// Shared pill style for attribute value display across temporal and attribute
// editors. Apply with `.gvAttributePill()`.
extension View {
    func gvAttributePill(borderColor: Color = .entryTextSecondary) -> some View {
        self
            .font(.attrField)
            .foregroundStyle(Color.entryTextPrimary)
            .padding(.horizontal, GvSpacing.lg)
            .padding(.vertical, GvSpacing.sm)
            .frame(minHeight: GvSpacing.minAttributeHeight)
            .background(.clear)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(borderColor, lineWidth: 1)
            )
    }

    /// Adds a global keyboard toolbar with a "Done" button to dismiss the keyboard.
    @ViewBuilder
    func gvKeyboardDoneButton() -> some View {
        #if os(iOS)
        self.toolbar {
            ToolbarItemGroup(placement: .keyboard) {
                Spacer()
                Button {
                    UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
                } label: {
                    Image(systemName: "keyboard.chevron.compact.down")
                }
            }
        }
        #else
        self
        #endif
    }

    /// Selects all text when the bound focus state becomes true.
    @ViewBuilder
    func gvSelectAllOnFocus(isFocused: Bool) -> some View {
        #if os(iOS)
        self.onChange(of: isFocused) { _, focused in
            if focused {
                DispatchQueue.main.async {
                    UIApplication.shared.sendAction(#selector(UIResponder.selectAll(_:)), to: nil, from: nil, for: nil)
                }
            }
        }
        #else
        self
        #endif
    }
}

// Fixed-width whitespace placeholder so empty pills have a consistent minimum size.
let gvEmptyPillText = "\u{00a0}\u{00a0}\u{00a0}\u{00a0}\u{00a0}"

// Identifiable + name accessor so SwiftUI can ForEach over `[AttributePair]`
// without per-call-site switch boilerplate.
extension AttributePair: Identifiable {
    public var id: String {
        switch self {
        case .numeric(let p): return p.attrId
        case .select(let p):  return p.attrId
        case .mass(let p):    return p.attrId
        }
    }

    public var name: String {
        switch self {
        case .numeric(let p): return p.name
        case .select(let p):  return p.name
        case .mass(let p):    return p.name
        }
    }
}
