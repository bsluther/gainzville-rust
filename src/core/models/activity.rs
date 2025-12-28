use crate::core::error::{Result, ValidationError};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Activity {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub source_activity_id: Option<Uuid>,
    pub name: ActivityName,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActivityName(String);
impl ActivityName {
    pub fn parse(str: String) -> Result<ActivityName> {
        let trimmed = str.trim();
        match str.len() {
            0 | 50.. => Err(ValidationError::InvalidActivityName(trimmed.to_string()).into()),
            _ => Ok(ActivityName(trimmed.to_string())),
        }
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
}

#[derive(Debug)]
pub struct ActivityUpdater {
    old: Activity,
    new: Activity,
}

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
