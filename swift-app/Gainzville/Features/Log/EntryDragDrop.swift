// # Drag and drop design
//
// Any entry (scalar or sequence, root or child) can be dragged to a new position within
// a sequence, or to the root of a day.
//
// ## Data flow
// - Drag source: EntryHeader applies .onDrag, which fires synchronously at drag start and
//   stores the dragged FfiEntry in DragState. The entry ID string is the NSItemProvider payload.
// - Sequence drop targets: DropTarget views are interleaved before, between, and after each
//   child in ChildrenSection. Each carries a pre-computed FfiPosition (from Forest.positionBetween)
//   and the pred/succ entry IDs for its slot.
// - Day-root drop target: DayRootDropDelegate is attached at the LogView level, covering the
//   day's entire content area. SwiftUI's hit-testing delivers drops to the deepest matching
//   delegate, so sequence-level DropTargets take precedence where they overlap. On a successful
//   day-root drop, LogView opens a time picker sheet to choose the entry's start time.
// - On hover (sequence): DropDelegate.validateDrop gates whether the indicator line is shown.
//   A target is invalid if the dragged entry is adjacent to the slot (no-op move) or if dropping
//   there would create a cycle (entry dragged into one of its own descendants).
// - On hover (day-root): the day's background tints (lighten in dark mode, darken in light mode).
// - On drop (sequence): performDrop re-validates and dispatches MoveEntry via ForestViewModel,
//   preserving the entry's existing temporal.
// - On drop (day-root): performDrop hands the dragged entry to LogView's onDrop callback, which
//   opens the time picker. On confirm, MoveEntry is dispatched with position=nil and a temporal
//   that uses the chosen start time (preserving duration if set, dropping any prior start/end).
//
// ## Platform notes
// - .onDrag (NSItemProvider) + .onDrop(delegate:) must be used together. The newer
//   .draggable/.dropDestination(for:) pair does not interoperate with this API on iOS.
// - Color.clear has no hit area for the drag system; DropTarget uses Color.white.opacity(0.001)
//   as its base layer to ensure drops register. The day-root delegate doesn't need this because
//   it attaches to a view that already has a non-clear background (Color.gvBackground).
// - Slot identity in ChildrenSection is pred/succ-based (not offset-based) to prevent SwiftUI
//   from transferring isTargeted @State to mis-matched slots after a drop + forest refresh.
// - DayRootDropDelegate dispatches its onDrop callback via DispatchQueue.main.async so the drag
//   session unwinds before SwiftUI evaluates any new sheet item — avoids "sheet during drag" bugs.

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

// MARK: - Day-root drop target

/// DropDelegate for the entire day's content area in LogView. Sibling DropTargets
/// inside sequences take precedence via SwiftUI hit-testing; this delegate handles
/// any drop that lands outside a sequence — i.e. dropping at the day's root.
struct DayRootDropDelegate: DropDelegate {
    let dragState: DragState
    @Binding var isTargeted: Bool
    let onDrop: (FfiEntry) -> Void

    func validateDrop(info: DropInfo) -> Bool {
        dragState.draggedEntry != nil
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
        guard let entry = dragState.draggedEntry else { return false }
        // Defer state mutation so the drag session unwinds before SwiftUI evaluates
        // a new sheet item — avoids "sheet during drag" presentation issues.
        DispatchQueue.main.async {
            onDrop(entry)
        }
        dragState.draggedEntry = nil
        return true
    }
}
