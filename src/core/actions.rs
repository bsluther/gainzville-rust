use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::core::{
    delta::{Delta, ModelDelta},
    error::{DomainError, Result},
    models::{
        activity::{Activity, ActivityName},
        actor::{Actor, ActorKind},
        entry::Entry,
        user::User,
    },
    repos::{ActivityRepo, AuthnRepo, EntryRepo},
};

pub enum Action {
    CreateUser(CreateUser),
    CreateActivity(CreateActivity),
    CreateEntry(CreateEntry),
}

pub struct CreateActivity {
    pub actor_id: Uuid,
    pub owner_id: Uuid,
    pub activity_id: Uuid,
    pub name: ActivityName,
    pub description: Option<String>,
}

pub struct CreateUser {
    pub user: User,
}

// CreateEntryFromTemplate
// CreateTemplateEntry
pub struct CreateEntry {
    pub actor_id: Uuid,
    pub entry: Entry,
}

// TODO: relocate.
pub struct Mutation {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub action: Action,
    pub changes: Vec<ModelDelta>,
}

pub struct ActionService {}

impl ActionService {
    /// Action handler for CreateUser.
    /// ctx contains the DB transaction we're operating in.
    pub async fn create_user(mut ctx: impl AuthnRepo, action: CreateUser) -> Result<Mutation> {
        let user = action.user;
        // Check if email is already registered.
        if ctx.is_email_registered(user.email.clone()).await? {
            return Err(DomainError::EmailAlreadyExists);
        }

        // Check if username is in use.
        if ctx
            .find_user_by_username(user.username.clone())
            .await?
            .is_some()
        {
            return Err(DomainError::Other("user already in use".to_string()));
        }

        // Check if ID is in use.
        if ctx.find_user_by_id(user.actor_id).await?.is_some() {
            return Err(DomainError::Other("actor_id already in use".to_string()));
        }

        // Create user and actor insert deltas.
        let insert_actor = Delta::<Actor>::Insert {
            id: user.actor_id,
            new: Actor {
                actor_id: user.actor_id,
                actor_kind: ActorKind::User,
                created_at: chrono::Utc::now(),
            },
        };
        let insert_user = Delta::<User>::Insert {
            id: user.actor_id,
            new: user.clone(),
        };

        // Create mutation.
        // - Might make more sense just to return deltas from here, we'll see.
        Ok(Mutation {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            action: Action::CreateUser(CreateUser { user }),
            changes: vec![insert_actor.into(), insert_user.into()],
        })
    }

    pub async fn create_activity(
        mut ctx: impl AuthnRepo,
        action: CreateActivity,
    ) -> Result<Mutation> {
        // Check if actor has permission to create activities for owner.
        // For now, only allow if actor == owner.
        if action.actor_id != action.owner_id {
            return Err(DomainError::Unauthorized(format!(
                "actor '{}' is not authorized to create activities for owner '{}'",
                action.actor_id, action.owner_id
            )));
        }

        let insert_activity = Delta::Insert {
            id: action.activity_id,
            new: Activity {
                id: action.activity_id.clone(),
                owner_id: action.owner_id.clone(),
                source_activity_id: None,
                name: action.name.clone(),
                description: action.description.clone(),
            },
        };

        Ok(Mutation {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            action: Action::CreateActivity(action),
            changes: vec![insert_activity.into()],
        })
    }

    pub async fn create_entry(mut ctx: impl ActivityRepo, action: CreateEntry) -> Result<Mutation> {
        // Check if actor has permission to create entry at the given position.
        // For now, only allow the owner to create.
        if action.actor_id != action.entry.owner_id {
            return Err(DomainError::Unauthorized(format!(
                "actor '{}' is not authorized to create entry for owner '{}' in parent entry '{:?}'",
                action.actor_id, action.entry.owner_id, action.entry.parent_id
            )));
        }

        // Check if referenced activity exists,
        if let Some(activity_id) = action.entry.activity_id {
            if ctx.find_activity_by_id(activity_id).await?.is_none() {
                return Err(DomainError::Other(format!(
                    "create entry failed, activity '{}' not found",
                    activity_id
                )));
            }
        };

        // Check that position/parent_id are both none or both some
        if action.entry.parent_id.is_none() && action.entry.frac_index.is_some()
            || action.entry.parent_id.is_some() && action.entry.frac_index.is_none()
        {
            return Err(DomainError::Other(
                "create entry failed, parent_id and frac_index must both be Some or both be None"
                    .to_string(),
            ));
        }

        let insert_entry = Delta::Insert {
            id: action.entry.id,
            new: action.entry.clone(),
        };

        Ok(Mutation {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            action: Action::CreateEntry(action),
            changes: vec![insert_entry.into()],
        })
    }
}
