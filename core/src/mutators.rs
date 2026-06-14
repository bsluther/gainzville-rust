use chrono::{DateTime, Utc};
use fractional_index::FractionalIndex;
use uuid::Uuid;

use crate::{
    actions::{
        Action, AttachValue, AttributeChange, ConvertToSets, CreateActivity, CreateAttribute,
        CreateEntry, CreateEntryFromActivity, CreateUser, CreateValue, DeleteAttributeValue,
        DeleteEntryRecursive, DuplicateEntry, EntryChange, LengthChange, MassChange, MoveEntry,
        NumericChange, SelectChange, TextChange, UpdateAttribute, UpdateAttributeValue,
        UpdateEntry, UpdateEntryCompletion, ValueField,
    },
    delta::{AnyDelta, Delta},
    error::{DomainError, RejectReason, Result},
    forest::Forest,
    instantiation::{duplicate_subtree, instantiate_subtree},
    io::Io,
    models::{
        actor::{Actor, ActorKind},
        attribute::{AttributeConfig, NumericValue, SelectValue, Value},
        entry::{Entry, Position, Temporal},
        user::User,
    },
    queries::{
        FindActivityById, FindActivityTemplateRoot, FindAncestors, FindAttributeById,
        FindDescendants, FindEntryById, FindUserById, FindUserByUsername, FindValueByKey,
        FindValuesForEntries, IsEmailRegistered,
    },
    query_executor::AnyQueryExecutor,
};

// TODO: make randomness/time deterministic in mutations.

/**
 * Constraints
 * - All mutators must be capable of running in a transaction.
 */

#[derive(Debug, Clone)]
pub struct Mutation {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub action: Action,
    pub changes: Vec<AnyDelta>,
}

/// Template entries live "outside the timeline": they may carry a duration but
/// never a start or end time. Shared by `create_entry` and `move_entry` so the
/// invariant is enforced on every write path that sets a template's temporal.
fn validate_template_temporal(
    is_template: bool,
    temporal: &crate::models::entry::Temporal,
) -> Result<()> {
    if is_template && (temporal.start().is_some() || temporal.end().is_some()) {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "template entries cannot have a start or end time (duration only)",
        )));
    }
    Ok(())
}

/// Load an entry's subtree (the entry plus all descendants) as a `Forest` for
/// structural checks. There is no dedicated children query; `FindDescendants`
/// is cheap at the scale of a single sequence.
async fn load_subtree_forest(
    executor: &mut impl AnyQueryExecutor,
    entry_id: Uuid,
) -> Result<Forest> {
    Ok(Forest::from(
        executor.execute(FindDescendants { entry_id }).await?,
    ))
}

/// The activity constraint a sets sequence imposes on an incoming member:
/// `None` when the sequence has no members yet (unconstrained), otherwise
/// `Some(activity)` — the value every member shares (`Some(id)` for an
/// activity, `None` for all-anonymous) that the incoming member must match.
/// Members of an already-flagged sequence that disagree are an observed
/// invariant violation, not a rejection.
fn sets_member_activity_constraint(
    parent_id: Uuid,
    members: &[&Entry],
) -> Result<Option<Option<Uuid>>> {
    let Some((first, rest)) = members.split_first() else {
        return Ok(None);
    };
    if rest.iter().any(|m| m.activity_id != first.activity_id) {
        return Err(DomainError::InvariantViolation {
            invariant: "sets members share one activity",
            context: format!("sequence '{}'", parent_id),
        });
    }
    Ok(Some(first.activity_id))
}

/// The display name of a sequence's first member, following the canonical
/// fallback rule (`EntryJoin`'s `compute_display_name`): the member's own
/// name, else its activity's name. `None` when there are no members or the
/// first member is fully anonymous — no "Unnamed" fallback; the caller leaves
/// the sequence unnamed instead.
async fn first_member_display_name(
    executor: &mut impl AnyQueryExecutor,
    parent_id: Uuid,
) -> Result<Option<String>> {
    let forest = load_subtree_forest(executor, parent_id).await?;
    let children = forest.children(parent_id);
    let Some(first) = children.first() else {
        return Ok(None);
    };
    if let Some(name) = first.name.as_deref() {
        if !name.is_empty() {
            return Ok(Some(name.to_string()));
        }
    }
    if let Some(activity_id) = first.activity_id {
        if let Some(activity) = executor
            .execute(FindActivityById { id: activity_id })
            .await?
        {
            return Ok(Some(activity.name.to_string()));
        }
    }
    Ok(None)
}

/// The shape `display_as_sets` requires at the moment the flag is set: a
/// sequence with at least one member, all members instances of one activity
/// (or all anonymous). Validated against any loaded forest — a stored subtree
/// or an in-memory template tree. Disagreement here is a rejection, not an
/// invariant violation: the flag isn't set yet, so heterogeneous members are
/// legal state.
fn validate_sets_shape(entry: &Entry, forest: &Forest) -> Result<()> {
    if !entry.is_sequence {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "display_as_sets requires a sequence entry",
        )));
    }
    let members = forest.children(entry.id);
    let Some((first, rest)) = members.split_first() else {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "display_as_sets requires at least one member",
        )));
    };
    if rest.iter().any(|m| m.activity_id != first.activity_id) {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "sets members must share one activity",
        )));
    }
    Ok(())
}

pub async fn create_user(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: CreateUser,
) -> Result<Mutation> {
    let user = action.user;
    // Check if email is already registered.
    if executor
        .execute(IsEmailRegistered {
            email: user.email.clone(),
        })
        .await?
    {
        return Err(DomainError::Rejected(RejectReason::EmailExists));
    }

    // Check if username is in use.
    if executor
        .execute(FindUserByUsername {
            username: user.username.clone(),
        })
        .await?
        .is_some()
    {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "user already in use",
        )));
    }

    // Check if ID is in use.
    if executor
        .execute(FindUserById {
            actor_id: user.actor_id,
        })
        .await?
        .is_some()
    {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "actor_id already in use",
        )));
    }

    let insert_actor = Delta::<Actor>::Insert {
        new: Actor {
            actor_id: user.actor_id,
            actor_kind: ActorKind::User,
            created_at: io.current_time_wall_clock(),
        },
    };
    let insert_user = Delta::<User>::Insert { new: user.clone() };

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::CreateUser(CreateUser { user }),
        changes: vec![insert_actor.into(), insert_user.into()],
    })
}

pub async fn create_activity(
    _executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: CreateActivity,
) -> Result<Mutation> {
    let activity = action.activity.clone();
    if action.actor_id != activity.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not authorized to create activities for owner '{}'",
            action.actor_id, activity.owner_id
        ))));
    }

    // Templates must form a tree.
    let template_forest = Forest::from(action.template.clone());
    let Some(root) = template_forest.tree_root() else {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "activity template must form a tree",
        )));
    };

    // Root of template tree must have activity_id == activity.id.
    if root.activity_id != Some(activity.id) {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "activity template must have a root entry with activity_id == activity.id",
        )));
    }

    // All templates must have is_template == true.
    if !action.template.iter().all(|e| e.is_template) {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "all template entries must have is_template == true",
        )));
    }

    // Sets invariants hold inside templates too: any flagged template entry
    // must already have the sets shape.
    for entry in action.template.iter().filter(|e| e.display_as_sets) {
        validate_sets_shape(entry, &template_forest)?;
    }

    let insert_activity = Delta::Insert { new: activity };
    let insert_templates: Vec<AnyDelta> = action
        .template
        .iter()
        .map(|e| Delta::Insert { new: e.clone() }.into())
        .collect();

    let mut deltas: Vec<AnyDelta> = vec![insert_activity.into()];
    deltas.extend(insert_templates);

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::CreateActivity(action.clone()),
        changes: deltas,
    })
}

pub async fn create_entry(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: CreateEntry,
) -> Result<Mutation> {
    // Check if actor has permission to create entry at the given position.
    // For now, only allow the owner to create.
    if action.actor_id != action.entry.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not authorized to create entry for owner '{}' in parent entry '{:?}'",
            action.actor_id,
            action.entry.owner_id,
            action.entry.position.map(|p| p.parent_id)
        ))));
    }

    // display_as_sets is earned, not born: a fresh entry has no members, so
    // it cannot satisfy the sets shape (>=1 member). The flag is set later
    // via ConvertToSets / UpdateEntry(SetDisplayAsSets).
    if action.entry.display_as_sets {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "a new entry cannot have display_as_sets set",
        )));
    }

    // Check if referenced activity exists.
    if let Some(activity_id) = action.entry.activity_id {
        if executor
            .execute(FindActivityById { id: activity_id })
            .await?
            .is_none()
        {
            return Err(DomainError::Rejected(RejectReason::NotFound(format!(
                "create entry failed, activity '{}' not found",
                activity_id
            ))));
        }
    };

    // A child must match its parent's template/log kind — a template tree and a
    // log tree never mix. Enforced here (not just in move_entry) so no caller
    // can create a mismatched child that would then fail to move.
    if let Some(position) = &action.entry.position {
        if let Some(parent) = executor
            .execute(FindEntryById {
                entry_id: position.parent_id,
            })
            .await?
        {
            if parent.is_template != action.entry.is_template {
                return Err(DomainError::Rejected(RejectReason::Precondition(
                    "child entry must match its parent's template/log kind",
                )));
            }

            // Joining a sets sequence: the new member must match the members'
            // shared activity (or shared anonymity).
            if parent.display_as_sets {
                let forest = load_subtree_forest(executor, parent.id).await?;
                if let Some(required) =
                    sets_member_activity_constraint(parent.id, &forest.children(parent.id))?
                {
                    if action.entry.activity_id != required {
                        return Err(DomainError::Rejected(RejectReason::Precondition(
                            "sets members must share one activity",
                        )));
                    }
                }
            }
        }
    }

    validate_template_temporal(action.entry.is_template, &action.entry.temporal)?;

    let insert_entry = Delta::Insert {
        new: action.entry.clone(),
    };

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::CreateEntry(action),
        changes: vec![insert_entry.into()],
    })
}

/// Instantiate an activity's template into a fresh log subtree. Finds the
/// activity's template root, deep-copies the subtree (entries + values) with new
/// ids and `is_template` cleared, and places the instantiated root at the given
/// position/temporal. Emits all inserts in one mutation (FK checks are deferred
/// to commit, so delta order doesn't matter).
pub async fn create_entry_from_activity(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: CreateEntryFromActivity,
) -> Result<Mutation> {
    let activity = executor
        .execute(FindActivityById {
            id: action.activity_id,
        })
        .await?
        .ok_or_else(|| {
            DomainError::Rejected(RejectReason::NotFound(format!(
                "activity '{}' not found",
                action.activity_id
            )))
        })?;

    // Only the owner can instantiate their activities (for now).
    if action.actor_id != activity.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not authorized to instantiate activity owned by '{}'",
            action.actor_id, activity.owner_id
        ))));
    }

    // Placement validation mirrors move_entry, applied to the instantiated root.
    if let Some(position) = &action.position {
        let parent = executor
            .execute(FindEntryById {
                entry_id: position.parent_id,
            })
            .await?
            .ok_or_else(|| {
                DomainError::Rejected(RejectReason::NotFound(
                    "instantiation parent does not exist".to_string(),
                ))
            })?;
        if !parent.is_sequence {
            return Err(DomainError::Rejected(RejectReason::Precondition(
                "cannot instantiate into a non-sequence entry",
            )));
        }
        // A template tree and a log tree never mix: the instantiated subtree's
        // kind must match the parent's (log into log, template into template).
        if parent.is_template != action.is_template {
            return Err(DomainError::Rejected(RejectReason::Precondition(
                "instantiated subtree must match its parent's template/log kind",
            )));
        }

        // Joining a sets sequence: the instantiated root is an entry of this
        // activity, which must match the members' shared activity.
        if parent.display_as_sets {
            let forest = load_subtree_forest(executor, parent.id).await?;
            if let Some(required) =
                sets_member_activity_constraint(parent.id, &forest.children(parent.id))?
            {
                if required != Some(action.activity_id) {
                    return Err(DomainError::Rejected(RejectReason::Precondition(
                        "sets members must share one activity",
                    )));
                }
            }
        }
    } else if !action.is_template
        && action.temporal.start().is_none()
        && action.temporal.end().is_none()
    {
        // Log roots must be placed on the timeline; template roots are exempt.
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "root entry must have defined start or end time",
        )));
    }

    // Template entries (root or child) never carry a start or end.
    validate_template_temporal(action.is_template, &action.temporal)?;

    let root = executor
        .execute(FindActivityTemplateRoot {
            activity_id: action.activity_id,
        })
        .await?
        .ok_or_else(|| {
            // Reaching here means we've observed an *existing* invariant violation:
            // the activity exists but its template root is missing.
            DomainError::InvariantViolation {
                invariant: "activity has a template root",
                context: format!("activity '{}'", action.activity_id),
            }
        })?;

    let subtree = executor
        .execute(FindDescendants { entry_id: root.id })
        .await?;
    let subtree_ids: Vec<Uuid> = subtree.iter().map(|e| e.id).collect();
    let values = executor
        .execute(FindValuesForEntries {
            entry_ids: subtree_ids,
        })
        .await?;

    let (entries, values) = instantiate_subtree(
        io,
        root.id,
        &subtree,
        &values,
        action.position.clone(),
        action.temporal.clone(),
        action.is_template,
    );

    let mut deltas: Vec<AnyDelta> = entries
        .into_iter()
        .map(|e| Delta::Insert { new: e }.into())
        .collect();
    deltas.extend(values.into_iter().map(|v| Delta::Insert { new: v }.into()));

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::CreateEntryFromActivity(action),
        changes: deltas,
    })
}

/// Move an entry by changing it's parent, fractional index, and temporal. Does not allow
/// moving to root without a defined start or end time; while the model allows for this, it
/// should be intentional and utilize a different action.
/// TOOD: you shouldn't be able to move template entries between template trees. Eg I can't move an
///       entry from My Workout's template to Strenght Workout's template.
pub async fn move_entry(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: MoveEntry,
) -> Result<Mutation> {
    // Moving entry must exist.
    let Some(entry) = executor
        .execute(FindEntryById {
            entry_id: action.entry_id,
        })
        .await?
    else {
        return Err(DomainError::Rejected(RejectReason::NotFound(
            "entry that does not exist cannot be moved".to_string(),
        )));
    };

    if let Some(position) = &action.position {
        // Root template entries must remain at the root.
        if entry.is_template && entry.position.is_none() {
            return Err(DomainError::Rejected(RejectReason::Precondition(
                "root template entry may not change position",
            )));
        }

        // Check for cycles.
        let parent_ancestors: Vec<Uuid> = executor
            .execute(FindAncestors {
                entry_id: position.parent_id,
            })
            .await?;
        if parent_ancestors.contains(&action.entry_id) {
            return Err(DomainError::Rejected(RejectReason::Precondition(
                "move_entry would create a cycle",
            )));
        }

        // Template entries cannot be moved outside their template tree.
        // Note that this also enforces that a template entry cannot be moved to root, and therefore
        // maintains that a template tree has a single root.
        if entry.is_template {
            let entry_root = executor
                .execute(FindAncestors { entry_id: entry.id })
                .await?
                .first()
                .copied()
                .expect("ancestors should always be non-empty");
            let dest_root = parent_ancestors
                .first()
                .copied()
                .expect("ancestors should always be non-empty");
            if entry_root != dest_root {
                return Err(DomainError::Rejected(RejectReason::Precondition(
                    "template entries cannot be moved outside their template tree",
                )));
            }
        }

        let parent = executor
            .execute(FindEntryById {
                entry_id: position.parent_id,
            })
            .await?
            .expect("parent should exist after earlier condition");

        // Destination entry must be a sequence.
        if !parent.is_sequence {
            return Err(DomainError::Rejected(RejectReason::Precondition(
                "cannot move entry into a non-sequence entry",
            )));
        }

        // Check parent and child are both template or log entries
        if entry.is_template && !parent.is_template {
            return Err(DomainError::Rejected(RejectReason::Precondition(
                "template entry cannot be a child of a log entry",
            )));
        }
        if !entry.is_template && parent.is_template {
            return Err(DomainError::Rejected(RejectReason::Precondition(
                "log entry cannot be a child of a template entry",
            )));
        }

        // Joining a sets sequence: the incoming member must match the
        // members' shared activity (or shared anonymity). A same-parent
        // reorder passes trivially — the mover is one of the members the
        // constraint is computed from.
        if parent.display_as_sets {
            let forest = load_subtree_forest(executor, parent.id).await?;
            if let Some(required) =
                sets_member_activity_constraint(parent.id, &forest.children(parent.id))?
            {
                if entry.activity_id != required {
                    return Err(DomainError::Rejected(RejectReason::Precondition(
                        "sets members must share one activity",
                    )));
                }
            }
        }
    } else if !entry.is_template {
        // Log entries at root must be placed on the timeline; templates are
        // exempt (they live outside the timeline).
        if action.temporal.start().is_none() && action.temporal.end().is_none() {
            return Err(DomainError::Rejected(RejectReason::Precondition(
                "root entry must have defined start or end time",
            )));
        }
    }

    // Leaving a sets sequence: its last member cannot leave (display_as_sets
    // requires >=1 member — break out or delete the whole sequence instead).
    if let Some(old_position) = &entry.position {
        let parent_changed =
            action.position.as_ref().map(|p| p.parent_id) != Some(old_position.parent_id);
        if parent_changed {
            let old_parent = executor
                .execute(FindEntryById {
                    entry_id: old_position.parent_id,
                })
                .await?;
            if old_parent.is_some_and(|p| p.display_as_sets) {
                let forest = load_subtree_forest(executor, old_position.parent_id).await?;
                if forest.children(old_position.parent_id).len() <= 1 {
                    return Err(DomainError::Rejected(RejectReason::Precondition(
                        "cannot remove the last member of a sets sequence",
                    )));
                }
            }
        }
    }

    // Any templ
    validate_template_temporal(entry.is_template, &action.temporal)?;

    let update_delta = entry
        .update()
        .position(action.position.clone())
        .temporal(action.temporal.clone())
        .to_delta();

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::MoveEntry(action),
        changes: vec![update_delta.into()],
    })
}

pub async fn delete_entry_recursive(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: DeleteEntryRecursive,
) -> Result<Mutation> {
    // Get entry and all descendants.
    // - Once attributes are in place, will need get/delete them as well.
    let subtree = executor
        .execute(FindDescendants {
            entry_id: action.entry_id,
        })
        .await?;
    let subtree_ids: Vec<Uuid> = subtree.iter().map(|e| e.id).collect();
    let subtree_attr_values = executor
        .execute(FindValuesForEntries {
            entry_ids: subtree_ids,
        })
        .await?;

    let Some(root) = subtree.iter().find(|e| e.id == action.entry_id) else {
        assert!(
            subtree.is_empty(),
            "descendants query must include the root entry or be null,
            found a non-empty tree which does not contain the root"
        );
        return Err(DomainError::Rejected(RejectReason::NotFound(
            "delete_entry_recursive failed: entry not found in database".to_string(),
        )));
    };

    // Check if actor has permission to delete.
    if action.actor_id != root.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(
            "delete_entry_recursive actor is not the owner of the deleting entry".to_string(),
        )));
    }

    // The last member of a sets sequence cannot be deleted (display_as_sets
    // requires >=1 member — break out or delete the sequence itself instead).
    if let Some(parent_id) = root.parent_id() {
        let parent = executor
            .execute(FindEntryById {
                entry_id: parent_id,
            })
            .await?;
        if parent.is_some_and(|p| p.display_as_sets) {
            let forest = load_subtree_forest(executor, parent_id).await?;
            if forest.children(parent_id).len() <= 1 {
                return Err(DomainError::Rejected(RejectReason::Precondition(
                    "cannot remove the last member of a sets sequence",
                )));
            }
        }
    }

    // Create delete deltas for entry and descendants.
    let entry_deltas: Vec<AnyDelta> = subtree
        .into_iter()
        .map(|e| Delta::Delete { old: e }.into())
        .collect();

    // Create delete deltas for entry and descendants attribute values.
    let attr_value_deltas: Vec<AnyDelta> = subtree_attr_values
        .into_iter()
        .map(|v| Delta::Delete { old: v }.into())
        .collect();

    let mut deltas = entry_deltas;
    deltas.extend(attr_value_deltas);

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: action.into(),
        changes: deltas,
    })
}

/// Convert an entry into a sets sequence (see `ConvertToSets`): one mutation
/// that inserts the anonymous sequence at the entry's position and reparents
/// the entry under it as the sole member, splitting the temporal — the
/// sequence takes the start/end, the entry keeps only its duration.
pub async fn convert_to_sets(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: ConvertToSets,
) -> Result<Mutation> {
    let Some(entry) = executor
        .execute(FindEntryById {
            entry_id: action.entry_id,
        })
        .await?
    else {
        return Err(DomainError::Rejected(RejectReason::NotFound(
            "entry that does not exist cannot be converted to sets".to_string(),
        )));
    };

    if action.actor_id != entry.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not the owner of entry '{}'",
            action.actor_id, entry.id
        ))));
    }

    // An activity template's root must keep activity_id == activity.id, and
    // the wrap sequence is anonymous — converting the root would break the
    // template-tree rule.
    if entry.is_template && entry.position.is_none() {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "cannot convert an activity template root to sets",
        )));
    }

    if executor
        .execute(FindEntryById {
            entry_id: action.sequence_id,
        })
        .await?
        .is_some()
    {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "sequence_id already in use",
        )));
    }

    // The anonymous sequence takes the entry's place among its siblings; if
    // the entry is itself a set member, anonymity must satisfy the parent's
    // member-activity constraint.
    if let Some(position) = &entry.position {
        let parent = executor
            .execute(FindEntryById {
                entry_id: position.parent_id,
            })
            .await?;
        if parent.is_some_and(|p| p.display_as_sets) {
            let forest = load_subtree_forest(executor, position.parent_id).await?;
            let siblings: Vec<&Entry> = forest
                .children(position.parent_id)
                .into_iter()
                .filter(|m| m.id != entry.id)
                .collect();
            if let Some(required) = sets_member_activity_constraint(position.parent_id, &siblings)?
            {
                if required.is_some() {
                    return Err(DomainError::Rejected(RejectReason::Precondition(
                        "sets members must share one activity",
                    )));
                }
            }
        }
    }

    // The sequence owns the entry's timeline slot (start/end); the entry
    // keeps only its duration.
    let sequence_temporal = Temporal::parse(entry.temporal.start(), entry.temporal.end(), None)?;
    let member_temporal = Temporal::parse(None, None, entry.temporal.duration())?;

    // The sequence inherits the entry's root placement, so the root rule
    // carries over: a log root must already be on the timeline.
    if entry.position.is_none()
        && !entry.is_template
        && sequence_temporal.start().is_none()
        && sequence_temporal.end().is_none()
    {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "root entry must have defined start or end time",
        )));
    }

    let sequence = Entry {
        id: action.sequence_id,
        activity_id: None,
        owner_id: entry.owner_id,
        name: None,
        position: entry.position.clone(),
        is_template: entry.is_template,
        display_as_sets: true,
        is_sequence: true,
        is_complete: false,
        temporal: sequence_temporal,
    };

    let member_update = entry
        .update()
        .position(Some(Position {
            parent_id: action.sequence_id,
            frac_index: FractionalIndex::default(),
        }))
        .temporal(member_temporal)
        .to_delta();

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::ConvertToSets(action),
        changes: vec![Delta::Insert { new: sequence }.into(), member_update.into()],
    })
}

/// Duplicate an entry's subtree in place (see `DuplicateEntry`): an exact
/// copy with fresh ids, inserted immediately after the source among its
/// siblings.
pub async fn duplicate_entry(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: DuplicateEntry,
) -> Result<Mutation> {
    let subtree = executor
        .execute(FindDescendants {
            entry_id: action.entry_id,
        })
        .await?;
    let Some(entry) = subtree.iter().find(|e| e.id == action.entry_id).cloned() else {
        return Err(DomainError::Rejected(RejectReason::NotFound(
            "entry that does not exist cannot be duplicated".to_string(),
        )));
    };

    if action.actor_id != entry.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not the owner of entry '{}'",
            action.actor_id, entry.id
        ))));
    }

    // Each activity has exactly one template root; duplicating it would mint
    // a second.
    if entry.is_template && entry.position.is_none() {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "cannot duplicate an activity template root",
        )));
    }

    // The copy lands immediately after the source among its siblings; a
    // forest root duplicates in place (same temporal, adjacent in the day
    // view).
    let root_position = match &entry.position {
        None => None,
        Some(position) => {
            let forest = load_subtree_forest(executor, position.parent_id).await?;
            let siblings = forest.children(position.parent_id);
            let successor = siblings
                .iter()
                .skip_while(|e| e.id != entry.id)
                .nth(1)
                .map(|e| e.id);
            let slot = forest
                .position_between(position.parent_id, Some(entry.id), successor)
                .ok_or_else(|| DomainError::InvariantViolation {
                    invariant: "sibling order yields an insertion slot",
                    context: format!("entry '{}' in parent '{}'", entry.id, position.parent_id),
                })?;
            Some(slot)
        }
    };

    let subtree_ids: Vec<Uuid> = subtree.iter().map(|e| e.id).collect();
    let values = executor
        .execute(FindValuesForEntries {
            entry_ids: subtree_ids,
        })
        .await?;

    let (entries, values) = duplicate_subtree(io, entry.id, &subtree, &values, root_position);

    let mut deltas: Vec<AnyDelta> = entries
        .into_iter()
        .map(|e| Delta::Insert { new: e }.into())
        .collect();
    deltas.extend(values.into_iter().map(|v| Delta::Insert { new: v }.into()));

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::DuplicateEntry(action),
        changes: deltas,
    })
}

pub async fn create_attribute(
    _executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: CreateAttribute,
) -> Result<Mutation> {
    let attribute = action.attribute.clone();

    // Only the owner can create attributes for themselves (for now).
    if action.actor_id != attribute.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not authorized to create attributes for owner '{}'",
            action.actor_id, attribute.owner_id
        ))));
    }

    attribute.config.validate()?;

    let insert_attribute = Delta::Insert { new: attribute };

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::CreateAttribute(action),
        changes: vec![insert_attribute.into()],
    })
}

pub async fn update_entry_completion(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: UpdateEntryCompletion,
) -> Result<Mutation> {
    let Some(entry) = executor
        .execute(FindEntryById {
            entry_id: action.entry_id,
        })
        .await?
    else {
        return Err(DomainError::Rejected(RejectReason::NotFound(
            "entry does not exist".to_string(),
        )));
    };

    // Only the owner may complete their own entries.
    if action.actor_id != entry.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not the owner of entry '{}'",
            action.actor_id, entry.id
        ))));
    }

    // Template entries represent activity definitions, not logged events.
    if entry.is_template {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "template entries cannot be marked complete",
        )));
    }

    // Sequence entries are containers; completion applies only to leaf entries.
    if entry.is_sequence {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "sequence entries cannot be marked complete",
        )));
    }

    let update_delta = entry.update().is_complete(action.is_complete).to_delta();

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::UpdateEntryCompletion(action),
        changes: vec![update_delta.into()],
    })
}

pub async fn create_value(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: CreateValue,
) -> Result<Mutation> {
    let value = action.value.clone();

    // The entry must exist.
    let entry = executor
        .execute(FindEntryById {
            entry_id: value.entry_id,
        })
        .await?
        .ok_or_else(|| {
            DomainError::Rejected(RejectReason::NotFound(format!(
                "entry '{}' not found",
                value.entry_id
            )))
        })?;

    // Only the entry owner can create values on it.
    if action.actor_id != entry.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not authorized to create values on entry owned by '{}'",
            action.actor_id, entry.owner_id
        ))));
    }

    // The attribute must exist and be owned by the same actor as the entry.
    let attribute = executor
        .execute(FindAttributeById {
            attribute_id: value.attribute_id,
        })
        .await?
        .ok_or_else(|| {
            DomainError::Rejected(RejectReason::NotFound(format!(
                "attribute '{}' not found",
                value.attribute_id
            )))
        })?;

    if attribute.owner_id != entry.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "attribute owner '{}' does not match entry owner '{}'",
            attribute.owner_id, entry.owner_id
        ))));
    }

    if let Some(plan) = &value.plan {
        attribute.validate_value(plan)?;
    }
    if let Some(actual) = &value.actual {
        attribute.validate_value(actual)?;
    }

    // No-op if a value for this (entry_id, attribute_id) already exists: an
    // attribute is attached to an entry at most once.
    if executor
        .execute(FindValueByKey {
            entry_id: value.entry_id,
            attribute_id: value.attribute_id,
        })
        .await?
        .is_some()
    {
        return Ok(Mutation {
            id: io.uuid(),
            timestamp: io.current_time_wall_clock(),
            action: Action::CreateValue(action),
            changes: vec![],
        });
    }

    let insert_value = Delta::Insert { new: value };

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::CreateValue(action),
        changes: vec![insert_value.into()],
    })
}

/// Attach an attribute to an entry, seeding the value from the attribute's
/// config default (both plan and actual). A no-op if the attribute is already
/// attached. The default is resolved here in core via `Attribute::seed_value`.
pub async fn attach_value(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: AttachValue,
) -> Result<Mutation> {
    let entry = executor
        .execute(FindEntryById {
            entry_id: action.entry_id,
        })
        .await?
        .ok_or_else(|| {
            DomainError::Rejected(RejectReason::NotFound(format!(
                "entry '{}' not found",
                action.entry_id
            )))
        })?;

    // Only the entry owner can attach values to it.
    if action.actor_id != entry.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not authorized to attach values on entry owned by '{}'",
            action.actor_id, entry.owner_id
        ))));
    }

    let attribute = executor
        .execute(FindAttributeById {
            attribute_id: action.attribute_id,
        })
        .await?
        .ok_or_else(|| {
            DomainError::Rejected(RejectReason::NotFound(format!(
                "attribute '{}' not found",
                action.attribute_id
            )))
        })?;

    if attribute.owner_id != entry.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "attribute owner '{}' does not match entry owner '{}'",
            attribute.owner_id, entry.owner_id
        ))));
    }

    // No-op if already attached.
    if executor
        .execute(FindValueByKey {
            entry_id: action.entry_id,
            attribute_id: action.attribute_id,
        })
        .await?
        .is_some()
    {
        return Ok(Mutation {
            id: io.uuid(),
            timestamp: io.current_time_wall_clock(),
            action: Action::AttachValue(action),
            changes: vec![],
        });
    }

    let seeded = attribute.seed_value(action.entry_id);
    let insert_value = Delta::Insert { new: seeded };

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::AttachValue(action),
        changes: vec![insert_value.into()],
    })
}

/// Detach an attribute from an entry by deleting its value, keyed by
/// `(entry_id, attribute_id)`. A no-op if no such value exists, so the UI
/// toggle is idempotent.
pub async fn delete_attribute_value(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: DeleteAttributeValue,
) -> Result<Mutation> {
    let Some(old) = executor
        .execute(FindValueByKey {
            entry_id: action.entry_id,
            attribute_id: action.attribute_id,
        })
        .await?
    else {
        // No-op: nothing attached.
        return Ok(Mutation {
            id: io.uuid(),
            timestamp: io.current_time_wall_clock(),
            action: Action::DeleteAttributeValue(action),
            changes: vec![],
        });
    };

    let entry = executor
        .execute(FindEntryById {
            entry_id: action.entry_id,
        })
        .await?
        .ok_or_else(|| {
            // A value keyed to an entry that doesn't exist is a dangling row —
            // an observed invariant violation, not a rejected action.
            DomainError::InvariantViolation {
                invariant: "value references an existing entry",
                context: format!("entry '{}'", action.entry_id),
            }
        })?;

    // Only the entry owner can detach values from it.
    if action.actor_id != entry.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not the owner of entry '{}'",
            action.actor_id, entry.id
        ))));
    }

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::DeleteAttributeValue(action),
        changes: vec![Delta::<Value>::Delete { old }.into()],
    })
}

pub async fn update_attribute_value(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: UpdateAttributeValue,
) -> Result<Mutation> {
    let Some(entry) = executor
        .execute(FindEntryById {
            entry_id: action.entry_id,
        })
        .await?
    else {
        return Err(DomainError::Rejected(RejectReason::NotFound(
            "entry does not exist".to_string(),
        )));
    };

    if action.actor_id != entry.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not the owner of entry '{}'",
            action.actor_id, entry.id
        ))));
    }

    let Some(attribute) = executor
        .execute(FindAttributeById {
            attribute_id: action.attribute_id,
        })
        .await?
    else {
        return Err(DomainError::Rejected(RejectReason::NotFound(
            "attribute does not exist".to_string(),
        )));
    };

    // `None` (clearing the field) is trivially valid.
    if let Some(value) = &action.value {
        attribute.validate_value(value)?;
    }

    let Some(old) = executor
        .execute(FindValueByKey {
            entry_id: action.entry_id,
            attribute_id: action.attribute_id,
        })
        .await?
    else {
        return Err(DomainError::Rejected(RejectReason::Precondition(
            "value does not exist; use CreateValue before UpdateAttributeValue",
        )));
    };

    let new = match action.field {
        ValueField::Plan => Value {
            plan: action.value.clone(),
            ..old.clone()
        },
        ValueField::Actual => Value {
            actual: action.value.clone(),
            ..old.clone()
        },
    };

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::UpdateAttributeValue(action),
        changes: vec![Delta::<Value>::Update { old, new }.into()],
    })
}

/// Apply a single `AttributeChange` to an attribute. Common edits (name,
/// description) are unconstrained; type-specific edits are rejected if the
/// variant doesn't match the attribute's config type, and `Set*Default` edits
/// are validated against the config (select option membership, numeric
/// integer/min/max). A change that leaves the attribute unchanged is a no-op.
pub async fn update_attribute(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: UpdateAttribute,
) -> Result<Mutation> {
    let Some(old) = executor
        .execute(FindAttributeById {
            attribute_id: action.attribute_id,
        })
        .await?
    else {
        return Err(DomainError::Rejected(RejectReason::NotFound(
            "attribute does not exist".to_string(),
        )));
    };

    // Only the owner can modify their attributes.
    if action.actor_id != old.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not the owner of attribute '{}'",
            action.actor_id, old.id
        ))));
    }

    let mut new = old.clone();
    match &action.change {
        AttributeChange::SetName(name) => new.name = name.clone(),
        AttributeChange::SetDescription(description) => new.description = description.clone(),
        AttributeChange::Numeric(change) => {
            let AttributeConfig::Numeric(cfg) = &mut new.config else {
                return Err(DomainError::Rejected(RejectReason::AttributeMismatch));
            };
            match change {
                NumericChange::SetDefault(default) => {
                    // Same rules as a value write: finite, integer-ness, min/max.
                    if let Some(v) = default {
                        cfg.validate_value(&NumericValue::Exact(*v))?;
                    }
                    cfg.default = *default;
                }
            }
        }
        AttributeChange::Select(change) => {
            let AttributeConfig::Select(cfg) = &mut new.config else {
                return Err(DomainError::Rejected(RejectReason::AttributeMismatch));
            };
            match change {
                SelectChange::SetDefault(default) => {
                    // Same rule as a value write: option membership.
                    if let Some(s) = default {
                        cfg.validate_value(&SelectValue::Exact(s.clone()))?;
                    }
                    cfg.default = default.clone();
                }
            }
        }
        AttributeChange::Mass(change) => {
            let AttributeConfig::Mass(cfg) = &mut new.config else {
                return Err(DomainError::Rejected(RejectReason::AttributeMismatch));
            };
            match change {
                MassChange::SetDefaultUnit(unit) => {
                    cfg.default_unit = unit.clone();
                }
            }
        }
        AttributeChange::Length(change) => {
            let AttributeConfig::Length(cfg) = &mut new.config else {
                return Err(DomainError::Rejected(RejectReason::AttributeMismatch));
            };
            match change {
                LengthChange::SetDefaultUnit(unit) => {
                    cfg.default_unit = unit.clone();
                }
            }
        }
        AttributeChange::Text(change) => {
            let AttributeConfig::Text(cfg) = &mut new.config else {
                return Err(DomainError::Rejected(RejectReason::AttributeMismatch));
            };
            match change {
                TextChange::SetDefault(default) => {
                    // Same rule as a value write: the length cap.
                    if let Some(s) = default {
                        cfg.validate_value(s)?;
                    }
                    cfg.default = default.clone();
                }
                TextChange::SetAutocomplete(on) => {
                    cfg.autocomplete = *on;
                }
            }
        }
    }

    // No-op if the change left the attribute unchanged.
    if new == old {
        return Ok(Mutation {
            id: io.uuid(),
            timestamp: io.current_time_wall_clock(),
            action: Action::UpdateAttribute(action),
            changes: vec![],
        });
    }

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::UpdateAttribute(action),
        changes: vec![Delta::<crate::models::attribute::Attribute>::Update { old, new }.into()],
    })
}

/// Update an entry's structural/metadata fields (currently `is_sequence`).
/// Converting a sequence to a scalar deletes all descendants and their values —
/// a scalar cannot contain children. Position/temporal are not touched here;
/// they go through `move_entry`.
pub async fn update_entry(
    executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: UpdateEntry,
) -> Result<Mutation> {
    let Some(entry) = executor
        .execute(FindEntryById {
            entry_id: action.entry_id,
        })
        .await?
    else {
        return Err(DomainError::Rejected(RejectReason::NotFound(
            "entry does not exist".to_string(),
        )));
    };

    if action.actor_id != entry.owner_id {
        return Err(DomainError::Rejected(RejectReason::Unauthorized(format!(
            "actor '{}' is not the owner of entry '{}'",
            action.actor_id, entry.id
        ))));
    }

    let mut deltas: Vec<AnyDelta> = vec![];

    match &action.change {
        EntryChange::SetIsSequence(is_sequence) => {
            if entry.is_sequence == *is_sequence {
                // No-op.
                return Ok(Mutation {
                    id: io.uuid(),
                    timestamp: io.current_time_wall_clock(),
                    action: Action::UpdateEntry(action),
                    changes: vec![],
                });
            }

            // A sets sequence relies on being a sequence — and becoming a
            // scalar would silently deep-delete the members. Break out of
            // sets first.
            if !is_sequence && entry.display_as_sets {
                return Err(DomainError::Rejected(RejectReason::Precondition(
                    "a sets sequence cannot become a scalar (break out of sets first)",
                )));
            }

            // Converting sequence -> scalar: a scalar can't hold children, so
            // delete the entire descendant subtree (and its attribute values).
            if !is_sequence {
                let subtree = executor
                    .execute(FindDescendants {
                        entry_id: action.entry_id,
                    })
                    .await?;
                let descendants: Vec<_> = subtree
                    .into_iter()
                    .filter(|e| e.id != action.entry_id)
                    .collect();
                let descendant_ids: Vec<Uuid> = descendants.iter().map(|e| e.id).collect();
                let values = executor
                    .execute(FindValuesForEntries {
                        entry_ids: descendant_ids,
                    })
                    .await?;
                deltas.extend(
                    descendants
                        .into_iter()
                        .map(|e| Delta::Delete { old: e }.into()),
                );
                deltas.extend(values.into_iter().map(|v| Delta::Delete { old: v }.into()));
            }

            let update = entry.update().is_sequence(*is_sequence).to_delta();
            deltas.push(update.into());
        }
        EntryChange::SetDisplayAsSets(display_as_sets) => {
            if entry.display_as_sets == *display_as_sets {
                // No-op.
                return Ok(Mutation {
                    id: io.uuid(),
                    timestamp: io.current_time_wall_clock(),
                    action: Action::UpdateEntry(action),
                    changes: vec![],
                });
            }

            if *display_as_sets {
                // Setting the flag requires the sets shape.
                let forest = load_subtree_forest(executor, entry.id).await?;
                validate_sets_shape(&entry, &forest)?;
                deltas.push(entry.update().display_as_sets(true).to_delta().into());
            } else {
                // Breaking out is always legal. The wrapper is typically
                // anonymous (ConvertToSets creates it that way) and would
                // display as "Unnamed" once it renders as an ordinary
                // sequence, so derive a name from the first member — only
                // when no name is already set.
                let mut update = entry.update().display_as_sets(false);
                if entry.name.is_none() {
                    if let Some(member_name) = first_member_display_name(executor, entry.id).await?
                    {
                        update = update.name(Some(format!("{member_name} Sets")));
                    }
                }
                deltas.push(update.to_delta().into());
            }
        }
    }

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::UpdateEntry(action),
        changes: deltas,
    })
}
