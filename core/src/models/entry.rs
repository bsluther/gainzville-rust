use chrono::{DateTime, Utc};
use fractional_index::FractionalIndex;
use uuid::Uuid;

use crate::{
    delta::Delta,
    error::{DomainError, Result, ValidationError},
};

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: Uuid,
    pub activity_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub name: Option<String>,
    pub position: Option<Position>,
    pub is_template: bool,
    pub display_as_sets: bool,
    pub is_sequence: bool,
    pub is_complete: bool,
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

#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    pub parent_id: Uuid,
    pub frac_index: FractionalIndex,
}

impl Position {
    pub fn parse(parent_id: Option<Uuid>, frac_index: Option<String>) -> Result<Option<Position>> {
        let frac_index = frac_index
            .map(|s| {
                FractionalIndex::from_string(&s).expect("fractional index should be valid")
            });
        Self::from_parts(parent_id, frac_index)
    }

    /// Validate the pairing invariant on already-parsed parts. Used by
    /// callers that decoded `frac_index` upstream (e.g. via `gv_sql`'s
    /// `FractionalIndexColumn`).
    pub fn from_parts(
        parent_id: Option<Uuid>,
        frac_index: Option<FractionalIndex>,
    ) -> Result<Option<Position>> {
        match (parent_id, frac_index) {
            (Some(parent_id), Some(frac_index)) => Ok(Some(Position {
                parent_id,
                frac_index,
            })),
            (None, None) => Ok(None),
            _ => Err(DomainError::Consistency(
                "parent_id and frac_index must both be defined or both be null".to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Temporal {
    None,
    Start {
        start: DateTime<Utc>,
    },
    End {
        end: DateTime<Utc>,
    },
    Duration {
        duration: u32,
    },
    StartAndEnd {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    StartAndDuration {
        start: DateTime<Utc>,
        duration_ms: u32,
    },
    DurationAndEnd {
        duration_ms: u32,
        end: DateTime<Utc>,
    },
}
// TODO: should enforce that start <= end.
impl Temporal {
    pub fn parse(
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
        duration_ms: Option<u32>,
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

    /// Returns the definite start, if present. Use `infer_start` to get the inferred start based
    /// on duration and end times.
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

    /// Returns the definite end, if present. Use `infer_end` to get the inferred end based on
    /// start and duration.
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

    /// Returns the definite duration, if present. Use `infer_duration` to get the inferred duration
    /// based on start and end times.
    pub fn duration(&self) -> Option<u32> {
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

    /// Returns the inferred start time, computing it from end and duration if not explicitly set.
    pub fn infer_start(&self) -> Option<DateTime<Utc>> {
        match self {
            Temporal::End { end } => Some(*end), // Instanstaneous, end is start.
            Temporal::Start { start }
            | Temporal::StartAndEnd { start, .. }
            | Temporal::StartAndDuration { start, .. } => Some(*start),
            Temporal::DurationAndEnd { duration_ms, end } => {
                Some(*end - chrono::Duration::milliseconds(*duration_ms as i64))
            }
            _ => None,
        }
    }

    /// Returns the inferred end time, computing it from start and duration if not explicitly set.
    pub fn infer_end(&self) -> Option<DateTime<Utc>> {
        match self {
            Temporal::Start { start } => Some(*start), // Instantaneous, start is end.
            Temporal::End { end }
            | Temporal::StartAndEnd { end, .. }
            | Temporal::DurationAndEnd { end, .. } => Some(*end),
            Temporal::StartAndDuration { start, duration_ms } => {
                Some(*start + chrono::Duration::milliseconds(*duration_ms as i64))
            }
            _ => None,
        }
    }

    /// Returns the inferred duration in milliseconds, computing it from start and end if not
    /// explicitly set.
    pub fn infer_duration_ms(&self) -> Option<i64> {
        match self {
            Temporal::Duration { duration } => Some(*duration as i64),
            Temporal::StartAndDuration { duration_ms, .. }
            | Temporal::DurationAndEnd { duration_ms, .. } => Some(*duration_ms as i64),
            Temporal::StartAndEnd { start, end } => Some((*end - *start).num_milliseconds()),
            _ => None,
        }
    }

    /// Returns the canonical instant used to place this entry in time for ordering and filtering.
    /// Prefers inferred start; falls back to end if no start can be derived.
    pub fn canonical_instant(&self) -> Option<DateTime<Utc>> {
        self.infer_start().or_else(|| self.end())
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

    pub fn is_complete(mut self, v: bool) -> Self {
        self.new.is_complete = v;
        self
    }

    pub fn temporal(mut self, temporal: Temporal) -> Self {
        self.new.temporal = temporal;
        self
    }

    pub fn to_delta(self) -> Delta<Entry> {
        assert_eq!(self.old.id, self.new.id, "update should not mutate id");
        Delta::Update {
            old: self.old,
            new: self.new,
        }
    }

    pub fn to_entry(self) -> Entry {
        self.new
    }
}
