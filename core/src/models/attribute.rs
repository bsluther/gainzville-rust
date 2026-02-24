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
use sqlx::FromRow;
use uuid::Uuid;

use crate::error::{DomainError, Result, ValidationError};

#[derive(Debug, Clone)]
pub struct Attribute {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String, // TODO: use a validated type.
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
}

///// Configs /////

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
                        ValidationError::InvalidNumericConfig(format!("min ({v}) must be an integer")),
                    ));
                }
            }
            if let Some(v) = max {
                if !is_integer(v) {
                    return Err(DomainError::Validation(
                        ValidationError::InvalidNumericConfig(format!("max ({v}) must be an integer")),
                    ));
                }
            }
            if let Some(v) = default {
                if !is_integer(v) {
                    return Err(DomainError::Validation(
                        ValidationError::InvalidNumericConfig(format!("default ({v}) must be an integer")),
                    ));
                }
            }
        }

        Ok(Self { min, max, integer, default })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectConfig {
    pub options: Vec<String>,
    pub ordered: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MassConfig {
    pub default_units: Vec<MassUnit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MassUnit {
    Gram,
    Kilogram,
    Pound,
}

///// Values /////

#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NumericValue {
    Exact(f64),
    Range { min: f64, max: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SelectValue {
    Exact(String),
    Range { min: String, max: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MassValue {
    Exact(Vec<MassMeasurement>),
    Range {
        min: Vec<MassMeasurement>,
        max: Vec<MassMeasurement>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MassMeasurement {
    pub unit: MassUnit,
    pub value: f64,
}

///// Rows /////

#[derive(Debug, Clone, FromRow)]
pub struct AttributeRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub data_type: String,
    pub config: String, // JSON as TEXT
}

impl AttributeRow {
    pub fn from_attribute(attr: &Attribute) -> Result<Self> {
        Ok(Self {
            id: attr.id,
            owner_id: attr.owner_id,
            name: attr.name.clone(),
            data_type: attr.config.data_type().to_string(),
            config: serde_json::to_string(&attr.config)
                .map_err(|e| DomainError::Other(e.to_string()))?,
        })
    }

    pub fn to_attribute(self) -> Result<Attribute> {
        let config: AttributeConfig =
            serde_json::from_str(&self.config).map_err(|e| DomainError::Other(e.to_string()))?;
        Ok(Attribute {
            id: self.id,
            owner_id: self.owner_id,
            name: self.name,
            config,
        })
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ValueRow {
    // Composite PK = (entry_id, attribute_id).
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
    pub plan: Option<String>,   // JSON as TEXT
    pub actual: Option<String>, // JSON as TEXT
    pub index_float: Option<f64>,
    pub index_string: Option<String>,
}

impl ValueRow {
    pub fn from_value(value: &Value) -> Result<Self> {
        let plan = value
            .plan
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| DomainError::Other(e.to_string()))?;
        let actual = value
            .actual
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| DomainError::Other(e.to_string()))?;
        Ok(Self {
            entry_id: value.entry_id,
            attribute_id: value.attribute_id,
            plan,
            actual,
            index_float: value.index_float,
            index_string: value.index_string.clone(),
        })
    }

    pub fn to_value(self) -> Result<Value> {
        let plan: Option<AttributeValue> = self
            .plan
            .as_deref()
            .map(|v| serde_json::from_str(v))
            .transpose()
            .map_err(|e| DomainError::Other(e.to_string()))?;
        let actual: Option<AttributeValue> = self
            .actual
            .as_deref()
            .map(|v| serde_json::from_str(v))
            .transpose()
            .map_err(|e| DomainError::Other(e.to_string()))?;
        Ok(Value {
            entry_id: self.entry_id,
            attribute_id: self.attribute_id,
            index_float: self.index_float,
            index_string: self.index_string,
            plan,
            actual,
        })
    }
}
