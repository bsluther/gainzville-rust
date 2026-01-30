use chrono::{DateTime, Utc};
use sqlx::prelude::FromRow;
use uuid::Uuid;

use super::activity::{Activity, ActivityName};
use super::entry::{Entry, Position, Temporal};
use crate::error::Result;

/// Domain model representing an Entry with all its joined relations.
/// Currently includes the optional Activity; will expand to include
/// attributes, categories, etc.
#[derive(Debug, Clone)]
pub struct EntryView {
    pub entry: Entry,
    pub activity: Option<Activity>,
}

impl EntryView {
    pub fn is_sequence(&self) -> bool {
        self.entry.is_sequence
    }

    pub fn display_name(&self) -> String {
        self.activity
            .as_ref()
            .map_or("Unnamed".to_string(), |a| a.name.to_string())
    }
}

/// Flat row struct for decoding a LEFT JOIN between entries and activities.
/// All activity columns are optional since the join may not match.
#[derive(Debug, Clone, FromRow)]
pub struct EntryViewRow {
    // Entry columns
    pub id: Uuid,
    pub activity_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub frac_index: Option<String>,
    pub is_template: bool,
    pub display_as_sets: bool,
    pub is_sequence: bool,
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

impl EntryViewRow {
    /// Convert this flat row into the nested EntryView domain model.
    pub fn to_entry_view(self) -> Result<EntryView> {
        let duration_ms: Option<u32> =
            self.duration_ms
                .map(|d| d.try_into())
                .transpose()
                .map_err(|_| {
                    crate::error::ValidationError::Other(
                        "duration must fit in a u32".to_string().into(),
                    )
                })?;

        let entry = Entry {
            id: self.id,
            activity_id: self.activity_id,
            owner_id: self.owner_id,
            position: Position::parse(self.parent_id, self.frac_index)?,
            is_template: self.is_template,
            is_sequence: self.is_sequence,
            display_as_sets: self.display_as_sets,
            temporal: Temporal::parse(self.start_time, self.end_time, duration_ms)?,
        };

        let activity = match self.act_id {
            Some(id) => Some(Activity {
                id,
                owner_id: self
                    .act_owner_id
                    .expect("act_owner_id should be present when act_id is"),
                source_activity_id: self.act_source_activity_id,
                name: ActivityName::parse(
                    self.act_name
                        .expect("act_name should be present when act_id is"),
                )?,
                description: self.act_description,
            }),
            None => None,
        };

        Ok(EntryView { entry, activity })
    }
}
