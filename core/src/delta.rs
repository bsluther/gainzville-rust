use crate::models::{
    activity::Activity,
    actor::Actor,
    attribute::{Attribute, Value},
    entry::Entry,
    user::User,
};

#[derive(Debug, Clone)]
pub enum Delta<M> {
    Insert { new: M },
    Update { old: M, new: M },
    Delete { old: M },
}

// If we need to the PK for sync logic, this would be a good place to implement. Need an Id type of
// some sort to represent both scalar and composite keys (values have a composite PK).
#[derive(Debug, Clone)]
pub enum ModelDelta {
    User(Delta<User>),
    Actor(Delta<Actor>),
    Activity(Delta<Activity>),
    Entry(Delta<Entry>),
    Attribute(Delta<Attribute>),
    Value(Delta<Value>),
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
impl From<Delta<Attribute>> for ModelDelta {
    fn from(d: Delta<Attribute>) -> Self {
        ModelDelta::Attribute(d)
    }
}
impl From<Delta<Value>> for ModelDelta {
    fn from(d: Delta<Value>) -> Self {
        ModelDelta::Value(d)
    }
}
