import SwiftUI

// Shared layout for attribute-style rows: label on the left, custom input on the
// right, consistent min-height across temporal and attribute editors. The row
// owns its focus state — pass the focus identifier and the row reads/writes the
// shared AttributeFocusModel directly.
struct AttributeRow<Content: View>: View {
    let label: String
    let focus: AttributeFocus
    let indent: CGFloat
    private let content: Content
    @EnvironmentObject private var focusModel: AttributeFocusModel

    init(label: String, focus: AttributeFocus, indent: CGFloat = 0, @ViewBuilder content: () -> Content) {
        self.label = label
        self.focus = focus
        self.indent = indent
        self.content = content()
    }

    private var isFocused: Bool { focusModel.focused == focus }

    var body: some View {
        HStack(alignment: .center) {
            Text(label)
                .font(.attrLabel)
                .foregroundStyle(Color.entryTextSecondary)
                .padding(.leading, indent)

            // Reserved 20×20 slot — gear is always laid out, only its opacity
            // toggles, so showing/hiding doesn't shift the row.
            Image(systemName: "gearshape")
                .foregroundStyle(Color.gvTextSecondary)
                .opacity(isFocused ? 1 : 0)
                .frame(width: 20, height: 20)
                .padding(.leading, GvSpacing.sm)

            Spacer()
            HStack(spacing: GvSpacing.lg) {
                content
            }
        }
        .frame(minHeight: GvSpacing.minAttributeHeight)
        .contentShape(Rectangle())
        .onTapGesture {
            focusModel.focused = focus
        }
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

// Identifiable + name accessor so SwiftUI can ForEach over `[FfiAttributePair]`
// without per-call-site switch boilerplate.
extension FfiAttributePair: Identifiable {
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
