// Attribute/Value model sketch.
//
// Structure:
// - Attribute: common fields + AttributeConfig enum for type-specific config.
// - Value: common fields + plan/actual AttributeValues.
//   - AttributeValue: enum of type-specific values (*Value, eg NumericValue or SelectValue).
//   - *Value: type-specific enum of exact or range values.
// - Row structs (AttributeRow, ValueRow): flat DB representations using JSON-as-TEXT
//   columns. Conversion methods handle serde round-tripping via to_string/from_str.
//
// Serde:
// - All enums use default external tagging. Internally-tagged enums are incompatible with
//   serde_json's arbitrary_precision feature (enabled workspace-wide via ivm/dbsp).
// - All config/value types derive Serialize + Deserialize.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{DomainError, Result, ValidationError};

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String, // TODO: use a validated type.
    pub description: Option<String>,
    pub config: AttributeConfig,
}

impl Attribute {
    pub fn as_numeric(&self) -> Option<&NumericConfig> {
        match &self.config {
            AttributeConfig::Numeric(c) => Some(c),
            _ => None,
        }
    }

    pub fn expect_numeric(&self) -> Result<&NumericConfig> {
        match &self.config {
            AttributeConfig::Numeric(c) => Ok(c),
            _ => Err(DomainError::AttributeMismatch),
        }
    }

    pub fn expect_select(&self) -> Result<&SelectConfig> {
        match &self.config {
            AttributeConfig::Select(c) => Ok(c),
            _ => Err(DomainError::AttributeMismatch),
        }
    }

    pub fn expect_mass(&self) -> Result<&MassConfig> {
        match &self.config {
            AttributeConfig::Mass(c) => Ok(c),
            _ => Err(DomainError::AttributeMismatch),
        }
    }

    /// The scalar config default mapped to an `AttributeValue`, if this type has
    /// one. Numeric and Select carry a scalar default; Mass has only
    /// `default_units` and returns `None` here (use `seed_value` to build a Mass
    /// seed from its units).
    pub fn default_value(&self) -> Option<AttributeValue> {
        match &self.config {
            AttributeConfig::Numeric(c) => {
                c.default.map(|d| AttributeValue::Numeric(NumericValue::Exact(d)))
            }
            AttributeConfig::Select(c) => c
                .default
                .clone()
                .map(|s| AttributeValue::Select(SelectValue::Exact(s))),
            AttributeConfig::Mass(_) => None,
        }
    }

    /// Build the seed `Value` used when attaching this attribute to an entry.
    /// Both `plan` and `actual` are set to the resolved default. Scalar types use
    /// `default_value`; Mass constructs a zero-magnitude `MassMeasurement` per
    /// `default_unit` (or `None` when there are no default units). The composite
    /// key is `(entry_id, self.id)`.
    pub fn seed_value(&self, entry_id: Uuid) -> Value {
        let seed = match &self.config {
            AttributeConfig::Mass(c) if !c.default_units.is_empty() => {
                let measurements = c
                    .default_units
                    .iter()
                    .map(|unit| MassMeasurement {
                        unit: unit.clone(),
                        value: 0.0,
                    })
                    .collect();
                Some(AttributeValue::Mass(MassValue::Exact(measurements)))
            }
            AttributeConfig::Mass(_) => None,
            _ => self.default_value(),
        };
        Value {
            entry_id,
            attribute_id: self.id,
            index_float: None,
            index_string: None,
            plan: seed.clone(),
            actual: seed,
        }
    }
}

///// Configs /////

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AttributeConfig {
    Numeric(NumericConfig),
    Select(SelectConfig),
    Mass(MassConfig),
}

impl From<NumericConfig> for AttributeConfig {
    fn from(value: NumericConfig) -> Self {
        AttributeConfig::Numeric(value)
    }
}
impl From<SelectConfig> for AttributeConfig {
    fn from(value: SelectConfig) -> Self {
        AttributeConfig::Select(value)
    }
}
impl From<MassConfig> for AttributeConfig {
    fn from(value: MassConfig) -> Self {
        AttributeConfig::Mass(value)
    }
}

impl AttributeConfig {
    pub fn data_type(&self) -> &'static str {
        match self {
            AttributeConfig::Numeric(_) => "Numeric",
            AttributeConfig::Select(_) => "Select",
            AttributeConfig::Mass(_) => "Mass",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumericConfig {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub integer: bool,
    pub default: Option<f64>,
}

impl NumericConfig {
    pub fn new(
        min: Option<f64>,
        max: Option<f64>,
        integer: bool,
        default: Option<f64>,
    ) -> Result<Self> {
        fn is_integer(v: f64) -> bool {
            v.is_finite() && v == v.trunc()
        }

        if integer {
            if let Some(v) = min {
                if !is_integer(v) {
                    return Err(DomainError::Validation(
                        ValidationError::InvalidNumericConfig(format!(
                            "min ({v}) must be an integer"
                        )),
                    ));
                }
            }
            if let Some(v) = max {
                if !is_integer(v) {
                    return Err(DomainError::Validation(
                        ValidationError::InvalidNumericConfig(format!(
                            "max ({v}) must be an integer"
                        )),
                    ));
                }
            }
            if let Some(v) = default {
                if !is_integer(v) {
                    return Err(DomainError::Validation(
                        ValidationError::InvalidNumericConfig(format!(
                            "default ({v}) must be an integer"
                        )),
                    ));
                }
            }
        }

        Ok(Self {
            min,
            max,
            integer,
            default,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectConfig {
    pub options: Vec<String>,
    pub ordered: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassConfig {
    pub default_units: Vec<MassUnit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MassUnit {
    Gram,
    Kilogram,
    Pound,
}

///// Values /////

#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    // Identified by a composite key: (entry_id, attribute_id).
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
    pub index_float: Option<f64>,
    pub index_string: Option<String>,
    pub plan: Option<AttributeValue>,
    pub actual: Option<AttributeValue>,
}

// Consider renaming AttributeValue -> TypeValue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AttributeValue {
    Numeric(NumericValue),
    Select(SelectValue),
    Mass(MassValue),
}

impl AttributeValue {
    pub fn expect_numeric(self) -> Result<NumericValue> {
        match self {
            AttributeValue::Numeric(v) => Ok(v),
            _ => Err(DomainError::AttributeMismatch),
        }
    }

    pub fn expect_select(self) -> Result<SelectValue> {
        match self {
            AttributeValue::Select(v) => Ok(v),
            _ => Err(DomainError::AttributeMismatch),
        }
    }

    pub fn expect_mass(self) -> Result<MassValue> {
        match self {
            AttributeValue::Mass(v) => Ok(v),
            _ => Err(DomainError::AttributeMismatch),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NumericValue {
    Exact(f64),
    Range { min: f64, max: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SelectValue {
    Exact(String),
    Range { min: String, max: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MassValue {
    Exact(Vec<MassMeasurement>),
    Range {
        min: Vec<MassMeasurement>,
        max: Vec<MassMeasurement>,
    },
}

impl MassValue {
    pub fn defined_units(&self) -> Vec<MassUnit> {
        match self {
            MassValue::Exact(measurements) => measurements.iter().map(|m| m.unit.clone()).collect(),
            MassValue::Range { min, max } => {
                let min_units: Vec<_> = min.iter().map(|m| m.unit.clone()).collect();
                let union: Vec<MassUnit> = min_units
                    .iter()
                    .chain(
                        max.iter()
                            .map(|m| &m.unit)
                            .filter(|m| !min_units.contains(m)),
                    )
                    .cloned()
                    .collect();
                union
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassMeasurement {
    pub unit: MassUnit,
    pub value: f64,
}

