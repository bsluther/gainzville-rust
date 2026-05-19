use chrono::{DateTime, Utc};
use gv_core::{
    SYSTEM_ACTOR_ID,
    delta::{AnyDelta, Delta},
    delta_executor::AnyDeltaExecutor,
    models::{
        activity::{Activity, ActivityName},
        actor::{Actor, ActorKind},
        attribute::{Attribute, Value},
        entry::Entry,
        user::User,
    },
    std_lib::StandardLibrary,
};
use rustc_hash::FxHashMap as HashMap;
use std::hash::Hash;
use uuid::Uuid;

pub struct Model {
    pub actors: HashMap<Uuid, Actor>,
    pub users: HashMap<Uuid, User>,
    pub entries: HashMap<Uuid, Entry>,
    pub activities: HashMap<Uuid, Activity>,
    pub attributes: HashMap<Uuid, Attribute>,
    pub values: HashMap<ValuePrimaryKey, Value>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ValuePrimaryKey {
    entry_id: Uuid,
    attribute_id: Uuid,
}

impl Model {
    pub fn empty() -> Self {
        Model {
            actors: HashMap::default(),
            users: HashMap::default(),
            entries: HashMap::default(),
            activities: HashMap::default(),
            attributes: HashMap::default(),
            values: HashMap::default(),
        }
    }
}

impl Model {
    pub async fn seed_basic(&mut self) -> gv_core::error::Result<()> {
        let actors_seed = vec![Actor {
            actor_id: SYSTEM_ACTOR_ID,
            actor_kind: ActorKind::System,
            created_at: "2026-01-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap(),
        }];
        let activities_seed = vec![Activity {
            id: Uuid::new_v4(),
            owner_id: SYSTEM_ACTOR_ID,
            source_activity_id: None,
            name: ActivityName::parse("Pull Ups".to_string()).unwrap(),
            description: None,
        }];
        let attributes_seed = StandardLibrary::attributes();

        for actor in actors_seed {
            self.apply_any_delta(Delta::insert(actor).into()).await?;
        }
        for activity in activities_seed {
            self.apply_any_delta(Delta::insert(activity).into()).await?;
        }
        for attribute in attributes_seed {
            self.apply_any_delta(Delta::insert(attribute).into())
                .await?;
        }

        Ok(())
    }
}

trait Entity: PartialEq {
    type PrimaryKey: Hash + Eq;

    fn primary_key(&self) -> Self::PrimaryKey;
    fn kind(&self) -> &'static str;
}

impl Entity for Actor {
    type PrimaryKey = Uuid;
    fn primary_key(&self) -> Uuid {
        self.actor_id
    }
    fn kind(&self) -> &'static str {
        "actor"
    }
}

impl Entity for User {
    type PrimaryKey = Uuid;
    fn primary_key(&self) -> Uuid {
        self.actor_id
    }
    fn kind(&self) -> &'static str {
        "user"
    }
}

impl Entity for Entry {
    type PrimaryKey = Uuid;
    fn primary_key(&self) -> Uuid {
        self.id
    }
    fn kind(&self) -> &'static str {
        "entry"
    }
}

impl Entity for Activity {
    type PrimaryKey = Uuid;
    fn primary_key(&self) -> Uuid {
        self.id
    }
    fn kind(&self) -> &'static str {
        "activity"
    }
}

impl Entity for Attribute {
    type PrimaryKey = Uuid;
    fn primary_key(&self) -> Uuid {
        self.id
    }
    fn kind(&self) -> &'static str {
        "attribute"
    }
}

impl Entity for Value {
    type PrimaryKey = ValuePrimaryKey;
    fn primary_key(&self) -> Self::PrimaryKey {
        ValuePrimaryKey {
            entry_id: self.entry_id,
            attribute_id: self.attribute_id,
        }
    }
    fn kind(&self) -> &'static str {
        "value"
    }
}

pub struct ModelDeltaExecutor {}
impl AnyDeltaExecutor for Model {
    async fn apply_any_delta(&mut self, delta: AnyDelta) -> gv_core::error::Result<()> {
        match delta {
            AnyDelta::Actor(delta) => hash_map_apply(delta, &mut self.actors),
            AnyDelta::User(delta) => hash_map_apply(delta, &mut self.users),
            AnyDelta::Activity(delta) => hash_map_apply(delta, &mut self.activities),
            AnyDelta::Entry(delta) => hash_map_apply(delta, &mut self.entries),
            AnyDelta::Attribute(delta) => hash_map_apply(delta, &mut self.attributes),
            AnyDelta::Value(delta) => hash_map_apply(delta, &mut self.values),
        }
        Ok(())
    }
}
fn hash_map_apply<E: Entity>(delta: Delta<E>, map: &mut HashMap<E::PrimaryKey, E>) {
    match delta {
        Delta::Insert { new } => {
            assert!(
                !map.contains_key(&new.primary_key()),
                "tried to insert {} that already exists",
                new.kind()
            );
            map.insert(new.primary_key(), new);
        }
        Delta::Update { old, new } => {
            assert!(
                old.primary_key() == new.primary_key(),
                "update old and new must share a primary key"
            );
            let existing = map.get(&new.primary_key());
            assert!(
                existing.is_some(),
                "tried to update {} that does not exist",
                old.kind()
            );
            assert!(
                existing.unwrap() == &old,
                "update delta's old value does not match existing"
            );
            map.insert(new.primary_key(), new);
        }
        Delta::Delete { old } => {
            if let Some(existing) = map.get(&old.primary_key()) {
                assert!(
                    existing == &old,
                    "existng value must match delete delta's old value"
                );
                map.remove(&old.primary_key());
            } else {
                assert!(
                    false,
                    "tried to apply delete delta for entity that does not exist"
                );
            }
        }
    };
}
