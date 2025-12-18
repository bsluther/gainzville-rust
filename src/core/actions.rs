use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::core::{
    delta::Delta,
    delta::ModelDelta,
    error::{DomainError, Result},
    models::{
        actor::{Actor, ActorKind},
        user::User,
    },
    repos::AuthnRepo,
    validation::{Email, Username},
};

pub enum Action {
    CreateUser(User),
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
    /// authn_repo contains the DB transaction we're operating in.
    pub async fn create_user(mut ctx: impl AuthnRepo, user: User) -> Result<Mutation> {
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

        // Create user, actor insert deltas.
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
            action: Action::CreateUser(user),
            changes: vec![insert_actor.into(), insert_user.into()],
        })
    }
}
