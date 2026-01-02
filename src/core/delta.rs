use crate::core::models::{activity::Activity, actor::Actor, entry::Entry, user::User};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Delta<M> {
    Insert { id: Uuid, new: M },
    Update { id: Uuid, old: M, new: M },
    Delete { id: Uuid, old: M },
}

#[derive(Debug, Clone)]
pub enum ModelDelta {
    User(Delta<User>),
    Actor(Delta<Actor>),
    Activity(Delta<Activity>),
    Entry(Delta<Entry>),
}

/// Convert Delta<T> --> ModelDelta::T.
impl From<Delta<User>> for ModelDelta {
    fn from(d: Delta<User>) -> Self {
        ModelDelta::User(d)
    }
}
impl From<Delta<Actor>> for ModelDelta {
    fn from(d: Delta<Actor>) -> Self {
        ModelDelta::Actor(d)
    }
}
impl From<Delta<Activity>> for ModelDelta {
    fn from(d: Delta<Activity>) -> Self {
        ModelDelta::Activity(d)
    }
}
impl From<Delta<Entry>> for ModelDelta {
    fn from(d: Delta<Entry>) -> Self {
        ModelDelta::Entry(d)
    }
}
