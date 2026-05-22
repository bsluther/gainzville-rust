//! `*Row` types: flat shapes that mirror DB tables, with every leaf
//! wrapped in a `*Column` so the sqlx encoding rule lives in one place.
//!
//! Each Row supports bidirectional conversion to/from its core domain
//! type. Flat models use `From` impls (infallible); nested models with
//! JSON-encoded fields or pairing invariants use methods returning
//! `Result`, matching the existing core style.
//!
//! Read-only join shapes (`EntryJoinRow`, `AttributePairRow`) only
//! convert in the `Row → core` direction since they don't correspond to
//! single tables.

use sqlx::FromRow;

use gv_core::{
    error::{DomainError, Result},
    models::{
        activity::Activity,
        attribute::{Attribute, AttributeConfig, AttributeValue, Value},
        attribute_pair::AttributePair,
        entry::{Entry, Position, Temporal},
        entry_join::EntryJoin,
        user::User,
    },
};

use crate::columns::{
    ActivityNameColumn, DateTimeColumn, EmailColumn, FractionalIndexColumn, UsernameColumn,
    UuidColumn,
};

// --- User ---

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct UserRow {
    pub actor_id: UuidColumn,
    pub username: UsernameColumn,
    pub email: EmailColumn,
}

impl From<User> for UserRow {
    fn from(user: User) -> Self {
        UserRow {
            actor_id: UuidColumn(user.actor_id),
            username: UsernameColumn(user.username),
            email: EmailColumn(user.email),
        }
    }
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        User {
            actor_id: row.actor_id.0,
            username: row.username.0,
            email: row.email.0,
        }
    }
}

// --- Activity ---

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct ActivityRow {
    pub id: UuidColumn,
    pub owner_id: UuidColumn,
    pub source_activity_id: Option<UuidColumn>,
    pub name: ActivityNameColumn,
    pub description: Option<String>,
}

impl From<Activity> for ActivityRow {
    fn from(a: Activity) -> Self {
        ActivityRow {
            id: UuidColumn(a.id),
            owner_id: UuidColumn(a.owner_id),
            source_activity_id: a.source_activity_id.map(UuidColumn),
            name: ActivityNameColumn(a.name),
            description: a.description,
        }
    }
}

impl From<ActivityRow> for Activity {
    fn from(row: ActivityRow) -> Self {
        Activity {
            id: row.id.0,
            owner_id: row.owner_id.0,
            source_activity_id: row.source_activity_id.map(|c| c.0),
            name: row.name.0,
            description: row.description,
        }
    }
}

// --- Entry ---

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct EntryRow {
    pub id: UuidColumn,
    pub activity_id: Option<UuidColumn>,
    pub owner_id: UuidColumn,
    pub name: Option<String>,
    pub parent_id: Option<UuidColumn>,
    pub frac_index: Option<FractionalIndexColumn>,
    pub is_template: bool,
    pub display_as_sets: bool,
    pub is_sequence: bool,
    pub is_complete: bool,
    pub start_time: Option<DateTimeColumn>,
    pub end_time: Option<DateTimeColumn>,
    pub duration_ms: Option<i64>,
}

impl EntryRow {
    pub fn from_entry(entry: &Entry) -> Self {
        let (parent_id, frac_index) = match entry.position.as_ref() {
            Some(p) => (Some(UuidColumn(p.parent_id)), Some(FractionalIndexColumn(p.frac_index.clone()))),
            None => (None, None),
        };
        EntryRow {
            id: UuidColumn(entry.id),
            activity_id: entry.activity_id.map(UuidColumn),
            owner_id: UuidColumn(entry.owner_id),
            name: entry.name.clone(),
            parent_id,
            frac_index,
            is_template: entry.is_template,
            display_as_sets: entry.display_as_sets,
            is_sequence: entry.is_sequence,
            is_complete: entry.is_complete,
            start_time: entry.temporal.start().map(DateTimeColumn),
            end_time: entry.temporal.end().map(DateTimeColumn),
            duration_ms: entry.temporal.duration().map(|d| d as i64),
        }
    }

    pub fn to_entry(self) -> Result<Entry> {
        let duration_ms: Option<u32> = self
            .duration_ms
            .map(|d| d.try_into())
            .transpose()
            .map_err(|_| {
                DomainError::Validation(gv_core::error::ValidationError::Other(
                    "duration must fit in a u32".to_string().into(),
                ))
            })?;
        let position = Position::from_parts(
            self.parent_id.map(|c| c.0),
            self.frac_index.map(|c| c.0),
        )?;
        let temporal = Temporal::parse(
            self.start_time.map(|c| c.0),
            self.end_time.map(|c| c.0),
            duration_ms,
        )?;
        Ok(Entry {
            id: self.id.0,
            activity_id: self.activity_id.map(|c| c.0),
            owner_id: self.owner_id.0,
            name: self.name,
            position,
            is_template: self.is_template,
            is_sequence: self.is_sequence,
            is_complete: self.is_complete,
            display_as_sets: self.display_as_sets,
            temporal,
        })
    }
}

// --- Attribute ---

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct AttributeRow {
    pub id: UuidColumn,
    pub owner_id: UuidColumn,
    pub name: String,
    pub data_type: String,
    pub config: String, // JSON as TEXT
}

impl AttributeRow {
    pub fn from_attribute(attr: &Attribute) -> Result<Self> {
        Ok(AttributeRow {
            id: UuidColumn(attr.id),
            owner_id: UuidColumn(attr.owner_id),
            name: attr.name.clone(),
            data_type: attr.config.data_type().to_string(),
            config: serde_json::to_string(&attr.config)
                .map_err(|e| DomainError::Other(e.to_string()))?,
        })
    }

    pub fn to_attribute(self) -> Result<Attribute> {
        let config: AttributeConfig =
            serde_json::from_str(&self.config).map_err(|e| DomainError::Other(e.to_string()))?;
        Ok(Attribute {
            id: self.id.0,
            owner_id: self.owner_id.0,
            name: self.name,
            config,
        })
    }
}

// --- Value ---

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct ValueRow {
    pub entry_id: UuidColumn,
    pub attribute_id: UuidColumn,
    pub plan: Option<String>,   // JSON as TEXT
    pub actual: Option<String>, // JSON as TEXT
    pub index_float: Option<f64>,
    pub index_string: Option<String>,
}

impl ValueRow {
    pub fn from_value(value: &Value) -> Result<Self> {
        let plan = value
            .plan
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| DomainError::Other(e.to_string()))?;
        let actual = value
            .actual
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| DomainError::Other(e.to_string()))?;
        Ok(ValueRow {
            entry_id: UuidColumn(value.entry_id),
            attribute_id: UuidColumn(value.attribute_id),
            plan,
            actual,
            index_float: value.index_float,
            index_string: value.index_string.clone(),
        })
    }

    pub fn to_value(self) -> Result<Value> {
        let plan: Option<AttributeValue> = self
            .plan
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|e| DomainError::Other(e.to_string()))?;
        let actual: Option<AttributeValue> = self
            .actual
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|e| DomainError::Other(e.to_string()))?;
        Ok(Value {
            entry_id: self.entry_id.0,
            attribute_id: self.attribute_id.0,
            index_float: self.index_float,
            index_string: self.index_string,
            plan,
            actual,
        })
    }
}

// --- AttributePair (read-only join: attributes JOIN values) ---

#[derive(Debug, Clone, FromRow)]
pub struct AttributePairRow {
    #[sqlx(rename = "attr_id")]
    pub attr_id: UuidColumn,
    #[sqlx(rename = "attr_owner_id")]
    pub attr_owner_id: UuidColumn,
    #[sqlx(rename = "attr_name")]
    pub attr_name: String,
    #[sqlx(rename = "attr_data_type")]
    pub attr_data_type: String,
    #[sqlx(rename = "attr_config")]
    pub attr_config: String,

    pub entry_id: UuidColumn,
    pub attribute_id: UuidColumn,
    pub plan: Option<String>,
    pub actual: Option<String>,
    pub index_float: Option<f64>,
    pub index_string: Option<String>,
}

impl AttributePairRow {
    pub fn to_attribute_pair(self) -> Result<AttributePair> {
        let attr = AttributeRow {
            id: self.attr_id,
            owner_id: self.attr_owner_id,
            name: self.attr_name,
            data_type: self.attr_data_type,
            config: self.attr_config,
        }
        .to_attribute()?;
        let val = ValueRow {
            entry_id: self.entry_id,
            attribute_id: self.attribute_id,
            plan: self.plan,
            actual: self.actual,
            index_float: self.index_float,
            index_string: self.index_string,
        }
        .to_value()?;
        AttributePair::try_from((attr, val))
    }
}

// --- EntryJoin (read-only join: entries LEFT JOIN activities) ---

#[derive(Debug, Clone, FromRow)]
pub struct EntryJoinRow {
    // Entry columns
    pub id: UuidColumn,
    pub activity_id: Option<UuidColumn>,
    pub owner_id: UuidColumn,
    pub name: Option<String>,
    pub parent_id: Option<UuidColumn>,
    pub frac_index: Option<FractionalIndexColumn>,
    pub is_template: bool,
    pub display_as_sets: bool,
    pub is_sequence: bool,
    pub is_complete: bool,
    pub start_time: Option<DateTimeColumn>,
    pub end_time: Option<DateTimeColumn>,
    pub duration_ms: Option<i64>,

    // Activity columns (all optional — LEFT JOIN may not match)
    #[sqlx(rename = "act_id")]
    pub act_id: Option<UuidColumn>,
    #[sqlx(rename = "act_owner_id")]
    pub act_owner_id: Option<UuidColumn>,
    #[sqlx(rename = "act_source_activity_id")]
    pub act_source_activity_id: Option<UuidColumn>,
    #[sqlx(rename = "act_name")]
    pub act_name: Option<ActivityNameColumn>,
    #[sqlx(rename = "act_description")]
    pub act_description: Option<String>,
}

impl EntryJoinRow {
    /// Combine this row with the entry's joined attribute pairs into a
    /// full `EntryJoin`. Caller is responsible for fetching the
    /// `attributes` Vec separately (typically via `AttributePairRow`).
    pub fn into_entry_join(self, attributes: Vec<AttributePair>) -> Result<EntryJoin> {
        let activity = match self.act_id {
            Some(act_id) => Some(Activity {
                id: act_id.0,
                owner_id: self
                    .act_owner_id
                    .expect("act_owner_id present when act_id is")
                    .0,
                source_activity_id: self.act_source_activity_id.map(|c| c.0),
                name: self
                    .act_name
                    .expect("act_name present when act_id is")
                    .0,
                description: self.act_description,
            }),
            None => None,
        };

        let entry = EntryRow {
            id: self.id,
            activity_id: self.activity_id,
            owner_id: self.owner_id,
            name: self.name,
            parent_id: self.parent_id,
            frac_index: self.frac_index,
            is_template: self.is_template,
            display_as_sets: self.display_as_sets,
            is_sequence: self.is_sequence,
            is_complete: self.is_complete,
            start_time: self.start_time,
            end_time: self.end_time,
            duration_ms: self.duration_ms,
        }
        .to_entry()?;

        Ok(EntryJoin::new(entry, activity, attributes))
    }
}