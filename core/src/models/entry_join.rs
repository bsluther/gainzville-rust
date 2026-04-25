use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::prelude::FromRow;
use uuid::Uuid;

use super::activity::{Activity, ActivityName};
use super::entry::{Entry, Position, Temporal};
use crate::error::Result;
use crate::models::attribute_pair::AttributePair;

/// Domain model representing an Entry with all its joined relations.
/// Currently includes the optional Activity and Attribute-Value pairs.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryJoin {
    pub entry: Entry,
    pub activity: Option<Activity>,
    attributes: HashMap<Uuid, AttributePair>,
}

impl EntryJoin {
    pub fn is_sequence(&self) -> bool {
        self.entry.is_sequence
    }

    pub fn display_name(&self) -> String {
        self.activity
            .as_ref()
            .map_or("Unnamed".to_string(), |a| a.name.to_string())
    }

    pub fn attribute(&self, attr_id: Uuid) -> Option<&AttributePair> {
        self.attributes.get(&attr_id)
    }

    pub fn attributes(&self) -> impl Iterator<Item = &AttributePair> {
        self.attributes.values()
    }

    pub fn from_row(row: EntryJoinRow, attributes: HashMap<Uuid, AttributePair>) -> Result<Self> {
        let duration_ms: Option<u32> =
            row.duration_ms
                .map(|d| d.try_into())
                .transpose()
                .map_err(|_| {
                    crate::error::ValidationError::Other(
                        "duration must fit in a u32".to_string().into(),
                    )
                })?;

        let entry = Entry {
            id: row.id,
            activity_id: row.activity_id,
            owner_id: row.owner_id,
            name: row.name,
            position: Position::parse(row.parent_id, row.frac_index)?,
            is_template: row.is_template,
            is_sequence: row.is_sequence,
            is_complete: row.is_complete,
            display_as_sets: row.display_as_sets,
            temporal: Temporal::parse(row.start_time, row.end_time, duration_ms)?,
        };

        let activity = match row.act_id {
            Some(id) => Some(Activity {
                id,
                owner_id: row
                    .act_owner_id
                    .expect("act_owner_id should be present when act_id is"),
                source_activity_id: row.act_source_activity_id,
                name: ActivityName::parse(
                    row.act_name
                        .expect("act_name should be present when act_id is"),
                )?,
                description: row.act_description,
            }),
            None => None,
        };

        Ok(EntryJoin {
            entry,
            activity,
            attributes,
        })
    }
}

/// Flat row struct for decoding a LEFT JOIN between entries and activities.
/// All activity columns are optional since the join may not match.
#[derive(Debug, Clone, FromRow)]
pub struct EntryJoinRow {
    // Entry columns
    pub id: Uuid,
    pub activity_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub name: Option<String>,
    pub parent_id: Option<Uuid>,
    pub frac_index: Option<String>,
    pub is_template: bool,
    pub display_as_sets: bool,
    pub is_sequence: bool,
    pub is_complete: bool,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,

    // Activity columns (all optional for LEFT JOIN)
    #[sqlx(rename = "act_id")]
    pub act_id: Option<Uuid>,
    #[sqlx(rename = "act_owner_id")]
    pub act_owner_id: Option<Uuid>,
    #[sqlx(rename = "act_source_activity_id")]
    pub act_source_activity_id: Option<Uuid>,
    #[sqlx(rename = "act_name")]
    pub act_name: Option<String>,
    #[sqlx(rename = "act_description")]
    pub act_description: Option<String>,
}
