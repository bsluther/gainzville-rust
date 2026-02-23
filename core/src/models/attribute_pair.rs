use uuid::Uuid;

use crate::{
    error::DomainError,
    models::attribute::{
        Attribute, AttributeConfig, MassConfig, MassValue, NumericConfig, NumericValue,
        SelectConfig, SelectValue, Value,
    },
};

pub enum AttributePair {
    Numeric(NumericAttributePair),
    Select(SelectAttributePair),
    Mass(MassAttributePair),
}

impl TryFrom<(Attribute, Value)> for AttributePair {
    type Error = DomainError;
    fn try_from((attr, val): (Attribute, Value)) -> Result<Self, Self::Error> {
        match (attr.config, val.plan, val.actual) {
            (AttributeConfig::Numeric(cfg), plan, actual) => {
                let plan = plan.map(|v| v.expect_numeric()).transpose()?;
                let actual = actual.map(|v| v.expect_numeric()).transpose()?;
                Ok(AttributePair::Numeric(NumericAttributePair {
                    attr_id: attr.id,
                    entry_id: val.entry_id,
                    owner_id: attr.owner_id,
                    name: attr.name,
                    config: cfg,
                    index_float: val.index_float,
                    plan,
                    actual,
                }))
            }
            (AttributeConfig::Select(cfg), plan, actual) => {
                let plan = plan.map(|v| v.expect_select()).transpose()?;
                let actual = actual.map(|v| v.expect_select()).transpose()?;
                Ok(AttributePair::Select(SelectAttributePair {
                    attr_id: attr.id,
                    entry_id: val.entry_id,
                    owner_id: attr.owner_id,
                    name: attr.name,
                    config: cfg,
                    index_string: val.index_string,
                    plan,
                    actual,
                }))
            }
            (AttributeConfig::Mass(cfg), plan, actual) => {
                let plan = plan.map(|v| v.expect_mass()).transpose()?;
                let actual = actual.map(|v| v.expect_mass()).transpose()?;
                Ok(AttributePair::Mass(MassAttributePair {
                    attr_id: attr.id,
                    entry_id: val.entry_id,
                    owner_id: attr.owner_id,
                    name: attr.name,
                    config: cfg,
                    index_float: val.index_float,
                    plan,
                    actual,
                }))
            }
        }
    }
}

pub struct NumericAttributePair {
    pub attr_id: Uuid,
    pub entry_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub config: NumericConfig,
    pub index_float: Option<f64>,
    pub plan: Option<NumericValue>,
    pub actual: Option<NumericValue>,
}

pub struct SelectAttributePair {
    pub attr_id: Uuid,
    pub entry_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub config: SelectConfig,
    pub index_string: Option<String>,
    pub plan: Option<SelectValue>,
    pub actual: Option<SelectValue>,
}

pub struct MassAttributePair {
    pub attr_id: Uuid,
    pub entry_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub config: MassConfig,
    pub index_float: Option<f64>,
    pub plan: Option<MassValue>,
    pub actual: Option<MassValue>,
}
