use chrono::{DateTime, Utc};
use std::fmt::Display;
use uuid::Uuid;

use crate::core::model::Model;

pub enum ActorKind {
    System,
    User,
}

impl Display for ActorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActorKind::System => write!(f, "system"),
            ActorKind::User => write!(f, "user"),
        }
    }
}

pub struct Actor {
    pub actor_id: Uuid,
    pub actor_kind: ActorKind,
    pub created_at: DateTime<Utc>,
}

pub struct ActorPatch {}

impl Model for Actor {
    const MODEL_NAME: &'static str = "actor";
    const PRIMARY_KEY: &'static str = "actor_id";
    type Patch = ActorPatch;
}
