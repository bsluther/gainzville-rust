//! The registry: the frozen name → schema mapping the extractor is bound to.
//! Activities and attributes come from the database (`AllActivities` /
//! `AllAttributes`); aliases come from the workspace's human-approved
//! `aliases.toml`. Resolution is exact case-insensitive on canonical names,
//! then aliases. Unknown names are errors, never silent creates —
//! new schema goes through the resolution workflow (docs/import-design.md).

use gv_core::models::{activity::Activity, attribute::Attribute};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("unknown activity '{0}' — not a registry name or alias; if it's new, propose it in the resolution workflow")]
    UnknownActivity(String),
    #[error("unknown attribute '{0}' — not a registry name or alias; if it's new, propose it in the resolution workflow")]
    UnknownAttribute(String),
    #[error("ambiguous name '{0}': multiple registry entries match case-insensitively")]
    Ambiguous(String),
}

/// Human-maintained alias tables, loaded from the workspace `aliases.toml`.
/// Keys are surface forms (matched case-insensitively), values are canonical
/// registry names. Additive-only, like configs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Aliases {
    #[serde(default)]
    pub activities: BTreeMap<String, String>,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Registry {
    pub activities: Vec<Activity>,
    pub attributes: Vec<Attribute>,
    pub aliases: Aliases,
}

impl Registry {
    pub fn new(activities: Vec<Activity>, attributes: Vec<Attribute>, aliases: Aliases) -> Self {
        Registry {
            activities,
            attributes,
            aliases,
        }
    }

    pub fn resolve_activity(&self, name: &str) -> Result<&Activity, ResolveError> {
        let canonical = resolve_alias(&self.aliases.activities, name);
        let matches: Vec<&Activity> = self
            .activities
            .iter()
            .filter(|a| a.name.to_string().eq_ignore_ascii_case(&canonical))
            .collect();
        match matches.as_slice() {
            [one] => Ok(one),
            [] => Err(ResolveError::UnknownActivity(name.to_string())),
            _ => Err(ResolveError::Ambiguous(name.to_string())),
        }
    }

    pub fn resolve_attribute(&self, name: &str) -> Result<&Attribute, ResolveError> {
        let canonical = resolve_alias(&self.aliases.attributes, name);
        let matches: Vec<&Attribute> = self
            .attributes
            .iter()
            .filter(|a| a.name.eq_ignore_ascii_case(&canonical))
            .collect();
        match matches.as_slice() {
            [one] => Ok(one),
            [] => Err(ResolveError::UnknownAttribute(name.to_string())),
            _ => Err(ResolveError::Ambiguous(name.to_string())),
        }
    }

    pub fn activity_by_id(&self, id: Uuid) -> Option<&Activity> {
        self.activities.iter().find(|a| a.id == id)
    }

    pub fn attribute_by_id(&self, id: Uuid) -> Option<&Attribute> {
        self.attributes.iter().find(|a| a.id == id)
    }
}

/// Map a surface form through the alias table (case-insensitive key match);
/// pass through unchanged when no alias applies.
fn resolve_alias(aliases: &BTreeMap<String, String>, name: &str) -> String {
    aliases
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gv_core::models::activity::ActivityName;

    fn activity(name: &str) -> Activity {
        Activity {
            id: Uuid::new_v4(),
            owner_id: Uuid::nil(),
            name: ActivityName::parse(name.to_string()).unwrap(),
            description: None,
            source_activity_id: None,
        }
    }

    #[test]
    fn resolves_exact_alias_and_unknown() {
        let mut aliases = Aliases::default();
        aliases
            .activities
            .insert("hangboard".to_string(), "Fingerboard".to_string());
        let reg = Registry::new(
            vec![activity("Fingerboard"), activity("Climbing")],
            vec![],
            aliases,
        );

        assert_eq!(reg.resolve_activity("climbing").unwrap().name.to_string(), "Climbing");
        assert_eq!(
            reg.resolve_activity("Hangboard").unwrap().name.to_string(),
            "Fingerboard"
        );
        assert!(matches!(
            reg.resolve_activity("Rowing"),
            Err(ResolveError::UnknownActivity(_))
        ));
    }
}
