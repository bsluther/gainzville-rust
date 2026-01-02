use chrono::{DateTime, Utc};
use std::fmt::Display;
use uuid::Uuid;

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct Actor {
    pub actor_id: Uuid,
    pub actor_kind: ActorKind,
    pub created_at: DateTime<Utc>,
}
