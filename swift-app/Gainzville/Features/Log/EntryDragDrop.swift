// # Drag and drop design
//
// Any entry (scalar or sequence, root or child) can be dragged to a new position within
// a sequence, or to the root of a day.
//
// ## Drop target zones
// 1. Sequence slots — between/around children inside an expanded sequence (DropTarget).
// 2. Day-root empty area — anywhere inside the log not covered by an entry (DayRootDropDelegate).
// 3. Root scalar entry body — file-system-style: dropping on a root-level scalar forwards
//    to the day root (EntryDropDelegate, "forwarding" mode).
// All other entry interiors (root sequences outside their child slots, every non-root entry)
// return a .forbidden DropProposal so day-root targeting doesn't activate over them.
//
// ## Data flow
// - Drag source: EntryHeader applies .onDrag, which fires synchronously at drag start and
//   stores the dragged Entry in DragState.
// - Three delegates compete via SwiftUI's deepest-hit dispatch:
//     • DropTarget (deepest, in ChildrenSection slots) — precise sequence insertion.
//     • EntryDropDelegate (per EntryView) — forwards to day-root for root scalars; .forbidden
//       elsewhere. The .forbidden return blocks the day-root delegate from receiving the
//       drop while inside an entry's body.
//     • DayRootDropDelegate (LogView outer) — handles drops outside any entry.
// - Day-root targeting state lives on DragState.isTargetingDayRoot (shared across delegates)
//   and drives LogView's background lighten effect.
// - Day-root drops open RootDropTimePickerSheet; on confirm, MoveEntry runs with position=nil
//   and a temporal that uses the chosen start time (preserving duration if set).
//
// ## Validity rules
// - Sequence: drop is invalid if it'd be a no-op (adjacent to source) or create a cycle.
// - Day-root (both empty-area and root-scalar forwarder): invalid if the dragged entry is
//   already at root (position == nil); the indicator stays off and drop is rejected.
//
// ## Platform notes
// - .onDrag (NSItemProvider) + .onDrop(delegate:) must be used together. The newer
//   .draggable/.dropDestination(for:) pair does not interoperate with this API on iOS.
// - Color.clear has no hit area for the drag system; DropTarget uses Color.white.opacity(0.001)
//   as its base layer to ensure drops register. EntryView and LogView attach to views with
//   their own hit areas so this isn't needed there.
// - Slot identity in ChildrenSection is pred/succ-based (not offset-based) to prevent SwiftUI
//   from transferring isTargeted @State to mis-matched slots after a drop + forest refresh.
// - Day-root drops dispatch their callback via DispatchQueue.main.async so the drag session
//   unwinds before SwiftUI evaluates any new sheet item — avoids "sheet during drag" bugs.
// - macOS silently drops the LAST .onDrop registration in the view tree: it is never
//   consulted (no validateDrop at all, not even a rejection), though the view renders and
//   applies the modifier. LogView appends a DropRegistrationSink to absorb this; without it
//   the bottom entry card's drops are dead. Symptom to recognise: drag ghost tracks normally,
//   slots render, and NOTHING fires — no indicator and no forbidden cursor.
//   The library never hit this because its template card is always followed by more content.
//   Evidence and root-cause research plan: swift-app/docs/macos-drop-registration-bug.md

import SwiftUI
import UniformTypeIdentifiers
internal import Combine

// MARK: - Drag state

@MainActor
class DragState: ObservableObject {
    // Not @Published — set during the .onDrag closure (view evaluation context),
    // where publishing is disallowed. DropTarget re-reads on isTargeted changes, so
    // reactivity isn't needed.
    var draggedEntry: Entry? = nil

    // Whether the current drag is targeting a day-root drop zone (empty area in
    // the log, or a root scalar entry). Both DayRootDropDelegate and the root-scalar
    // path of EntryDropDelegate write to this; LogView observes it to lighten the
    // log background. Published is safe here because it's only mutated from
    // dropEntered/dropExited/performDrop (action contexts), never the .onDrag closure.
    @Published var isTargetingDayRoot: Bool = false
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
    let position: Position
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

// MARK: - Trailing registration sink (macOS)

/// A sacrificial drop registration placed after the last entry card in the log.
///
/// On macOS the LAST `.onDrop` registration in the view tree never gets consulted:
/// no validateDrop, no dropEntered, no rejection cursor — it is simply absent from
/// the set AppKit hit-tests, even though the view renders and applies its modifier
/// normally. In the log that victim was the bottom entry card (with a single root
/// entry, the only card), so drag-and-drop appeared completely dead there while
/// working fine in the library, where a template card is always followed by another
/// registration. This view takes the bullet instead, and declines everything.
///
/// Trailing `Color.clear` does NOT substitute — it has no hit area, so it forms no
/// registration at all (same reason DropTarget uses opacity(0.001) as its base).
struct DropRegistrationSink: View {
    var body: some View {
        Color.white.opacity(0.001)
            .frame(height: 1)
            .onDrop(of: [UTType.plainText], delegate: SinkDelegate())
    }

    private struct SinkDelegate: DropDelegate {
        func validateDrop(info: DropInfo) -> Bool { false }
        func performDrop(info: DropInfo) -> Bool { false }
    }
}

// MARK: - Day-root drop target

/// DropDelegate attached at the LogView level. Handles drops landing in empty
/// space inside the log (outside any entry). Drops that land inside an entry
/// are intercepted by EntryDropDelegate (deeper in the hit chain), which either
/// forwards to day-root semantics (root scalars) or returns .forbidden.
struct DayRootDropDelegate: DropDelegate {
    let dragState: DragState
    let onDrop: (Entry) -> Void

    func validateDrop(info: DropInfo) -> Bool {
        // Reject if the dragged entry is already at root — moving root→root is a no-op.
        guard let dragged = dragState.draggedEntry else { return false }
        return dragged.position != nil
    }

    func dropEntered(info: DropInfo) {
        dragState.isTargetingDayRoot = true
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) {
        dragState.isTargetingDayRoot = false
    }

    func performDrop(info: DropInfo) -> Bool {
        dragState.isTargetingDayRoot = false
        guard let entry = dragState.draggedEntry, entry.position != nil else { return false }
        // Defer state mutation so the drag session unwinds before SwiftUI evaluates
        // a new sheet item — avoids "sheet during drag" presentation issues.
        DispatchQueue.main.async {
            onDrop(entry)
        }
        dragState.draggedEntry = nil
        return true
    }
}

// MARK: - Per-entry drop delegate

/// Attached to every EntryView. Two modes:
/// - "Forwarding": when the entry is a root scalar (position == nil && !isSequence)
///   AND a day-root drop callback is provided, behaves like DayRootDropDelegate —
///   file-system metaphor where dropping on a leaf inserts at the leaf's level.
/// - "Forbidding": for all other entries (root sequences, child entries). Returns
///   .forbidden so the day-root delegate above doesn't activate while the cursor
///   is inside the entry's body. Sequence-level DropTargets are deeper still and
///   continue to handle in-sequence drops normally.
struct EntryDropDelegate: DropDelegate {
    let entry: Entry
    let dragState: DragState
    let onDayRootDrop: ((Entry) -> Void)?

    private var actsAsDayRoot: Bool {
        onDayRootDrop != nil && entry.position == nil && !entry.isSequence
    }

    private var canAcceptAsDayRoot: Bool {
        guard actsAsDayRoot else { return false }
        guard let dragged = dragState.draggedEntry else { return false }
        return dragged.position != nil
    }

    func validateDrop(info: DropInfo) -> Bool {
        // Always claim the type so the parent (DayRootDropDelegate) doesn't
        // receive it. dropUpdated returns .forbidden when we won't actually
        // accept — that suppresses the indicator and rejects the drop.
        dragState.draggedEntry != nil
    }

    func dropEntered(info: DropInfo) {
        if canAcceptAsDayRoot {
            dragState.isTargetingDayRoot = true
        }
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: canAcceptAsDayRoot ? .move : .forbidden)
    }

    func dropExited(info: DropInfo) {
        if actsAsDayRoot {
            dragState.isTargetingDayRoot = false
        }
    }

    func performDrop(info: DropInfo) -> Bool {
        guard canAcceptAsDayRoot,
              let onDayRootDrop,
              let dragged = dragState.draggedEntry else { return false }
        dragState.isTargetingDayRoot = false
        DispatchQueue.main.async {
            onDayRootDrop(dragged)
        }
        dragState.draggedEntry = nil
        return true
    }
}
