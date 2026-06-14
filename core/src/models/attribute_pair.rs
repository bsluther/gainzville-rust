use uuid::Uuid;

use crate::{
    error::DomainError,
    models::attribute::{
        Attribute, AttributeConfig, LengthConfig, LengthUnit, LengthValue, MassConfig, MassUnit,
        MassValue, NumericConfig, NumericValue, SelectConfig, SelectValue, TextConfig, Value,
    },
};

#[derive(Debug, Clone, PartialEq)]
pub enum AttributePair {
    Numeric(NumericAttributePair),
    Select(SelectAttributePair),
    Mass(MassAttributePair),
    Length(LengthAttributePair),
    Text(TextAttributePair),
}

impl AttributePair {
    pub fn attr_id(&self) -> Uuid {
        match self {
            AttributePair::Numeric(p) => p.attr_id,
            AttributePair::Select(p) => p.attr_id,
            AttributePair::Mass(p) => p.attr_id,
            AttributePair::Length(p) => p.attr_id,
            AttributePair::Text(p) => p.attr_id,
        }
    }

    pub fn name(&self) -> String {
        match self {
            AttributePair::Numeric(p) => p.name.clone(),
            AttributePair::Select(p) => p.name.clone(),
            AttributePair::Mass(p) => p.name.clone(),
            AttributePair::Length(p) => p.name.clone(),
            AttributePair::Text(p) => p.name.clone(),
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
            (AttributeConfig::Length(cfg), plan, actual) => {
                let plan = plan.map(|v| v.expect_length()).transpose()?;
                let actual = actual.map(|v| v.expect_length()).transpose()?;
                Ok(AttributePair::Length(LengthAttributePair {
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
            (AttributeConfig::Text(cfg), plan, actual) => {
                let plan = plan.map(|v| v.expect_text()).transpose()?;
                let actual = actual.map(|v| v.expect_text()).transpose()?;
                Ok(AttributePair::Text(TextAttributePair {
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
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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

impl MassAttributePair {
    /// The unit this pair presents in: the actual value's unit, else the
    /// plan's, else the config's `default_unit`.
    pub fn display_unit(&self) -> MassUnit {
        self.actual
            .as_ref()
            .or(self.plan.as_ref())
            .map(|v| v.unit().clone())
            .unwrap_or_else(|| self.config.default_unit.clone())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LengthAttributePair {
    pub attr_id: Uuid,
    pub entry_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub config: LengthConfig,
    pub index_float: Option<f64>,
    pub plan: Option<LengthValue>,
    pub actual: Option<LengthValue>,
}

impl LengthAttributePair {
    /// The unit this pair presents in: the actual value's unit, else the
    /// plan's, else the config's `default_unit`. Mirrors `MassAttributePair`.
    pub fn display_unit(&self) -> LengthUnit {
        self.actual
            .as_ref()
            .or(self.plan.as_ref())
            .map(|v| v.unit().clone())
            .unwrap_or_else(|| self.config.default_unit.clone())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextAttributePair {
    pub attr_id: Uuid,
    pub entry_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub config: TextConfig,
    pub index_string: Option<String>,
    // A text value is a bare `String` (no exact/range axis), so plan/actual are
    // `Option<String>` directly.
    pub plan: Option<String>,
    pub actual: Option<String>,
}
