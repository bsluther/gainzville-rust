use uuid::Uuid;

use crate::{
    SYSTEM_ACTOR_ID,
    models::attribute::{Attribute, MassConfig, MassUnit, NumericConfig, SelectConfig},
};

pub struct StandardLibrary {}

impl StandardLibrary {
    pub fn create_attributes() -> Vec<Attribute> {
        let reps = Attribute {
            id: Uuid::new_v4(),
            owner_id: SYSTEM_ACTOR_ID,
            name: "Reps".to_string(),
            config: NumericConfig {
                min: Some(0.),
                max: Some(0.),
                integer: true,
                default: Some(0.),
            }
            .into(),
        };

        let load = Attribute {
            id: Uuid::new_v4(),
            owner_id: SYSTEM_ACTOR_ID,
            name: "Load".to_string(),
            config: MassConfig {
                default_units: vec![MassUnit::Pound],
            }
            .into(),
        };

        let outcome = Attribute {
            id: Uuid::new_v4(),
            owner_id: SYSTEM_ACTOR_ID,
            name: "Outcome".to_string(),
            config: SelectConfig {
                options: vec![
                    "Redpoint".to_string(),
                    "Flash".to_string(),
                    "Onsight".to_string(),
                    "Attempt".to_string(),
                ],
                default: None,
                ordered: false,
            }
            .into(),
        };

        // This is a good example where you want something like an equivalence, so you can use 10-
        // but map that to 10a or 10b if someone else uses that, or map to French grades.
        let yds_grade = Attribute {
            id: Uuid::new_v4(),
            owner_id: SYSTEM_ACTOR_ID,
            name: "YDS Grade".to_string(),
            config: SelectConfig {
                options: vec![
                    "5.8".to_string(),
                    "5.9".to_string(),
                    "10-".to_string(),
                    "10".to_string(),
                    "10+".to_string(),
                    "11-".to_string(),
                    "11-".to_string(),
                    "11".to_string(),
                    "11+".to_string(),
                    "12-".to_string(),
                    "12".to_string(),
                    "12+".to_string(),
                    "13-".to_string(),
                    "13".to_string(),
                    "13+".to_string(),
                ],
                default: None,
                ordered: true,
            }
            .into(),
        };

        vec![reps, load, outcome, yds_grade]
    }
}
