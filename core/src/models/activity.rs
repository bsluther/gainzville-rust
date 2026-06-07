use crate::{
    actions::CreateActivity,
    error::{Result, ValidationError},
    models::entry::{Entry, Temporal},
};
use uuid::Uuid;

// TODO: Activities can't currently be sequences! Need to add a field to the activity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Activity {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub source_activity_id: Option<Uuid>,
    pub name: ActivityName,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActivityName(String);
impl ActivityName {
    /// Maximum length, in characters, of a (trimmed) activity name.
    pub const MAX_LEN: usize = 50;

    pub fn parse(str: String) -> Result<ActivityName> {
        let trimmed = str.trim();
        let len = trimmed.chars().count();
        if len == 0 || len > Self::MAX_LEN {
            return Err(ValidationError::InvalidActivityName(trimmed.to_string()).into());
        }
        Ok(ActivityName(trimmed.to_string()))
    }

    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl Activity {
    pub fn update(&self) -> ActivityUpdater {
        ActivityUpdater {
            old: self.clone(),
            new: self.clone(),
        }
    }

    /// Build the `CreateActivity` action, using `template_id` as the id of the
    /// template-root entry. The caller mints the id (via `Io::uuid` in
    /// production, the rng in generation) so this stays a pure conversion.
    pub fn into_create_activity(&self, template_id: Uuid) -> CreateActivity {
        CreateActivity {
            actor_id: self.owner_id,
            activity: self.clone(),
            template: vec![Entry {
                id: template_id,
                owner_id: self.owner_id,
                activity_id: Some(self.id),
                name: None,
                position: None,
                is_template: true,
                display_as_sets: false,
                is_sequence: true,
                is_complete: false,
                temporal: Temporal::None,
            }],
        }
    }
}

#[derive(Debug)]
#[allow(unused)]
pub struct ActivityUpdater {
    old: Activity,
    new: Activity,
}

#[allow(unused)]
impl ActivityUpdater {
    fn source_activity_id(mut self, source_activity_id: Option<Uuid>) -> Self {
        self.new.source_activity_id = source_activity_id;
        self
    }

    fn name(mut self, name: ActivityName) -> Self {
        self.new.name = name;
        self
    }

    fn description(mut self, description: Option<String>) -> Self {
        self.new.description = description;
        self
    }
}
