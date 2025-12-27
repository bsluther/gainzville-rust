use crate::core::models::{activity::Activity, actor::Actor, user::User};
use uuid::Uuid;

#[derive(Debug)]
pub enum Delta<M> {
    Insert { id: Uuid, new: M },
    Update { id: Uuid, old: M, new: M },
    Delete { id: Uuid, old: M },
}

#[derive(Debug)]
pub enum ModelDelta {
    User(Delta<User>),
    Actor(Delta<Actor>),
    Activity(Delta<Activity>),
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
