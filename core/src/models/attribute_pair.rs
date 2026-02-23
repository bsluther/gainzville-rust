use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    error::{DomainError, Result},
    models::attribute::{
        Attribute, AttributeConfig, AttributeRow, MassConfig, MassValue, NumericConfig,
        NumericValue, SelectConfig, SelectValue, Value, ValueRow,
    },
};

#[derive(Debug, Clone)]
pub enum AttributePair {
    Numeric(NumericAttributePair),
    Select(SelectAttributePair),
    Mass(MassAttributePair),
}

impl AttributePair {
    pub fn attr_id(&self) -> Uuid {
        match self {
            AttributePair::Numeric(p) => p.attr_id,
            AttributePair::Select(p) => p.attr_id,
            AttributePair::Mass(p) => p.attr_id,
        }
    }
}

impl TryFrom<(Attribute, Value)> for AttributePair {
    type Error = DomainError;
    fn try_from((attr, val): (Attribute, Value)) -> std::result::Result<Self, Self::Error> {
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

/// Flat row struct for decoding a JOIN between attributes and attribute_values.
#[derive(Debug, Clone, FromRow)]
pub struct AttributePairRow {
    // Attribute columns
    #[sqlx(rename = "attr_id")]
    pub attr_id: Uuid,
    #[sqlx(rename = "attr_owner_id")]
    pub attr_owner_id: Uuid,
    #[sqlx(rename = "attr_name")]
    pub attr_name: String,
    #[sqlx(rename = "attr_data_type")]
    pub attr_data_type: String,
    #[sqlx(rename = "attr_config")]
    pub attr_config: String,

    // Value columns
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
    pub plan: Option<String>,
    pub actual: Option<String>,
    pub index_float: Option<f64>,
    pub index_string: Option<String>,
}

impl AttributePairRow {
    pub fn to_attribute_pair(self) -> Result<AttributePair> {
        let attr = AttributeRow {
            id: self.attr_id,
            owner_id: self.attr_owner_id,
            name: self.attr_name,
            data_type: self.attr_data_type,
            config: self.attr_config,
        }
        .to_attribute()?;

        let val = ValueRow {
            entry_id: self.entry_id,
            attribute_id: self.attribute_id,
            plan: self.plan,
            actual: self.actual,
            index_float: self.index_float,
            index_string: self.index_string,
        }
        .to_value()?;

        AttributePair::try_from((attr, val))
    }
}
