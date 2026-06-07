use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    actions::{
        Action, AttachValue, AttributeChange, CreateActivity, CreateAttribute, CreateEntry,
        CreateEntryFromActivity, CreateUser, CreateValue, DeleteAttributeValue,
        DeleteEntryRecursive, EntryChange, MassChange, MoveEntry, NumericChange, SelectChange,
        UpdateAttribute, UpdateAttributeValue, UpdateEntry, UpdateEntryCompletion, ValueField,
    },
    delta::{AnyDelta, Delta},
    error::{DomainError, Result, ValidationError},
    forest::Forest,
    instantiation::instantiate_subtree,
    io::Io,
    models::{
        actor::{Actor, ActorKind},
        attribute::{AttributeConfig, Value},
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
        return Err(DomainError::Consistency(
            "template entries cannot have a start or end time (duration only)".to_string(),
        ));
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
        return Err(DomainError::EmailAlreadyExists);
    }

    // Check if username is in use.
    if executor
        .execute(FindUserByUsername {
            username: user.username.clone(),
        })
        .await?
        .is_some()
    {
        return Err(DomainError::Other("user already in use".to_string()));
    }

    // Check if ID is in use.
    if executor
        .execute(FindUserById {
            actor_id: user.actor_id,
        })
        .await?
        .is_some()
    {
        return Err(DomainError::Other("actor_id already in use".to_string()));
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
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not authorized to create activities for owner '{}'",
            action.actor_id, activity.owner_id
        )));
    }

    // Templates must form a tree.
    let template_forest = Forest::from(action.template.clone());
    let Some(root) = template_forest.tree_root() else {
        return Err(DomainError::Consistency(
            "activity template must form a tree".to_string(),
        ));
    };

    // Root of template tree must have activity_id == activity.id.
    if root.activity_id != Some(activity.id) {
        return Err(DomainError::Consistency(
            "activity template must have a root entry with activity_id == activity.id".to_string(),
        ));
    }

    // All templates must have is_template == true.
    if !action.template.iter().all(|e| e.is_template) {
        return Err(DomainError::Consistency(
            "all template entries must have is_template == true".to_string(),
        ));
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
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not authorized to create entry for owner '{}' in parent entry '{:?}'",
            action.actor_id,
            action.entry.owner_id,
            action.entry.position.map(|p| p.parent_id)
        )));
    }

    // Check if referenced activity exists.
    if let Some(activity_id) = action.entry.activity_id {
        if executor
            .execute(FindActivityById { id: activity_id })
            .await?
            .is_none()
        {
            return Err(DomainError::Other(format!(
                "create entry failed, activity '{}' not found",
                activity_id
            )));
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
                return Err(DomainError::Consistency(
                    "child entry must match its parent's template/log kind".to_string(),
                ));
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
            DomainError::Other(format!("activity '{}' not found", action.activity_id))
        })?;

    // Only the owner can instantiate their activities (for now).
    if action.actor_id != activity.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not authorized to instantiate activity owned by '{}'",
            action.actor_id, activity.owner_id
        )));
    }

    // Placement validation mirrors move_entry, applied to the instantiated root.
    if let Some(position) = &action.position {
        let parent = executor
            .execute(FindEntryById {
                entry_id: position.parent_id,
            })
            .await?
            .ok_or_else(|| {
                DomainError::Consistency("instantiation parent does not exist".to_string())
            })?;
        if !parent.is_sequence {
            return Err(DomainError::Consistency(
                "cannot instantiate into a non-sequence entry".to_string(),
            ));
        }
        // A template tree and a log tree never mix: the instantiated subtree's
        // kind must match the parent's (log into log, template into template).
        if parent.is_template != action.is_template {
            return Err(DomainError::Consistency(
                "instantiated subtree must match its parent's template/log kind".to_string(),
            ));
        }
    } else if !action.is_template
        && action.temporal.start().is_none()
        && action.temporal.end().is_none()
    {
        // Log roots must be placed on the timeline; template roots are exempt.
        return Err(DomainError::Consistency(
            "root entry must have defined start or end time".to_string(),
        ));
    }

    // Template entries (root or child) never carry a start or end.
    validate_template_temporal(action.is_template, &action.temporal)?;

    let root = executor
        .execute(FindActivityTemplateRoot {
            activity_id: action.activity_id,
        })
        .await?
        .ok_or_else(|| {
            DomainError::Consistency(format!(
                // TODO: this condition is hit a lot when running arbitrary actions.
                "activity '{}' has no template root",
                action.activity_id
            ))
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
    // Moving entry should exist.
    let Some(entry) = executor
        .execute(FindEntryById {
            entry_id: action.entry_id,
        })
        .await?
    else {
        return Err(DomainError::Consistency(
            "entry that does not exist cannot be moved".to_string(),
        ));
    };

    // Root template entries are not allowed to move.
    // TODO: this isn't quite true - root entries aren't allowed to change positions, but they are
    // allowed to change their durations. Need to fix this.
    if entry.is_template && entry.position.is_none() {
        return Err(DomainError::Consistency(
            "root template entries cannot be moved".to_string(),
        ));
    }

    // TODO: Template entries cannot be moved to root.
    // TODO: Template entries cannot be moved outside template tree. This would cover the above,
    // since root is not part of the tree.

    if let Some(position) = &action.position {
        // Check for cycles.
        let parent_ancestors: Vec<Uuid> = executor
            .execute(FindAncestors {
                entry_id: position.parent_id,
            })
            .await?;
        if parent_ancestors.contains(&action.entry_id) {
            return Err(DomainError::Consistency(
                "move_entry would create a cycle".to_string(),
            ));
        }

        let parent = executor
            .execute(FindEntryById {
                entry_id: position.parent_id,
            })
            .await?
            .expect("parent should exist after earlier condition");

        // Destination entry must be a sequence.
        if !parent.is_sequence {
            return Err(DomainError::Consistency(
                "cannot move entry into a non-sequence entry".to_string(),
            ));
        }

        // Check parent and child are both template or log entries
        if entry.is_template && !parent.is_template {
            return Err(DomainError::Consistency(
                "template entry cannot be a child of a log entry".to_string(),
            ));
        }
        if !entry.is_template && parent.is_template {
            return Err(DomainError::Consistency(
                "log entry cannot be a child of a template entry".to_string(),
            ));
        }
    } else if !entry.is_template {
        // Log entries at root must be placed on the timeline; templates are
        // exempt (they live outside the timeline).
        if action.temporal.start().is_none() && action.temporal.end().is_none() {
            return Err(DomainError::Consistency(
                "root entry must have defined start or end time".to_string(),
            ));
        }
    }

    // Template entries never carry a start or end, root or child.
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
        return Err(DomainError::Other(
            "delete_entry_recursive failed: entry not found in database".into(),
        ));
    };

    // Check if actor has permission to delete.
    if action.actor_id != root.owner_id {
        return Err(DomainError::Unauthorized(
            "delete_entry_recursive actor is not the owner of the deleting entry".to_string(),
        ));
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

pub async fn create_attribute(
    _executor: &mut impl AnyQueryExecutor,
    io: &dyn Io,
    action: CreateAttribute,
) -> Result<Mutation> {
    let attribute = action.attribute.clone();

    // Only the owner can create attributes for themselves (for now).
    if action.actor_id != attribute.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not authorized to create attributes for owner '{}'",
            action.actor_id, attribute.owner_id
        )));
    }

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
        return Err(DomainError::Consistency("entry does not exist".to_string()));
    };

    // Only the owner may complete their own entries.
    if action.actor_id != entry.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not the owner of entry '{}'",
            action.actor_id, entry.id
        )));
    }

    // Template entries represent activity definitions, not logged events.
    if entry.is_template {
        return Err(DomainError::Consistency(
            "template entries cannot be marked complete".to_string(),
        ));
    }

    // Sequence entries are containers; completion applies only to leaf entries.
    if entry.is_sequence {
        return Err(DomainError::Consistency(
            "sequence entries cannot be marked complete".to_string(),
        ));
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
        .ok_or_else(|| DomainError::Other(format!("entry '{}' not found", value.entry_id)))?;

    // Only the entry owner can create values on it.
    if action.actor_id != entry.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not authorized to create values on entry owned by '{}'",
            action.actor_id, entry.owner_id
        )));
    }

    // The attribute must exist and be owned by the same actor as the entry.
    let attribute = executor
        .execute(FindAttributeById {
            attribute_id: value.attribute_id,
        })
        .await?
        .ok_or_else(|| {
            DomainError::Other(format!("attribute '{}' not found", value.attribute_id))
        })?;

    if attribute.owner_id != entry.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "attribute owner '{}' does not match entry owner '{}'",
            attribute.owner_id, entry.owner_id
        )));
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
        .ok_or_else(|| DomainError::Other(format!("entry '{}' not found", action.entry_id)))?;

    // Only the entry owner can attach values to it.
    if action.actor_id != entry.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not authorized to attach values on entry owned by '{}'",
            action.actor_id, entry.owner_id
        )));
    }

    let attribute = executor
        .execute(FindAttributeById {
            attribute_id: action.attribute_id,
        })
        .await?
        .ok_or_else(|| {
            DomainError::Other(format!("attribute '{}' not found", action.attribute_id))
        })?;

    if attribute.owner_id != entry.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "attribute owner '{}' does not match entry owner '{}'",
            attribute.owner_id, entry.owner_id
        )));
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
            DomainError::Consistency(format!(
                "value exists for entry '{}' but the entry does not",
                action.entry_id
            ))
        })?;

    // Only the entry owner can detach values from it.
    if action.actor_id != entry.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not the owner of entry '{}'",
            action.actor_id, entry.id
        )));
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
        return Err(DomainError::Consistency("entry does not exist".to_string()));
    };

    if action.actor_id != entry.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not the owner of entry '{}'",
            action.actor_id, entry.id
        )));
    }

    if executor
        .execute(FindAttributeById {
            attribute_id: action.attribute_id,
        })
        .await?
        .is_none()
    {
        return Err(DomainError::Consistency(
            "attribute does not exist".to_string(),
        ));
    }

    let Some(old) = executor
        .execute(FindValueByKey {
            entry_id: action.entry_id,
            attribute_id: action.attribute_id,
        })
        .await?
    else {
        return Err(DomainError::Consistency(
            "value does not exist; use CreateValue before UpdateAttributeValue".to_string(),
        ));
    };

    let new = match action.field {
        ValueField::Plan => Value {
            plan: Some(action.value.clone()),
            ..old.clone()
        },
        ValueField::Actual => Value {
            actual: Some(action.value.clone()),
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
        return Err(DomainError::Consistency(
            "attribute does not exist".to_string(),
        ));
    };

    // Only the owner can modify their attributes.
    if action.actor_id != old.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not the owner of attribute '{}'",
            action.actor_id, old.id
        )));
    }

    let mut new = old.clone();
    match &action.change {
        AttributeChange::SetName(name) => new.name = name.clone(),
        AttributeChange::SetDescription(description) => new.description = description.clone(),
        AttributeChange::Numeric(change) => {
            let AttributeConfig::Numeric(cfg) = &mut new.config else {
                return Err(DomainError::AttributeMismatch);
            };
            match change {
                NumericChange::SetDefault(default) => {
                    if let Some(v) = default {
                        if cfg.integer && (!v.is_finite() || v.trunc() != *v) {
                            return Err(ValidationError::InvalidNumericConfig(format!(
                                "default ({v}) must be an integer"
                            ))
                            .into());
                        }
                        if let Some(min) = cfg.min {
                            if *v < min {
                                return Err(ValidationError::InvalidNumericConfig(format!(
                                    "default ({v}) is below min ({min})"
                                ))
                                .into());
                            }
                        }
                        if let Some(max) = cfg.max {
                            if *v > max {
                                return Err(ValidationError::InvalidNumericConfig(format!(
                                    "default ({v}) is above max ({max})"
                                ))
                                .into());
                            }
                        }
                    }
                    cfg.default = *default;
                }
            }
        }
        AttributeChange::Select(change) => {
            let AttributeConfig::Select(cfg) = &mut new.config else {
                return Err(DomainError::AttributeMismatch);
            };
            match change {
                SelectChange::SetDefault(default) => {
                    if let Some(s) = default {
                        if !cfg.options.contains(s) {
                            return Err(ValidationError::Other(format!(
                                "default '{s}' is not one of the select options"
                            ))
                            .into());
                        }
                    }
                    cfg.default = default.clone();
                }
            }
        }
        AttributeChange::Mass(change) => {
            let AttributeConfig::Mass(cfg) = &mut new.config else {
                return Err(DomainError::AttributeMismatch);
            };
            match change {
                MassChange::SetDefaultUnits(units) => {
                    // Dedupe while preserving order.
                    let mut deduped = Vec::with_capacity(units.len());
                    for unit in units {
                        if !deduped.contains(unit) {
                            deduped.push(unit.clone());
                        }
                    }
                    cfg.default_units = deduped;
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
        return Err(DomainError::Consistency("entry does not exist".to_string()));
    };

    if action.actor_id != entry.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not the owner of entry '{}'",
            action.actor_id, entry.id
        )));
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
    }

    Ok(Mutation {
        id: io.uuid(),
        timestamp: io.current_time_wall_clock(),
        action: Action::UpdateEntry(action),
        changes: deltas,
    })
}
