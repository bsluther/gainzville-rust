// # Drag and drop design
//
// Any entry (scalar or sequence, root or child) can be dragged to a new position within
// a sequence. Dragging to the root level is deferred — see "Pending work" below.
//
// ## Data flow
// - Drag source: EntryHeader applies .onDrag, which fires synchronously at drag start and
//   stores the dragged FfiEntry in DragState. The entry ID string is the NSItemProvider payload.
// - Drop targets: DropTarget views are interleaved before, between, and after each child in
//   ChildrenSection. Each carries a pre-computed FfiPosition (from Forest.positionBetween) and
//   the pred/succ entry IDs for its slot.
// - On hover: DropDelegate.validateDrop gates whether the indicator line is shown. A target is
//   invalid if the dragged entry is adjacent to the slot (no-op move) or if dropping there would
//   create a cycle (entry dragged into one of its own descendants).
// - On drop: performDrop re-validates and dispatches MoveEntry via ForestViewModel, preserving
//   the entry's existing temporal. DragState is cleared.
//
// ## Platform notes
// - .onDrag (NSItemProvider) + .onDrop(delegate:) must be used together. The newer
//   .draggable/.dropDestination(for:) pair does not interoperate with this API on iOS.
// - Color.clear has no hit area for the drag system; DropTarget uses Color.white.opacity(0.001)
//   as its base layer to ensure drops register.
// - Slot identity in ChildrenSection is pred/succ-based (not offset-based) to prevent SwiftUI
//   from transferring isTargeted @State to mis-matched slots after a drop + forest refresh.
//
// ## Pending work
// - Drag to root: dropping an entry onto the root log level should detach it from its parent
//   sequence and assign it a start time. The intended UX is to open the temporal time picker
//   with the date pre-filled from the drop target's day, letting the user explicitly choose the
//   time. No root-level drop targets are shown today; drops there are silently ignored.

import SwiftUI
import UniformTypeIdentifiers
internal import Combine

// MARK: - Drag state

@MainActor
class DragState: ObservableObject {
    // Not @Published — set during the .onDrag closure (view evaluation context),
    // where publishing is disallowed. DropTarget re-reads on isTargeted changes, so
    // reactivity isn't needed.
    var draggedEntry: FfiEntry? = nil
}

// MARK: - Drag preview

struct EntryDragPreview: View {
    let displayName: String

    var body: some View {
        Text(displayName)
            .font(.gvBody)
            .foregroundStyle(Color.entryTextPrimary)
            .padding(.vertical, GvSpacing.entrySpacing)
            .padding(.horizontal, GvSpacing.entrySpacing)
            .frame(width: 260, alignment: .leading)
            .entryContainerStyle(isSequence: false)
    }
}

// MARK: - Drop target

struct DropTarget: View, DropDelegate {
    let position: FfiPosition
    let predId: String?
    let succId: String?
    @EnvironmentObject private var forestVM: ForestViewModel
    @EnvironmentObject private var dragState: DragState
    @State private var isTargeted = false

    private func isValidDrop(entryId: String) -> Bool {
        entryId != predId && entryId != succId &&
        !forestVM.wouldCreateCycle(entryId: entryId, proposedParentId: position.parentId)
    }

    func validateDrop(info: DropInfo) -> Bool {
        guard let entry = dragState.draggedEntry else { return false }
        return isValidDrop(entryId: entry.id)
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        isTargeted = true
        return DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) {
        isTargeted = false
    }

    func performDrop(info: DropInfo) -> Bool {
        isTargeted = false
        guard let entry = dragState.draggedEntry,
              isValidDrop(entryId: entry.id)
        else { return false }
        forestVM.moveEntry(entry, to: position)
        dragState.draggedEntry = nil
        return true
    }

    var body: some View {
        ZStack {
            // Nearly-transparent rather than Color.clear — Color.clear has no hit area
            // for the drag system and drops won't register.
            Color.white.opacity(0.001)
            if isTargeted {
                RoundedRectangle(cornerRadius: 1)
                    .fill(Color.gvLoggedBlue)
                    .frame(height: 2)
            }
        }
        .frame(height: GvSpacing.entrySpacing)
        .onDrop(of: [UTType.plainText], delegate: self)
    }
}
