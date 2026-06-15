import SwiftUI

// Shared layout for attribute-style rows: label on the left, custom input on the
// right, consistent min-height across temporal and attribute editors.
struct AttributeRow<Content: View>: View {
    let label: String
    let indent: CGFloat
    // Most editors fall back to vertical stacking when compact pills don't fit
    // the width. The text editor opts out: its field is always full-width (so it
    // always fits the horizontal candidate), and the re-measure ViewThatFits
    // runs when the vertical-growing TextField wraps 1→2 lines rebuilds the
    // candidate subtrees, re-making the field and dropping first responder
    // mid-type.
    let usesViewThatFits: Bool
    // How the label aligns to the value column. `.top` (default) is right for
    // the compact pills, whose height equals the label band — top and center
    // coincide. The text editor's field is taller than the band (extra vertical
    // padding for multi-line breathing room), so it passes `.firstTextBaseline`
    // to sit the label on the field's first line instead of riding high above it.
    let verticalAlignment: VerticalAlignment
    private let content: Content

    init(
        label: String,
        indent: CGFloat = 0,
        usesViewThatFits: Bool = true,
        verticalAlignment: VerticalAlignment = .top,
        @ViewBuilder content: () -> Content
    ) {
        self.label = label
        self.indent = indent
        self.usesViewThatFits = usesViewThatFits
        self.verticalAlignment = verticalAlignment
        self.content = content()
    }

    var body: some View {
        // Top/first-baseline alignment (see `verticalAlignment`) keeps the label
        // anchored to the first row when value pills wrap to multiple lines.
        HStack(alignment: verticalAlignment, spacing: 0) {
            // Label, sized to content, on the left.
            HStack(alignment: .center, spacing: 0) {
                Text(label)
                    .font(.attrLabel)
                    .foregroundStyle(Color.entryTextSecondary)
                    .padding(.leading, indent)
            }
            .frame(minHeight: GvSpacing.minAttributeHeight)

            // Value pills lay out in a row; if they don't fit the remaining
            // width, fall back to stacking vertically (right-aligned) rather
            // than wrapping mid-row. ViewThatFits measures each candidate
            // against the proposed width and picks the first that fits — this is
            // also what stops fixed-size pills from inflating the entry. The
            // maxWidth:.infinity frame claims the leftover horizontal space and
            // right-pushes the content, mirroring the prior Spacer-pushed look.
            if usesViewThatFits {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: GvSpacing.lg) { content }
                    VStack(alignment: .trailing, spacing: GvSpacing.lg) { content }
                }
                .frame(maxWidth: .infinity, alignment: .trailing)
            } else {
                // No re-measuring wrapper: the content (a full-width text field)
                // always fits, and ViewThatFits' re-eval on growth re-makes it.
                HStack(spacing: GvSpacing.lg) { content }
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
        }
        .frame(minHeight: GvSpacing.minAttributeHeight)
    }
}

// Shared pill style for attribute value display across temporal and attribute
// editors. Apply with `.gvAttributePill()`.
extension View {
    func gvAttributePill(
        borderColor: Color = .entryTextSecondary,
        verticalPadding: CGFloat = GvSpacing.sm
    ) -> some View {
        self
            .font(.attrField)
            .foregroundStyle(Color.entryTextPrimary)
            .padding(.horizontal, GvSpacing.lg)
            .padding(.vertical, verticalPadding)
            .frame(minHeight: GvSpacing.minAttributeHeight)
            .background(.clear)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(borderColor, lineWidth: 1)
            )
    }

    /// Adds the shared attribute action bar above the keyboard (iOS). When an
    /// attribute is focused the bar shows that attribute's controls; otherwise
    /// it falls back to a lone dismiss button (the old "Done" behavior). Apply
    /// once at the container level — only one `.keyboard` toolbar may exist, or
    /// items duplicate across multiple text fields.
    func gvAttributeKeyboardBar() -> some View {
        modifier(AttributeKeyboardBar())
    }

    /// Adds a global keyboard toolbar with a "Done" button to dismiss the
    /// keyboard. Used on surfaces with plain text fields and no entry-attribute
    /// editing (e.g. the library attribute-config screens).
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
        case .length(let p):  return p.attrId
        case .text(let p):    return p.attrId
        }
    }

    public var name: String {
        switch self {
        case .numeric(let p): return p.name
        case .select(let p):  return p.name
        case .mass(let p):    return p.name
        case .length(let p):  return p.name
        case .text(let p):    return p.name
        }
    }
}
