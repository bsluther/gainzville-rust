use std::ops::RangeBounds;

use chrono::{DateTime, Utc};
use sqlx::{Database, Executor};
use uuid::Uuid;

use crate::{
    delta::{Delta, ModelDelta},
    error::{DomainError, Result},
    models::{
        activity::Activity,
        actor::{Actor, ActorKind},
        entry::{Entry, Position, Temporal},
        user::User,
    },
    repos::{ActivityRepo, ActivityRepo2, AuthnRepo, EntryRepo},
};

#[derive(Debug, Clone)]
pub enum Action {
    CreateUser(CreateUser),
    CreateActivity(CreateActivity),
    CreateEntry(CreateEntry),
    MoveEntry(MoveEntry),
}

impl From<CreateUser> for Action {
    fn from(value: CreateUser) -> Self {
        Action::CreateUser(value)
    }
}

impl From<CreateActivity> for Action {
    fn from(value: CreateActivity) -> Self {
        Action::CreateActivity(value)
    }
}

impl From<CreateEntry> for Action {
    fn from(value: CreateEntry) -> Self {
        Action::CreateEntry(value)
    }
}

#[derive(Debug, Clone)]
pub struct CreateActivity {
    pub actor_id: Uuid,
    pub activity: Activity,
}

impl From<Activity> for CreateActivity {
    fn from(activity: Activity) -> Self {
        CreateActivity {
            actor_id: activity.owner_id,
            activity,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateUser {
    pub user: User,
}

#[derive(Debug, Clone)]
pub struct CreateEntry {
    pub actor_id: Uuid,
    pub entry: Entry,
}

impl From<Entry> for CreateEntry {
    fn from(entry: Entry) -> Self {
        CreateEntry {
            actor_id: entry.owner_id,
            entry: entry,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MoveEntry {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub position: Option<Position>,
    pub temporal: Temporal,
}

// TODO: relocate.
#[derive(Debug, Clone)]
pub struct Mutation {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub action: Action,
    pub changes: Vec<ModelDelta>,
}

pub struct ActionService {}

// Note: the naming of the "mutators" defined here is a bit confusing. create_user for example
// does *not* write to the database, it just returns the Mutation which can be written to the db.
// But create_user *does* read from the database, which is why it takes a repo (or executor in the
// refactored version) - that's what repo methods take.
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
        let activity = action.activity.clone();
        // Check if actor has permission to create activities for owner.
        // For now, only allow if actor == owner.
        if action.actor_id != activity.owner_id {
            return Err(DomainError::Unauthorized(format!(
                "actor '{}' is not authorized to create activities for owner '{}'",
                action.actor_id, activity.owner_id
            )));
        }

        let insert_activity = Delta::Insert {
            id: activity.id,
            new: activity,
        };

        Ok(Mutation {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            action: Action::CreateActivity(action.clone()),
            changes: vec![insert_activity.into()],
        })
    }

    pub async fn create_activity2<'e, E, DB: sqlx::Database>(
        executor: E,
        _repo: impl ActivityRepo2<DB>,
        action: CreateActivity,
    ) -> Result<Mutation>
    where
        E: Executor<'e, Database = DB>,
    {
        let activity = action.activity.clone();
        // Check if actor has permission to create activities for owner.
        // For now, only allow if actor == owner.
        if action.actor_id != activity.owner_id {
            return Err(DomainError::Unauthorized(format!(
                "actor '{}' is not authorized to create activities for owner '{}'",
                action.actor_id, activity.owner_id
            )));
        }

        let insert_activity = Delta::Insert {
            id: activity.id,
            new: activity,
        };

        Ok(Mutation {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            action: Action::CreateActivity(action.clone()),
            changes: vec![insert_activity.into()],
        })
    }

    pub async fn create_entry(mut ctx: impl ActivityRepo, action: CreateEntry) -> Result<Mutation> {
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
            if ctx.find_activity_by_id(activity_id).await?.is_none() {
                return Err(DomainError::Other(format!(
                    "create entry failed, activity '{}' not found",
                    activity_id
                )));
            }
        };

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

    /// Move an entry by changing it's parent, fractional index, and temporal. Does not allow
    /// moving to root without a defined start or end time; while the model allows for this, it
    /// should be intentional and utilize a different action.
    pub async fn move_entry(mut ctx: impl EntryRepo, action: MoveEntry) -> Result<Mutation> {
        // Moving entry should exist.
        let Some(entry) = ctx.find_entry_by_id(action.entry_id).await? else {
            return Err(DomainError::Consistency(
                "entry that does not exist cannot be moved".to_string(),
            ));
        };

        if let Some(position) = &action.position {
            // Check for cycles.
            let parent_ancestors: Vec<Uuid> = ctx.find_ancestors(position.parent_id).await?;
            if parent_ancestors.contains(&action.entry_id) {
                return Err(DomainError::Consistency(
                    "move_entry would create a cycle".to_string(),
                ));
            }

            let parent = ctx
                .find_entry_by_id(action.entry_id)
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
}
