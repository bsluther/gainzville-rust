use chrono::{DateTime, Utc};
use fractional_index::FractionalIndex;
use uuid::Uuid;

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct Position {
    pub parent_id: Uuid,
    pub frac_index: FractionalIndex,
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
}
