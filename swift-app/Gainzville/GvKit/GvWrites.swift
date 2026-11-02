//
//  GvWrites.swift
//  GvKit
//
//  What views can do to the data, named in the domain's terms. Each helper is
//  one record-level action today; later intent-level writes replace the bodies,
//  not the call sites.
//
//  Writes return at commit, and hooks follow on a later batch: nothing here
//  reads its own write. A write that fails is logged and dropped.
//

import Foundation
import os

private let log = Logger(subsystem: "com.gainzville", category: "GvKit")

extension GvClient {
    func run(_ action: Action) {
        do {
            try core.runAction(action: action)
        } catch {
            log.error("write failed: \(String(describing: error), privacy: .public)")
        }
    }

    // MARK: Entries

    func moveEntry(_ entry: Entry, to position: Position) {
        run(.moveEntry(MoveEntry(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entry.id,
            position: position,
            temporal: entry.temporal
        )))
    }

    func updateEntryTemporal(entry: Entry, temporal: Temporal) {
        run(.moveEntry(MoveEntry(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entry.id,
            position: entry.position,
            temporal: temporal
        )))
    }

    /// Move an entry to root on the given day with a chosen start time.
    /// Preserves duration if set; replaces any prior start and drops any prior end.
    func moveEntryToRoot(_ entry: Entry, startTime: Date) {
        let ms = Int64(startTime.timeIntervalSince1970 * 1000)
        let newTemporal: Temporal = {
            switch entry.temporal {
            case .none, .start, .end, .startAndEnd:
                return .start(start: ms)
            case .duration(let d),
                 .startAndDuration(_, let d),
                 .durationAndEnd(let d, _):
                return .startAndDuration(start: ms, durationMs: d)
            }
        }()
        run(.moveEntry(MoveEntry(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entry.id,
            position: nil,
            temporal: newTemporal
        )))
    }

    func createRootEntry(activityId: String?, name: String?, isSequence: Bool, for logDay: LogDay) {
        let suggestedMs = read(.suggestedRootInsertionTime(on: logDay))
        // With an activity, instantiate its template (structure + default values);
        // the sheet's name/sequence are ignored since the template defines them.
        if let activityId {
            run(.createEntryFromActivity(CreateEntryFromActivity(
                actorId: SYSTEM_ACTOR_ID,
                activityId: activityId,
                position: nil,
                temporal: .start(start: suggestedMs),
                isTemplate: false  // root entries created from the log are log entries
            )))
            return
        }
        run(.createEntry(CreateEntry(
            actorId: SYSTEM_ACTOR_ID,
            entry: Entry(
                id: UUID().uuidString,
                activityId: nil,
                ownerId: SYSTEM_ACTOR_ID,
                name: name,
                position: nil,
                isTemplate: false,
                displayAsSets: false,
                isSequence: isSequence,
                isComplete: false,
                temporal: .start(start: suggestedMs)
            )
        )))
    }

    /// Create a child of `parent` at `position` (from `.appendPosition(in:)`).
    func createChildEntry(in parent: Entry, at position: Position, activityId: String?, name: String?, isSequence: Bool) {
        // With an activity, instantiate its template under this parent. The
        // instantiated subtree's kind matches the parent: composing into a
        // template yields template entries; into the log, log entries.
        if let activityId {
            run(.createEntryFromActivity(CreateEntryFromActivity(
                actorId: SYSTEM_ACTOR_ID,
                activityId: activityId,
                position: position,
                temporal: .none,
                isTemplate: parent.isTemplate
            )))
            return
        }
        run(.createEntry(CreateEntry(
            actorId: SYSTEM_ACTOR_ID,
            entry: Entry(
                id: UUID().uuidString,
                activityId: nil,
                ownerId: SYSTEM_ACTOR_ID,
                name: name,
                position: position,
                // A child of a template is itself a template entry; a child of a
                // log entry is a log entry. Mismatched template/log nesting is
                // rejected by move_entry, so inherit the parent's flag.
                isTemplate: parent.isTemplate,
                displayAsSets: false,
                isSequence: isSequence,
                isComplete: false,
                temporal: .none
            )
        )))
    }

    func updateEntryCompletion(entry: Entry, isComplete: Bool) {
        run(.updateEntryCompletion(UpdateEntryCompletion(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entry.id,
            isComplete: isComplete
        )))
    }

    func deleteEntry(entry: Entry) {
        run(.deleteEntryRecursive(DeleteEntryRecursive(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entry.id
        )))
    }

    /// Toggle an entry between sequence and scalar. Converting a sequence to a
    /// scalar deletes its children (enforced in core); the UI should eventually
    /// confirm this destructive action.
    func setIsSequence(entryId: String, isSequence: Bool) {
        run(.updateEntry(UpdateEntry(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entryId,
            change: .setIsSequence(isSequence)
        )))
    }

    /// Toggle the sets presentation. Setting it requires the sets shape
    /// (sequence, ≥1 member, members share one activity — enforced in core);
    /// clearing it ("break out") is always legal.
    func setDisplayAsSets(entryId: String, displayAsSets: Bool) {
        run(.updateEntry(UpdateEntry(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entryId,
            change: .setDisplayAsSets(displayAsSets)
        )))
    }

    /// Convert an entry into a sets sequence. The caller mints `sequenceId`
    /// (lowercased, to match ids as core emits them) so it can mark the
    /// replacement card expanded before it mounts.
    func convertToSets(entry: Entry, sequenceId: String) {
        run(.convertToSets(ConvertToSets(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entry.id,
            sequenceId: sequenceId
        )))
    }

    /// Deep-copy an entry's subtree (exact copy, fresh ids), inserted
    /// immediately after the source among its siblings.
    func duplicateEntry(entry: Entry) {
        run(.duplicateEntry(DuplicateEntry(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entry.id
        )))
    }

    // MARK: Attribute values

    /// Update an entry's value for a given attribute. The value row must exist:
    /// callers reach this from attribute pairs in an entry's join.
    func updateAttributeValue(
        entryId: String,
        attributeId: String,
        field: ValueField,
        value: AttributeValue?
    ) {
        run(.updateAttributeValue(UpdateAttributeValue(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entryId,
            attributeId: attributeId,
            field: field,
            value: value
        )))
    }

    /// Empty a value's field (`plan`/`actual`) while keeping the attribute
    /// attached — `UpdateAttributeValue` with a `nil` value.
    func clearAttributeValue(entryId: String, attributeId: String, field: ValueField) {
        updateAttributeValue(entryId: entryId, attributeId: attributeId, field: field, value: nil)
    }

    /// Attach an attribute to an entry (creates its value row).
    func attachValue(entryId: String, attributeId: String) {
        run(.attachValue(AttachValue(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entryId,
            attributeId: attributeId
        )))
    }

    /// Detach an attribute from an entry (deletes its value row). Idempotent in core.
    func removeAttribute(entryId: String, attributeId: String) {
        run(.deleteAttributeValue(DeleteAttributeValue(
            actorId: SYSTEM_ACTOR_ID,
            entryId: entryId,
            attributeId: attributeId
        )))
    }

    // MARK: Library

    func createActivity(name: String, description: String?) {
        let activityId = UUID().uuidString
        let activity = Activity(
            id: activityId,
            ownerId: SYSTEM_ACTOR_ID,
            sourceActivityId: nil,
            name: name,
            description: description
        )
        let template = Entry(
            id: UUID().uuidString,
            activityId: activityId,
            ownerId: SYSTEM_ACTOR_ID,
            name: nil,
            position: nil,
            isTemplate: true,
            displayAsSets: false,
            isSequence: false,
            isComplete: false,
            temporal: .none
        )
        run(.createActivity(CreateActivity(
            actorId: SYSTEM_ACTOR_ID,
            activity: activity,
            template: [template]
        )))
    }

    func updateAttribute(attributeId: String, change: AttributeChange) {
        run(.updateAttribute(UpdateAttribute(
            actorId: SYSTEM_ACTOR_ID,
            attributeId: attributeId,
            change: change
        )))
    }
}
