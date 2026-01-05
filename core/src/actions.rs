use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    delta::{Delta, ModelDelta},
    error::{DomainError, Result},
    models::{
        activity::Activity,
        actor::{Actor, ActorKind},
        entry::Entry,
        user::User,
    },
    repos::{ActivityRepo, AuthnRepo},
};

/*
Decisions
- Do users duplicate activites added from another library?
    - Always consider the sequence case first, it's more complcated.
    - Should profiles be separate from activites? Again, consider the sequence case...
*/

/*
Features to add:
- Time
    - Attribute or built-in
- Sets
- Attributes
- Categories
- Permissions
*/

/*
Properties to test:
- Forest (acyclic)
*/

/*
Actions to add:
    MoveEntry
    - Check template and log entires are disjoint.
    - Check acyclic.
    CreateEntryFromTemplate
    - Or should the client do the look-up, and just CreateEntry?
    CreateActivityTemplate
    - Or should each activity automatically have a template?
    CreateAttribute
    AddValueToEntry

*/

#[derive(Debug, Clone)]
pub enum Action {
    CreateUser(CreateUser),
    CreateActivity(CreateActivity),
    CreateEntry(CreateEntry),
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

// TODO: relocate.
#[derive(Debug, Clone)]
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
}
