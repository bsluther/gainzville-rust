use crate::core::{
    model::Model,
    models::{actor::Actor, user::User},
};
use uuid::Uuid;

pub enum Delta<M: Model> {
    Insert {
        id: Uuid,
        new: M,
    },

    Update {
        id: Uuid,
        old: M::Patch,
        new: M::Patch,
    },

    Delete {
        id: Uuid,
        old: M,
    },
}

pub enum ModelDelta {
    User(Delta<User>),
    Actor(Delta<Actor>),
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
