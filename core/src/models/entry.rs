use chrono::{DateTime, Utc};
use fractional_index::FractionalIndex;
use rand::Rng;
use sqlx::prelude::FromRow;
use uuid::Uuid;

use crate::{
    delta::Delta,
    error::{DomainError, Result, ValidationError},
};

#[derive(Debug, Clone, FromRow)]
pub struct Entry {
    pub id: Uuid,
    pub activity_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub position: Option<Position>,
    pub is_template: bool,
    pub display_as_sets: bool,
    pub is_sequence: bool,
    pub temporal: Temporal,
}

impl Entry {
    pub fn parent_id(&self) -> Option<Uuid> {
        self.position.as_ref().map(|p| p.parent_id)
    }

    pub fn frac_index(&self) -> Option<&FractionalIndex> {
        self.position.as_ref().map(|p| &p.frac_index)
    }

    pub fn update(&self) -> EntryUpdater {
        EntryUpdater {
            old: self.clone(),
            new: self.clone(),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct EntryRow {
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
}

impl EntryRow {
    /// Convert this EntryRow to an Entry, may fail if constraints are violated.
    // Implement TryFrom instead?
    pub fn to_entry(self) -> Result<Entry> {
        let duration_ms: Option<u64> =
            self.duration_ms
                .map(|d| d.try_into())
                .transpose()
                .map_err(|_| {
                    ValidationError::Other(
                        "duration must be positive, failed to cast i64 to u64"
                            .to_string()
                            .into(),
                    )
                })?;
        Ok(Entry {
            id: self.id,
            activity_id: self.activity_id,
            owner_id: self.owner_id,
            position: Position::parse_optional(self.parent_id, self.frac_index)?,
            is_template: self.is_template,
            is_sequence: self.is_sequence,
            display_as_sets: self.display_as_sets,
            temporal: Temporal::parse(self.start_time, self.end_time, duration_ms)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Position {
    pub parent_id: Uuid,
    pub frac_index: FractionalIndex,
}

impl Position {
    pub fn parse_optional(
        parent_id: Option<Uuid>,
        frac_index: Option<String>,
    ) -> Result<Option<Position>> {
        if (parent_id.is_none() && frac_index.is_some())
            || (parent_id.is_some() && frac_index.is_none())
        {
            return Err(DomainError::Consistency(
                "parent_id and frac_index must both be defined or both be null".to_string(),
            ));
        }

        let position = match (parent_id, frac_index) {
            (Some(parent_id), Some(frac_index)) => Some(Position {
                parent_id,
                frac_index: FractionalIndex::from_string(&frac_index)
                    .expect("fractonal index should be valid"),
            }),
            (None, None) => None,
            _ => unreachable!(
                "parent_id and frac_index must both be defined or both be null, already checked above"
            ),
        };
        Ok(position)
    }
}

#[derive(Debug, Clone)]
pub enum Temporal {
    None,
    Start {
        start: DateTime<Utc>,
    },
    End {
        end: DateTime<Utc>,
    },
    Duration {
        duration: u64,
    },
    StartAndEnd {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    StartAndDuration {
        start: DateTime<Utc>,
        duration_ms: u64,
    },
    DurationAndEnd {
        duration_ms: u64,
        end: DateTime<Utc>,
    },
}

impl Temporal {
    pub fn parse(
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
        duration_ms: Option<u64>,
    ) -> Result<Temporal> {
        match (start, end, duration_ms) {
            (None, None, None) => Ok(Temporal::None),
            (Some(start), None, None) => Ok(Temporal::Start { start }),
            (None, Some(end), None) => Ok(Temporal::End { end }),
            (None, None, Some(duration)) => Ok(Temporal::Duration { duration }),
            (Some(start), Some(end), None) => Ok(Temporal::StartAndEnd { start, end }),
            (Some(start), None, Some(duration_ms)) => {
                Ok(Temporal::StartAndDuration { start, duration_ms })
            }
            (None, Some(end), Some(duration_ms)) => {
                Ok(Temporal::DurationAndEnd { duration_ms, end })
            }

            _ => Err(
                ValidationError::Other("invalid combination of temporal values".to_string()).into(),
            ),
        }
    }

    pub fn start(&self) -> Option<DateTime<Utc>> {
        match self {
            Temporal::None
            | Temporal::End { end: _ }
            | Temporal::Duration { duration: _ }
            | Temporal::DurationAndEnd {
                duration_ms: _,
                end: _,
            } => None,
            Temporal::Start { start }
            | Temporal::StartAndEnd { start, end: _ }
            | Temporal::StartAndDuration {
                start,
                duration_ms: _,
            } => Some(*start),
        }
    }

    pub fn end(&self) -> Option<DateTime<Utc>> {
        match self {
            Temporal::None
            | Temporal::Start { start: _ }
            | Temporal::Duration { duration: _ }
            | Temporal::StartAndDuration {
                start: _,
                duration_ms: _,
            } => None,
            Temporal::End { end }
            | Temporal::StartAndEnd { start: _, end }
            | Temporal::DurationAndEnd {
                duration_ms: _,
                end,
            } => Some(*end),
        }
    }

    pub fn duration(&self) -> Option<u64> {
        match self {
            Temporal::None
            | Temporal::Start { start: _ }
            | Temporal::End { end: _ }
            | Temporal::StartAndEnd { start: _, end: _ } => None,
            Temporal::Duration { duration } => Some(*duration),
            Temporal::StartAndDuration {
                start: _,
                duration_ms,
            } => Some(*duration_ms),
            Temporal::DurationAndEnd {
                duration_ms,
                end: _,
            } => Some(*duration_ms),
        }
    }
}

#[derive(Debug)]
pub struct EntryUpdater {
    old: Entry,
    new: Entry,
}

impl EntryUpdater {
    pub fn position(mut self, position: Option<Position>) -> Self {
        self.new.position = position;
        self
    }

    pub fn activity_id(mut self, activity_id: Option<Uuid>) -> Self {
        self.new.activity_id = activity_id;
        self
    }

    pub fn display_as_sets(mut self, display_as_sets: bool) -> Self {
        self.new.display_as_sets = display_as_sets;
        self
    }

    pub fn is_sequence(mut self, is_sequence: bool) -> Self {
        self.new.is_sequence = is_sequence;
        self
    }

    pub fn temporal(mut self, temporal: Temporal) -> Self {
        self.new.temporal = temporal;
        self
    }

    pub fn to_delta(self) -> Delta<Entry> {
        assert_eq!(self.old.id, self.new.id, "update should not mutate id");
        // YOU ARE HERE
        // convert updater to a delta to use in action
        Delta::Update {
            id: self.old.id,
            old: self.old,
            new: self.new,
        }
    }

    pub fn to_entry(self) -> Entry {
        self.new
    }
}
