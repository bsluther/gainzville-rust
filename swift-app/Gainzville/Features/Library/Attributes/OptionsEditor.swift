import SwiftUI
import UniformTypeIdentifiers

/// One editable option row's model. A stable id keeps SwiftUI's `ForEach`
/// identity intact while the text is edited or rows are reordered — using the
/// string itself as id breaks on edits and on duplicate values.
struct OptionDraft: Identifiable, Equatable {
    let id = UUID()
    var text: String

    init(text: String = "") { self.text = text }
}

/// A bordered, editable list of option strings for Select/Multiselect configs.
///
/// Grows to fit its contents up to `visibleRows` rows, then scrolls. Rows carry
/// full-bleed separators and the list ends with an in-box "Add option" row.
///
/// Capability flags gate the affordances so one view serves both use cases:
/// the create form (full editing) and — later — the attribute detail card,
/// which is additive-only (`canDelete: false`, options never removed). Edits are
/// applied straight to the `options` binding; a live-committing host (the detail
/// card) can observe that binding to dispatch `UpdateAttribute` changes.
///
/// Drag-to-reorder (`canReorder`): each row has a leading drag handle that drags
/// (via system drag-and-drop, using an app-private transfer type so text fields
/// don't grab it). A row-shaped drag preview makes the dragged option obvious,
/// and the row under the cursor highlights to show where it will land.
struct OptionsEditor: View {
    @Binding var options: [OptionDraft]
    var canAdd: Bool = true
    var canDelete: Bool = true
    var canReorder: Bool = false
    var visibleRows: Int = 6
    var rowHeight: CGFloat = 44
    var borderColor: Color = .gvNeutral550

    @FocusState private var focusedOption: UUID?
    @State private var dropTargetID: UUID?

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                ForEach($options) { $option in
                    optionRow($option)
                }
                if canAdd { addRow }
            }
        }
        .frame(height: boxHeight)
        // No scroll needed while everything fits; only the over-cap case scrolls
        // (nested inside the form's ScrollView).
        .scrollDisabled(rowCount <= visibleRows)
        .font(.attrField)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(
            RoundedRectangle(cornerRadius: 8).stroke(borderColor, lineWidth: 1)
        )
    }

    /// Option rows plus the trailing add row.
    private var rowCount: Int { options.count + (canAdd ? 1 : 0) }

    /// Grow-to-fit, capped at `visibleRows`. Rows are exact-height and separators
    /// are overlays (no layout height), so this matches the content precisely on
    /// both platforms — no per-platform guessing.
    private var boxHeight: CGFloat { CGFloat(min(rowCount, visibleRows)) * rowHeight }

    @ViewBuilder
    private func optionRow(_ option: Binding<OptionDraft>) -> some View {
        let id = option.wrappedValue.id
        let isLastOption = id == options.last?.id
        let row = HStack(spacing: GvSpacing.md) {
            if canReorder {
                Image(systemName: "line.3.horizontal")
                    .foregroundStyle(Color.gvTextSecondary)
                    .frame(width: 28, height: rowHeight)
                    .contentShape(Rectangle())
                    .draggable(OptionDragItem(id: id.uuidString)) {
                        dragPreview(text: option.wrappedValue.text)
                    }
            }
            TextField("Option", text: option.text)
                .textFieldStyle(.plain)
                .foregroundStyle(Color.entryTextPrimary)
                .focused($focusedOption, equals: id)
            if canDelete {
                Button {
                    options.removeAll { $0.id == id }
                } label: {
                    Image(systemName: "minus.circle.fill")
                        .foregroundStyle(Color.gvTextSecondary)
                }
                .buttonStyle(.plain)
            }
        }
        .frame(height: rowHeight)
        .padding(.horizontal, GvSpacing.lg)
        // Highlight the row the dragged option will drop onto.
        .background(dropTargetID == id ? Color.gvNeutral850 : Color.clear)
        // Full-bleed rule below every row except the last one in the box. Drawn
        // as an overlay so it adds no layout height (keeps boxHeight exact).
        .overlay(alignment: .bottom) {
            if !isLastOption || canAdd {
                Rectangle().fill(Color.gvNeutral600).frame(height: 0.5)
            }
        }

        if canReorder {
            row.dropDestination(for: OptionDragItem.self) { items, _ in
                move(draggedID: items.first?.id, onto: id)
            } isTargeted: { targeted in
                if targeted { dropTargetID = id }
                else if dropTargetID == id { dropTargetID = nil }
            }
        } else {
            row
        }
    }

    /// The lifted drag preview: shaped like the actual row (handle + label on a
    /// solid card) so it's obvious which option is being dragged.
    private func dragPreview(text: String) -> some View {
        let card = HStack(spacing: 192) {
            Image(systemName: "line.3.horizontal")
                .foregroundStyle(Color.gvTextSecondary)
            Text(text.isEmpty ? "Option" : text)
                .foregroundStyle(Color.entryTextPrimary)
        }
        .font(.attrField)
        .padding(.horizontal, GvSpacing.lg)
        .frame(height: rowHeight)
        .background(Color.gvNeutral900)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(borderColor, lineWidth: 1))

        // The system centers the preview on the finger. An invisible twin to the
        // left reserves the card's width (plus the trailing clearance) so the
        // visible card's LEFT edge lands at the finger and the card extends
        // rightward — clear of the thumb on the leading drag handle. The trailing
        // clear space keeps the system drag "+" badge off the option text.
        let badgeClearance: CGFloat = 48
        return HStack(spacing: 0) {
            card.hidden().padding(.trailing, badgeClearance)
            card
            Color.clear.frame(width: badgeClearance)
        }
    }

    /// Reorder: move the dragged option to the dropped-on row's position.
    /// Returns false (a no-op) for anything that isn't one of our own options.
    private func move(draggedID: String?, onto targetID: UUID) -> Bool {
        dropTargetID = nil
        guard let draggedID,
              let from = options.firstIndex(where: { $0.id.uuidString == draggedID }),
              let to = options.firstIndex(where: { $0.id == targetID }),
              from != to
        else { return false }
        withAnimation {
            options.move(fromOffsets: IndexSet(integer: from),
                         toOffset: to > from ? to + 1 : to)
        }
        return true
    }

    private var addRow: some View {
        Button {
            let new = OptionDraft()
            options.append(new)
            focusedOption = new.id
        } label: {
            Label("Add option", systemImage: "plus")
                .foregroundStyle(Color.gvPrimaryAction)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: rowHeight)
                .padding(.horizontal, GvSpacing.lg)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// Drag payload for reordering option rows. Advertised as `public.data` (not a
/// text type) so text fields don't accept the drop and paste the payload into
/// their contents — while still being a registered type the drop target
/// recognizes on both iOS and macOS. (A custom `exportedAs` UTType worked on iOS
/// but, being undeclared in this generate-Info.plist target, wasn't advertised
/// on macOS, so drops there never matched.)
private struct OptionDragItem: Codable, Transferable {
    let id: String

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: .data)
    }
}
