use chrono::{DateTime, Utc};
use sqlx::{Database, Transaction};
use tracing::debug;
use uuid::Uuid;

use crate::{
    actions::{
        Action, CreateActivity, CreateAttribute, CreateEntry, CreateUser, CreateValue,
        DeleteEntryRecursive, MoveEntry, UpdateAttributeValue, UpdateEntryCompletion, ValueField,
    },
    delta::{Delta, ModelDelta},
    error::{DomainError, Result},
    models::{
        actor::{Actor, ActorKind},
        attribute::Value,
        user::User,
    },
    reader::Reader,
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
    pub changes: Vec<ModelDelta>,
}

pub async fn create_user<'t, DB, R>(
    tx: &mut Transaction<'t, DB>,
    action: CreateUser,
) -> Result<Mutation>
where
    DB: Database,
    R: Reader<DB>,
{
    let user = action.user;
    // Check if email is already registered.
    if R::is_email_registered(&mut **tx, user.email.clone()).await? {
        return Err(DomainError::EmailAlreadyExists);
    }

    // Check if username is in use.
    if R::find_user_by_username(&mut **tx, user.username.clone())
        .await?
        .is_some()
    {
        return Err(DomainError::Other("user already in use".to_string()));
    }

    // Check if ID is in use.
    if R::find_user_by_id(&mut **tx, user.actor_id)
        .await?
        .is_some()
    {
        return Err(DomainError::Other("actor_id already in use".to_string()));
    }

    // Create user and actor insert deltas.
    let insert_actor = Delta::<Actor>::Insert {
        new: Actor {
            actor_id: user.actor_id,
            actor_kind: ActorKind::User,
            created_at: chrono::Utc::now(),
        },
    };
    let insert_user = Delta::<User>::Insert { new: user.clone() };

    // Create mutation.
    // - Might make more sense just to return deltas from here, we'll see.
    Ok(Mutation {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        action: Action::CreateUser(CreateUser { user }),
        changes: vec![insert_actor.into(), insert_user.into()],
    })
}

pub async fn create_activity<'t, DB, R>(
    tx: &mut Transaction<'t, DB>,
    action: CreateActivity,
) -> Result<Mutation>
where
    DB: Database,
    R: Reader<DB>,
{
    let _all_activities = R::all_activities(&mut **tx);
    let activity = action.activity.clone();
    // Check if actor has permission to create activities for owner.
    // For now, only allow if actor == owner.
    if action.actor_id != activity.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not authorized to create activities for owner '{}'",
            action.actor_id, activity.owner_id
        )));
    }

    let insert_activity = Delta::Insert { new: activity };

    Ok(Mutation {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        action: Action::CreateActivity(action.clone()),
        changes: vec![insert_activity.into()],
    })
}

pub async fn create_entry<'t, DB, R>(
    tx: &mut Transaction<'t, DB>,
    action: CreateEntry,
) -> Result<Mutation>
where
    DB: Database,
    R: Reader<DB>,
{
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

    // Check if referenced activity exists,
    if let Some(activity_id) = action.entry.activity_id {
        if R::find_activity_by_id(&mut **tx, activity_id)
            .await?
            .is_none()
        {
            return Err(DomainError::Other(format!(
                "create entry failed, activity '{}' not found",
                activity_id
            )));
        }
    };

    let insert_entry = Delta::Insert {
        new: action.entry.clone(),
    };

    Ok(Mutation {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        action: Action::CreateEntry(action),
        changes: vec![insert_entry.into()],
    })
}

/// Move an entry by changing it's parent, fractional index, and temporal. Does not allow
/// moving to root without a defined start or end time; while the model allows for this, it
/// should be intentional and utilize a different action.
pub async fn move_entry<'t, DB, R>(
    tx: &mut Transaction<'t, DB>,
    action: MoveEntry,
) -> Result<Mutation>
where
    DB: Database,
    R: Reader<DB>,
{
    // Moving entry should exist.
    let Some(entry) = R::find_entry_by_id(&mut **tx, action.entry_id).await? else {
        return Err(DomainError::Consistency(
            "entry that does not exist cannot be moved".to_string(),
        ));
    };

    if let Some(position) = &action.position {
        // Check for cycles.
        let parent_ancestors: Vec<Uuid> = R::find_ancestors(&mut **tx, position.parent_id).await?;
        if parent_ancestors.contains(&action.entry_id) {
            return Err(DomainError::Consistency(
                "move_entry would create a cycle".to_string(),
            ));
        }

        let parent = R::find_entry_by_id(&mut **tx, action.entry_id)
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
    } else {
        // Moving to root, check that start or end time is defined.
        if action.temporal.start().is_none() && action.temporal.end().is_none() {
            return Err(DomainError::Consistency(
                "root entry must have defined start or end time".to_string(),
            ));
        }
    }

    let update_delta = entry
        .update()
        .position(action.position.clone())
        .temporal(action.temporal.clone())
        .to_delta();

    Ok(Mutation {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        action: Action::MoveEntry(action),
        changes: vec![update_delta.into()],
    })
}

pub async fn delete_entry_recursive<'t, DB, R>(
    tx: &mut Transaction<'t, DB>,
    action: DeleteEntryRecursive,
) -> Result<Mutation>
where
    DB: Database,
    R: Reader<DB>,
{
    // Get entry and all descendants.
    // - Once attributes are in place, will need get/delete them as well.
    let subtree = R::find_descendants(&mut **tx, action.entry_id).await?;
    let subtree_ids: Vec<Uuid> = subtree.iter().map(|e| e.id).collect();
    let subtree_attr_values = R::find_values_for_entries(&mut **tx, &subtree_ids).await?;

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
    let entry_deltas: Vec<ModelDelta> = subtree
        .into_iter()
        .map(|e| Delta::Delete { old: e }.into())
        .collect();

    // Create delete deltas for entry and descedants attribute values.
    let attr_value_deltas: Vec<ModelDelta> = subtree_attr_values
        .into_iter()
        .map(|v| Delta::Delete { old: v }.into())
        .collect();

    let mut deltas = entry_deltas;
    deltas.extend(attr_value_deltas);

    Ok(Mutation {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        action: action.into(),
        changes: deltas,
    })
}

pub async fn create_attribute<'t, DB, R>(
    _tx: &mut Transaction<'t, DB>,
    action: CreateAttribute,
) -> Result<Mutation>
where
    DB: Database,
    R: Reader<DB>,
{
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
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        action: Action::CreateAttribute(action),
        changes: vec![insert_attribute.into()],
    })
}

pub async fn update_entry_completion<'t, DB, R>(
    tx: &mut Transaction<'t, DB>,
    action: UpdateEntryCompletion,
) -> Result<Mutation>
where
    DB: Database,
    R: Reader<DB>,
{
    let Some(entry) = R::find_entry_by_id(&mut **tx, action.entry_id).await? else {
        return Err(DomainError::Consistency(
            "entry does not exist".to_string(),
        ));
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

    let update_delta = entry
        .update()
        .is_complete(action.is_complete)
        .to_delta();

    Ok(Mutation {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        action: Action::UpdateEntryCompletion(action),
        changes: vec![update_delta.into()],
    })
}

pub async fn create_value<'t, DB, R>(
    tx: &mut Transaction<'t, DB>,
    action: CreateValue,
) -> Result<Mutation>
where
    DB: Database,
    R: Reader<DB>,
{
    let value = action.value.clone();

    // The entry must exist.
    let entry = R::find_entry_by_id(&mut **tx, value.entry_id)
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
    let attribute = R::find_attribute_by_id(&mut **tx, value.attribute_id)
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

    let insert_value = Delta::Insert { new: value };

    Ok(Mutation {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        action: Action::CreateValue(action),
        changes: vec![insert_value.into()],
    })
}

pub async fn update_attribute_value<'t, DB, R>(
    tx: &mut Transaction<'t, DB>,
    action: UpdateAttributeValue,
) -> Result<Mutation>
where
    DB: Database,
    R: Reader<DB>,
{
    let Some(entry) = R::find_entry_by_id(&mut **tx, action.entry_id).await? else {
        return Err(DomainError::Consistency("entry does not exist".to_string()));
    };

    if action.actor_id != entry.owner_id {
        return Err(DomainError::Unauthorized(format!(
            "actor '{}' is not the owner of entry '{}'",
            action.actor_id, entry.id
        )));
    }

    if R::find_attribute_by_id(&mut **tx, action.attribute_id)
        .await?
        .is_none()
    {
        return Err(DomainError::Consistency(
            "attribute does not exist".to_string(),
        ));
    }

    let Some(old) = R::find_value_by_key(&mut **tx, action.entry_id, action.attribute_id).await?
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
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        action: Action::UpdateAttributeValue(action),
        changes: vec![Delta::<Value>::Update { old, new }.into()],
    })
}
