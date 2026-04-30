import SwiftUI

// Shared layout for attribute-style rows: label on the left, custom input on the
// right, consistent min-height across temporal and attribute editors.
struct AttributeRow<Content: View>: View {
    let label: String
    let indent: CGFloat
    private let content: Content

    init(label: String, indent: CGFloat = 0, @ViewBuilder content: () -> Content) {
        self.label = label
        self.indent = indent
        self.content = content()
    }

    var body: some View {
        HStack(alignment: .center) {
            Text(label)
                .font(.attrLabel)
                .foregroundStyle(Color.entryTextSecondary)
                .padding(.leading, indent)
            Spacer()
            HStack(spacing: GvSpacing.lg) {
                content
            }
        }
        .frame(minHeight: GvSpacing.minAttributeHeight)
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
    func gvSelectAllOnFocus(isFocused: Bool) -> some View {
        self.onChange(of: isFocused) { _, focused in
            if focused {
                DispatchQueue.main.async {
                    UIApplication.shared.sendAction(#selector(UIResponder.selectAll(_:)), to: nil, from: nil, for: nil)
                }
            }
        }
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
