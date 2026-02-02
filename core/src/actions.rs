use uuid::Uuid;

use crate::models::{
    activity::Activity,
    entry::{Entry, Position, Temporal},
    user::User,
};

// TODO: consider adding more structure to the action.
// Action { actor_id, data: { ... }}

#[derive(Debug, Clone)]
pub enum Action {
    CreateUser(CreateUser),
    CreateActivity(CreateActivity),
    CreateEntry(CreateEntry),
    DeleteEntryRecursive(DeleteEntryRecursive),
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

impl From<MoveEntry> for Action {
    fn from(value: MoveEntry) -> Self {
        Action::MoveEntry(value)
    }
}

impl From<DeleteEntryRecursive> for Action {
    fn from(value: DeleteEntryRecursive) -> Self {
        Action::DeleteEntryRecursive(value)
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

impl From<User> for CreateUser {
    fn from(user: User) -> Self {
        CreateUser { user: user }
    }
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

#[derive(Debug, Clone)]
pub struct DeleteEntryRecursive {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
}
