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
    mutators::Mutation,
    queries::Snapshot,
    std_lib::StandardLibrary,
};
use rustc_hash::FxHashMap as HashMap;
use std::{collections::HashSet, hash::Hash};
use tracing::error;
use uuid::Uuid;

#[derive(Debug, PartialEq)]
pub struct Model {
    actors: HashMap<Uuid, Actor>,
    users: HashMap<Uuid, User>,
    entries: HashMap<Uuid, Entry>,
    activities: HashMap<Uuid, Activity>,
    attributes: HashMap<Uuid, Attribute>,
    values: HashMap<ValuePrimaryKey, Value>,
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

    pub fn actors(&self) -> impl Iterator<Item = &Actor> {
        self.actors.values()
    }

    pub fn users(&self) -> impl Iterator<Item = &User> {
        self.users.values()
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    /// Look up a single entry by id.
    pub fn entry(&self, id: Uuid) -> Option<&Entry> {
        self.entries.get(&id)
    }

    pub fn activities(&self) -> impl Iterator<Item = &Activity> {
        self.activities.values()
    }

    pub fn attributes(&self) -> impl Iterator<Item = &Attribute> {
        self.attributes.values()
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.values.values()
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

    /// Build a fresh model from a full-database snapshot.
    pub fn from_snapshot(snapshot: Snapshot) -> Self {
        Model {
            actors: Self::map_of(snapshot.actors),
            users: Self::map_of(snapshot.users),
            activities: Self::map_of(snapshot.activities),
            attributes: Self::map_of(snapshot.attributes),
            entries: Self::map_of(snapshot.entries),
            values: Self::map_of(snapshot.values),
        }
    }

    /// Helper to build a hash map from a set of entities. Entities must all have distinct ID's.
    fn map_of<E: Entity>(items: Vec<E>) -> HashMap<E::PrimaryKey, E> {
        let mut map = HashMap::default();
        for item in items {
            let kind = item.kind();
            assert!(
                (map.insert(item.primary_key(), item)).is_none(),
                "duplicate {kind} primary key in snapshot"
            )
        }
        map
    }

    pub async fn apply_mutation(&mut self, mx: Mutation) -> gv_core::error::Result<()> {
        for delta in mx.changes.clone() {
            self.apply_any_delta(delta).await?;
        }
        if !self.all_activities_have_one_template_root() {
            error!(
                ?mx,
                "Property violated: all activities must have exactly one template root."
            );
        }
        Ok(())
    }

    pub fn all_activities_have_one_template_root(&self) -> bool {
        let mut seen = HashSet::new();
        let unique = self
            .entries()
            .filter(|e| e.is_template && e.parent_id().is_none())
            .filter_map(|e| e.activity_id) // TODO: activity_id being None here is a violation of another property, all root templates must correspond to an activity.
            .all(|activity_id| seen.insert(activity_id));
        unique && self.activities().all(|a| seen.contains(&a.id))
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

impl Model {
    /// Apply a delta to the in-memory model synchronously. The model is pure
    /// in-memory state with no IO, so this can't fail — invariant breaches panic
    /// rather than return an error. The async `AnyDeltaExecutor::apply_any_delta`
    /// trait method delegates here; sync callers can use this directly to avoid
    /// the async tax the DB-backed executors require.
    pub fn apply_any_delta_sync(&mut self, delta: AnyDelta) {
        match delta {
            AnyDelta::Actor(delta) => hash_map_apply(delta, &mut self.actors),
            AnyDelta::User(delta) => hash_map_apply(delta, &mut self.users),
            AnyDelta::Activity(delta) => hash_map_apply(delta, &mut self.activities),
            AnyDelta::Entry(delta) => hash_map_apply(delta, &mut self.entries),
            AnyDelta::Attribute(delta) => hash_map_apply(delta, &mut self.attributes),
            AnyDelta::Value(delta) => hash_map_apply(delta, &mut self.values),
        }
    }
}

impl AnyDeltaExecutor for Model {
    async fn apply_any_delta(&mut self, delta: AnyDelta) -> gv_core::error::Result<()> {
        self.apply_any_delta_sync(delta);
        Ok(())
    }
}

fn hash_map_apply<E: Entity + std::fmt::Debug>(
    delta: Delta<E>,
    map: &mut HashMap<E::PrimaryKey, E>,
) {
    match delta {
        Delta::Insert { new } => {
            if let Some(existing) = map.get(&new.primary_key()) {
                panic!(
                    "tried to insert {} that already exists:\n  \
                     new:       {:?}\n  model has: {:?}",
                    new.kind(),
                    new,
                    existing,
                );
            }
            map.insert(new.primary_key(), new);
        }
        Delta::Update { old, new } => {
            assert!(
                old.primary_key() == new.primary_key(),
                "update old and new must share a primary key for {}:\n  \
                 old: {:?}\n  new: {:?}",
                old.kind(),
                old,
                new,
            );
            let existing = map.get(&new.primary_key());
            assert!(
                existing.is_some(),
                "tried to update {} that does not exist:\n  delta.old: {:?}",
                old.kind(),
                old,
            );
            assert!(
                existing.unwrap() == &old,
                "update delta's old value does not match existing {}:\n  \
                 delta.old: {:?}\n  model has: {:?}",
                old.kind(),
                old,
                existing.unwrap(),
            );
            map.insert(new.primary_key(), new);
        }
        Delta::Delete { old } => match map.get(&old.primary_key()) {
            Some(existing) => {
                assert!(
                    existing == &old,
                    "delete delta's old value does not match existing {}:\n  \
                     delta.old: {:?}\n  model has: {:?}",
                    old.kind(),
                    old,
                    existing,
                );
                map.remove(&old.primary_key());
            }
            None => panic!(
                "tried to delete {} that does not exist:\n  delta.old: {:?}",
                old.kind(),
                old,
            ),
        },
    };
}
