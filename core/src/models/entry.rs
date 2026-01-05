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
}
