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

impl<T> Delta<T> {
    pub fn insert(new: T) -> Self {
        Delta::Insert { new }
    }
}

// If we need to the PK for sync logic, this would be a good place to implement. Need an Id type of
// some sort to represent both scalar and composite keys (values have a composite PK).
#[derive(Debug, Clone)]
pub enum AnyDelta {
    User(Delta<User>),
    Actor(Delta<Actor>),
    Activity(Delta<Activity>),
    Entry(Delta<Entry>),
    Attribute(Delta<Attribute>),
    Value(Delta<Value>),
}

/// Convert Delta<T> --> AnyDelta::T.
impl From<Delta<User>> for AnyDelta {
    fn from(d: Delta<User>) -> Self {
        AnyDelta::User(d)
    }
}
impl From<Delta<Actor>> for AnyDelta {
    fn from(d: Delta<Actor>) -> Self {
        AnyDelta::Actor(d)
    }
}
impl From<Delta<Activity>> for AnyDelta {
    fn from(d: Delta<Activity>) -> Self {
        AnyDelta::Activity(d)
    }
}
impl From<Delta<Entry>> for AnyDelta {
    fn from(d: Delta<Entry>) -> Self {
        AnyDelta::Entry(d)
    }
}
impl From<Delta<Attribute>> for AnyDelta {
    fn from(d: Delta<Attribute>) -> Self {
        AnyDelta::Attribute(d)
    }
}
impl From<Delta<Value>> for AnyDelta {
    fn from(d: Delta<Value>) -> Self {
        AnyDelta::Value(d)
    }
}
