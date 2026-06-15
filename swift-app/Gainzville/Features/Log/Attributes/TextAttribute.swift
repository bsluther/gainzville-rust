internal import Combine
import SwiftUI

// Editor for text attributes — free-form notes, locations, route names. The
// value is a bare string (no exact/range axis, no units), so this is the
// simplest of the value editors; the only non-trivial part is optional
// autocomplete from the attribute's prior values.
//
// Field shape: a vertical-growing `TextField`. Unfocused it caps short (a
// truncated preview); focused it grows with content and scrolls past the cap.
//
// Autocomplete (when `config.autocomplete`): a focus-scoped subscription to
// `DistinctTextValuesForAttribute` (plan ∪ actual). The subscription is held
// only while the field is focused — it re-runs on every app-wide write, so
// scoping it to the edit session keeps that cost to a single field. See
// `TextSuggestionsModel`.
struct TextAttribute: View {
    let entry: Entry
    let pair: TextAttributePair
    @EnvironmentObject private var forestVM: ForestViewModel
    @EnvironmentObject private var coreEnv: CoreEnv
    @EnvironmentObject private var dataChange: DataChange
    @EnvironmentObject private var autocomplete: AutocompleteCoordinator

    @State private var text: String = ""
    @FocusState private var focused: Bool
    // Set by Remove so the blur handler skips the commit (it would write against
    // the just-deleted row).
    @State private var pendingRemoval = false
    @StateObject private var suggestions = TextSuggestionsModel()

    private var focusToken: String { "\(entry.id)/\(pair.attrId)" }

    private var barActions: [AttributeBarAction] {
        .text(remove: {
            pendingRemoval = true
            forestVM.removeAttribute(entryId: entry.id, attributeId: pair.attrId)
            focused = false
        })
    }

    // Prior values matching the current input, minus an exact echo of it.
    private var filteredSuggestions: [String] {
        guard pair.config.autocomplete else { return [] }
        let query = text.trimmingCharacters(in: .whitespaces)
        // Exclude the in-progress text and this field's own committed value —
        // suggesting the value you're already editing is just noise.
        let pool = suggestions.all.filter { $0 != text && $0 != pair.actual }
        let matches =
            query.isEmpty ? pool : pool.filter { $0.localizedCaseInsensitiveContains(query) }
        // Capped, but generous enough that the box scrolls (see its max height).
        return Array(matches.prefix(20))
    }

    var body: some View {
        AttributeRow(label: pair.name, usesViewThatFits: false) {
            textField
                // Publish bounds + matches so LogView can float the suggestion
                // list outside this (clipped) card. Attached to the pill before
                // the leading padding so the anchor tracks the visible field.
                .anchorPreference(key: AutocompleteRequestKey.self, value: .bounds) { anchor in
                    (focused && !filteredSuggestions.isEmpty)
                        ? AutocompleteRequest(
                            fieldKey: focusToken, suggestions: filteredSuggestions, anchor: anchor)
                        : nil
                }
                // Gap between the label and the full-width text pill. The
                // compact numeric/mass pills get this spacing for free from
                // being right-aligned; the greedy text pill otherwise butts
                // against the label.
                .padding(.leading, GvSpacing.xl)
        }
        .onAppear {
            syncEditState()
            #if os(macOS)
            AttributePopoverClicks.install()
            #endif
        }
        .onChange(of: pair.actual) { _, _ in
            guard !focused else { return }
            syncEditState()
        }
        .attributeBarActions(
            token: focusToken,
            isFocused: focused,
            actions: barActions
        )
        .onChange(of: focused) { _, isFocused in
            if isFocused {
                if pair.config.autocomplete {
                    suggestions.start(
                        core: coreEnv.core, dataChange: dataChange, attributeId: pair.attrId)
                }
            } else {
                suggestions.stop()
                if pendingRemoval {
                    pendingRemoval = false
                } else if !flushNow() {
                    syncEditState()
                }
            }
        }
        // Apply a suggestion tapped in the LogView overlay (routed by fieldKey)
        // through the field's own pick(): set text, blur, commit.
        .onChange(of: autocomplete.pendingPick) { _, pending in
            guard let pending, pending.fieldKey == focusToken else { return }
            pick(pending.value)
            autocomplete.pendingPick = nil
        }
    }

    @ViewBuilder
    private var textField: some View {
        TextField("", text: $text, axis: .vertical)
            .textFieldStyle(.plain)
            // Unfocused: a short truncated preview. Focused: grow with content,
            // then scroll past the cap.
            .lineLimit(focused ? 1...12 : 1...3)
            .focused($focused)
            // Same pill border as the other attribute fields. The pill's
            // minHeight vertically centers a single line on the label (without
            // it, a one-line field floats to the top of the row). No
            // `.fixedSize` — unlike the compact numeric/mass pills, this one
            // fills the row width and grows downward for long notes. Vertical
            // padding is bumped up (one notch below the horizontal inset) so a
            // multi-line note breathes (the compact pills keep the default —
            // their height is driven by minHeight anyway).
            .gvAttributePill(verticalPadding: GvSpacing.md)
        // macOS: anchor the action-bar popover (Remove) to the field, mirroring
        // the other editors. Closing it ends editing.
        #if os(macOS)
            .popover(
                isPresented: Binding(
                    get: { focused },
                    set: { shown in
                        guard !shown else { return }
                        guard let hit = AttributePopoverClicks.consumedTextFieldHit() else {
                            focused = false
                            return
                        }
                        guard !AttributePopoverClicks.isFirstResponder(hit) else { return }
                        DispatchQueue.main.async {
                            let ok = hit.window?.makeFirstResponder(hit) ?? false
                            if !ok { focused = false }
                        }
                    }
                ),
                arrowEdge: .top
            ) {
                AttributeSheetBar(
                    title: pair.name,
                    actions: barActions,
                    onDismiss: { focused = false }
                )
                .frame(width: 280)
            }
        #endif
    }

    // Fill the field from a suggestion and dismiss. The blur handler owns the
    // commit — flushing here too would double-dispatch, since the first write
    // hasn't refreshed `pair.actual` by the time the blur handler re-checks.
    private func pick(_ suggestion: String) {
        text = suggestion
        focused = false
    }

    // MARK: - Sync cache → shadow

    private func syncEditState() {
        text = pair.actual ?? ""
    }

    // MARK: - Commit shadow → cache

    // Text commits on blur (and on pick), not via a debounce: an autosave while
    // typing would write the in-progress value, which then refreshes the
    // history subscription and echoes back into this field's own suggestions.
    @discardableResult
    private func flushNow() -> Bool {
        switch pendingCommit() {
        case nil:
            return false
        case .clear:
            forestVM.clearAttributeValue(entryId: entry.id, attributeId: pair.attrId, field: .actual)
            return true
        case .set(let value):
            forestVM.updateAttributeValue(
                entryId: entry.id,
                attributeId: pair.attrId,
                field: .actual,
                value: .text(value)
            )
            return true
        }
    }

    /// An emptied field clears the stored value; otherwise the text is stored
    /// verbatim (no normalization). A no-op when nothing changed.
    private func pendingCommit() -> PendingWrite<String>? {
        if text.isEmpty {
            return pair.actual == nil ? nil : .clear
        }
        guard pair.actual != text else { return nil }
        return .set(text)
    }
}

/// Backs a text attribute's autocomplete. Focus-scoped: `start()` on focus,
/// `stop()` on blur. Holds the `FfiQuerySubscription` only while editing, so
/// the query (which re-runs on every app-wide write) is live for one field at a
/// time. Mirrors the per-component subscription pattern of `EntryViewModel` /
/// `AttributeDetailViewModel`, but torn down on blur rather than on disappear.
@MainActor
final class TextSuggestionsModel: ObservableObject {
    @Published var all: [String] = []
    private var core: GainzvilleCore?
    private var attributeId: String = ""
    private var subscription: FfiQuerySubscription?
    private var cancellable: AnyCancellable?

    func start(core: GainzvilleCore?, dataChange: DataChange, attributeId: String) {
        guard subscription == nil, let core else { return }
        self.core = core
        self.attributeId = attributeId
        subscription = try? core.subscribeQuery(
            query: .distinctTextValuesForAttribute(
                DistinctTextValuesForAttribute(attributeId: attributeId)))
        refresh()
        cancellable = dataChange.didChange.sink { [weak self] in self?.refresh() }
    }

    func stop() {
        // Releasing the handle drops the Rust-side subscription (refcount
        // evict); clearing the sink stops refreshes.
        subscription = nil
        cancellable = nil
        all = []
    }

    private func refresh() {
        guard let core else { return }
        if case .distinctTextValuesForAttribute(let values) = core.readQuery(
            query: .distinctTextValuesForAttribute(
                DistinctTextValuesForAttribute(attributeId: attributeId)))
        {
            all = values
        }
    }
}
