use fractional_index::FractionalIndex;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: Uuid,
    pub activity_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub frac_index: Option<FractionalIndex>,
    pub is_template: bool,
    pub display_as_sets: bool,
    pub is_sequence: bool,
}

impl Entry {
    pub fn update(&self) -> EntryUpdater {
        EntryUpdater {
            old: self.clone(),
            new: self.clone(),
        }
    }
}

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
    /// Atomically update both parent_id and frac_index (position in tree)
    pub fn position(
        mut self,
        parent_id: Option<Uuid>,
        frac_index: Option<FractionalIndex>,
    ) -> Self {
        self.new.parent_id = parent_id;
        self.new.frac_index = frac_index;
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
