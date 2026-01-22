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
