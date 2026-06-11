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

use crate::error::{DomainError, RejectReason, Result, ValidationError};

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
            _ => Err(DomainError::Rejected(RejectReason::AttributeMismatch)),
        }
    }

    pub fn expect_select(&self) -> Result<&SelectConfig> {
        match &self.config {
            AttributeConfig::Select(c) => Ok(c),
            _ => Err(DomainError::Rejected(RejectReason::AttributeMismatch)),
        }
    }

    pub fn expect_mass(&self) -> Result<&MassConfig> {
        match &self.config {
            AttributeConfig::Mass(c) => Ok(c),
            _ => Err(DomainError::Rejected(RejectReason::AttributeMismatch)),
        }
    }

    /// The scalar config default mapped to an `AttributeValue`, if this type has
    /// one. Numeric and Select carry a scalar default; Mass has only
    /// `default_unit` and returns `None` here (use `seed_value` to build a Mass
    /// seed from its unit).
    pub fn default_value(&self) -> Option<AttributeValue> {
        match &self.config {
            AttributeConfig::Numeric(c) => c
                .default
                .map(|d| AttributeValue::Numeric(NumericValue::Exact(d))),
            AttributeConfig::Select(c) => c
                .default
                .clone()
                .map(|s| AttributeValue::Select(SelectValue::Exact(s))),
            AttributeConfig::Mass(_) => None,
        }
    }

    /// Validate a value against this attribute's config: the variant must match
    /// the config type (numeric value on a numeric attribute, etc.), and the
    /// inner value must satisfy the config's constraints. Applied on every
    /// value write path (`CreateValue`, `UpdateAttributeValue`); configs are
    /// additive-only (options never removed, bounds never shrunk), so values
    /// admitted here stay conformant.
    pub fn validate_value(&self, value: &AttributeValue) -> Result<()> {
        match (&self.config, value) {
            (AttributeConfig::Numeric(c), AttributeValue::Numeric(v)) => c.validate_value(v),
            (AttributeConfig::Select(c), AttributeValue::Select(v)) => c.validate_value(v),
            (AttributeConfig::Mass(c), AttributeValue::Mass(v)) => c.validate_value(v),
            _ => Err(DomainError::Rejected(RejectReason::AttributeMismatch)),
        }
    }

    /// Build the seed `Value` used when attaching this attribute to an entry.
    /// Both `plan` and `actual` are set to the resolved default. Scalar types use
    /// `default_value`; Mass constructs a zero-magnitude `MassMeasurement` in the
    /// config's `default_unit`. The composite key is `(entry_id, self.id)`.
    pub fn seed_value(&self, entry_id: Uuid) -> Value {
        let seed = match &self.config {
            AttributeConfig::Mass(c) => {
                Some(AttributeValue::Mass(MassValue::Exact(MassMeasurement {
                    unit: c.default_unit.clone(),
                    value: 0.0,
                })))
            }
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
    /// Validate the config itself (applied at `CreateAttribute`): bounds and
    /// defaults must be coherent, so values and seeds derived from the config
    /// pass `validate_value`. Per-field edits (`Set*Default`) re-check their
    /// own field in `update_attribute`.
    pub fn validate(&self) -> Result<()> {
        match self {
            AttributeConfig::Numeric(c) => c.validate(),
            AttributeConfig::Select(c) => c.validate(),
            // Mass has no cross-field coherence to check: any single
            // `default_unit` is valid.
            AttributeConfig::Mass(_) => Ok(()),
        }
    }

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
        let cfg = Self {
            min,
            max,
            integer,
            default,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Validate the config itself: bounds must be finite (and integers when
    /// `integer`) and ordered (min <= max); the default, when present, must be
    /// valid as a value.
    pub fn validate(&self) -> Result<()> {
        for (label, bound) in [("min", self.min), ("max", self.max)] {
            if let Some(v) = bound {
                if !v.is_finite() {
                    return Err(ValidationError::InvalidNumericConfig(format!(
                        "{label} ({v}) must be finite"
                    ))
                    .into());
                }
                if self.integer && v.trunc() != v {
                    return Err(ValidationError::InvalidNumericConfig(format!(
                        "{label} ({v}) must be an integer"
                    ))
                    .into());
                }
            }
        }
        if let (Some(min), Some(max)) = (self.min, self.max) {
            if min > max {
                return Err(ValidationError::InvalidNumericConfig(format!(
                    "min ({min}) is above max ({max})"
                ))
                .into());
            }
        }
        if let Some(d) = self.default {
            self.validate_value(&NumericValue::Exact(d))?;
        }
        Ok(())
    }

    /// Validate a numeric value against this config: finite, integer when the
    /// config demands it, within min/max. Range endpoints are each checked and
    /// must be ordered (min <= max).
    pub fn validate_value(&self, value: &NumericValue) -> Result<()> {
        let check = |v: f64| -> Result<()> {
            if !v.is_finite() {
                return Err(
                    ValidationError::InvalidValue(format!("value ({v}) must be finite")).into(),
                );
            }
            if self.integer && v.trunc() != v {
                return Err(ValidationError::InvalidValue(format!(
                    "value ({v}) must be an integer"
                ))
                .into());
            }
            if let Some(min) = self.min {
                if v < min {
                    return Err(ValidationError::InvalidValue(format!(
                        "value ({v}) is below min ({min})"
                    ))
                    .into());
                }
            }
            if let Some(max) = self.max {
                if v > max {
                    return Err(ValidationError::InvalidValue(format!(
                        "value ({v}) is above max ({max})"
                    ))
                    .into());
                }
            }
            Ok(())
        };
        match value {
            NumericValue::Exact(v) => check(*v),
            NumericValue::Range { min, max } => {
                check(*min)?;
                check(*max)?;
                if min > max {
                    return Err(ValidationError::InvalidValue(format!(
                        "range min ({min}) is above range max ({max})"
                    ))
                    .into());
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectConfig {
    pub options: Vec<String>,
    pub ordered: bool,
    pub default: Option<String>,
}

impl SelectConfig {
    /// Validate the config itself: options must be unique (duplicates make the
    /// option order ambiguous); the default, when present, must be a member.
    /// An empty options list is allowed — options are added incrementally when
    /// authoring.
    pub fn validate(&self) -> Result<()> {
        for (i, option) in self.options.iter().enumerate() {
            if self.options[..i].contains(option) {
                return Err(ValidationError::InvalidSelectConfig(format!(
                    "duplicate option '{option}'"
                ))
                .into());
            }
        }
        if let Some(d) = &self.default {
            self.validate_value(&SelectValue::Exact(d.clone()))?;
        }
        Ok(())
    }

    /// Validate a select value against this config: every option must be a
    /// member of `options`. Ranges additionally require an `ordered` config and
    /// endpoints ordered by their position in `options` (min <= max).
    pub fn validate_value(&self, value: &SelectValue) -> Result<()> {
        let member_index = |s: &String| -> Result<usize> {
            self.options.iter().position(|o| o == s).ok_or_else(|| {
                ValidationError::InvalidValue(format!("'{s}' is not one of the select options"))
                    .into()
            })
        };
        match value {
            SelectValue::Exact(s) => member_index(s).map(|_| ()),
            SelectValue::Range { min, max } => {
                if !self.ordered {
                    return Err(ValidationError::InvalidValue(
                        "range value on an unordered select".to_string(),
                    )
                    .into());
                }
                let lo = member_index(min)?;
                let hi = member_index(max)?;
                if lo > hi {
                    return Err(ValidationError::InvalidValue(format!(
                        "range min ('{min}') comes after range max ('{max}') in the option order"
                    ))
                    .into());
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassConfig {
    /// Unit used for the attach-time seed value and for presenting an empty
    /// (cleared) value. A stored value carries its own unit and may differ.
    pub default_unit: MassUnit,
}

impl MassConfig {
    /// Validate a mass value against this config: magnitudes must be finite,
    /// and range endpoints ordered (min <= max) — both endpoints share one
    /// unit, so no conversion is involved. The unit itself is unconstrained;
    /// `default_unit` only picks the presentation default.
    pub fn validate_value(&self, value: &MassValue) -> Result<()> {
        let finite = |label: &str, v: f64| -> Result<()> {
            if !v.is_finite() {
                return Err(ValidationError::InvalidValue(format!(
                    "{label} magnitude ({v}) must be finite"
                ))
                .into());
            }
            Ok(())
        };
        match value {
            MassValue::Exact(m) => finite("mass value", m.value),
            MassValue::Range { unit: _, min, max } => {
                finite("mass range min", *min)?;
                finite("mass range max", *max)?;
                if min > max {
                    return Err(ValidationError::InvalidValue(format!(
                        "range min ({min}) is above range max ({max})"
                    ))
                    .into());
                }
                Ok(())
            }
        }
    }
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

impl Value {
    /// Copy a template value onto a new entry, re-keying to `entry_id` and
    /// keeping the attribute, indices, plan, and actual. Mirrors
    /// `Entry::from_template` for instantiation.
    pub fn from_template(template: &Value, entry_id: Uuid) -> Value {
        Value {
            entry_id,
            ..template.clone()
        }
    }
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
            _ => Err(DomainError::Rejected(RejectReason::AttributeMismatch)),
        }
    }

    pub fn expect_select(self) -> Result<SelectValue> {
        match self {
            AttributeValue::Select(v) => Ok(v),
            _ => Err(DomainError::Rejected(RejectReason::AttributeMismatch)),
        }
    }

    pub fn expect_mass(self) -> Result<MassValue> {
        match self {
            AttributeValue::Mass(v) => Ok(v),
            _ => Err(DomainError::Rejected(RejectReason::AttributeMismatch)),
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

/// One measurement in one unit. Composite formats (e.g. "5 lb 4 oz") are a
/// presentation concern — see docs/attributes-design.md "Single measurement
/// per value".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MassValue {
    Exact(MassMeasurement),
    /// Both endpoints share one unit, so ordering needs no conversion.
    Range { unit: MassUnit, min: f64, max: f64 },
}

impl MassValue {
    pub fn unit(&self) -> &MassUnit {
        match self {
            MassValue::Exact(m) => &m.unit,
            MassValue::Range { unit, .. } => unit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassMeasurement {
    pub unit: MassUnit,
    pub value: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejected_validation(r: &Result<()>) -> bool {
        matches!(
            r,
            Err(DomainError::Rejected(RejectReason::Validation(
                ValidationError::InvalidValue(_)
            )))
        )
    }

    fn attr(config: impl Into<AttributeConfig>) -> Attribute {
        Attribute {
            id: Uuid::nil(),
            owner_id: Uuid::nil(),
            name: "test".to_string(),
            description: None,
            config: config.into(),
        }
    }

    #[test]
    fn validate_value_rejects_type_mismatch() {
        let a = attr(SelectConfig {
            options: vec!["a".to_string()],
            ordered: false,
            default: None,
        });
        let r = a.validate_value(&AttributeValue::Numeric(NumericValue::Exact(1.0)));
        assert!(matches!(
            r,
            Err(DomainError::Rejected(RejectReason::AttributeMismatch))
        ));
    }

    #[test]
    fn numeric_validation() {
        let cfg = NumericConfig {
            min: Some(0.0),
            max: Some(10.0),
            integer: true,
            default: None,
        };
        assert!(cfg.validate_value(&NumericValue::Exact(5.0)).is_ok());
        assert!(cfg
            .validate_value(&NumericValue::Range { min: 1.0, max: 9.0 })
            .is_ok());
        assert!(rejected_validation(&cfg.validate_value(&NumericValue::Exact(2.5)))); // non-integer
        assert!(rejected_validation(&cfg.validate_value(&NumericValue::Exact(-1.0)))); // below min
        assert!(rejected_validation(&cfg.validate_value(&NumericValue::Exact(11.0)))); // above max
        assert!(rejected_validation(
            &cfg.validate_value(&NumericValue::Exact(f64::NAN))
        ));
        assert!(rejected_validation(
            &cfg.validate_value(&NumericValue::Range { min: 9.0, max: 1.0 })
        ));
    }

    #[test]
    fn select_validation() {
        let opts = |ordered| SelectConfig {
            options: vec!["low".to_string(), "mid".to_string(), "high".to_string()],
            ordered,
            default: None,
        };
        let in_range = SelectValue::Range {
            min: "low".to_string(),
            max: "high".to_string(),
        };
        assert!(opts(false)
            .validate_value(&SelectValue::Exact("mid".to_string()))
            .is_ok());
        assert!(opts(true).validate_value(&in_range).is_ok());
        assert!(rejected_validation(
            &opts(false).validate_value(&SelectValue::Exact("nope".to_string()))
        ));
        assert!(rejected_validation(&opts(false).validate_value(&in_range))); // range on unordered
        assert!(rejected_validation(
            // endpoints reversed in option order
            &opts(true).validate_value(&SelectValue::Range {
                min: "high".to_string(),
                max: "low".to_string(),
            })
        ));
    }

    #[test]
    fn config_validation() {
        // Numeric: unordered bounds, out-of-bounds default, non-integer bound.
        assert!(NumericConfig::new(Some(10.0), Some(0.0), false, None).is_err());
        assert!(NumericConfig::new(Some(0.0), Some(10.0), false, Some(11.0)).is_err());
        assert!(NumericConfig::new(Some(0.5), None, true, None).is_err());
        assert!(NumericConfig::new(Some(0.0), Some(10.0), true, Some(5.0)).is_ok());

        // Select: duplicate options, non-member default; empty options allowed.
        let select = |options: &[&str], default: Option<&str>| SelectConfig {
            options: options.iter().map(|s| s.to_string()).collect(),
            ordered: false,
            default: default.map(|s| s.to_string()),
        };
        assert!(select(&["a", "b", "a"], None).validate().is_err());
        assert!(select(&["a", "b"], Some("c")).validate().is_err());
        assert!(select(&[], None).validate().is_ok());
        assert!(select(&["a", "b"], Some("b")).validate().is_ok());

    }

    #[test]
    fn mass_validation() {
        let cfg = MassConfig {
            default_unit: MassUnit::Pound,
        };
        let m = |unit, value| MassMeasurement { unit, value };
        // The value's unit is free to differ from the config default.
        assert!(cfg
            .validate_value(&MassValue::Exact(m(MassUnit::Kilogram, 20.0)))
            .is_ok());
        assert!(cfg
            .validate_value(&MassValue::Range {
                unit: MassUnit::Pound,
                min: 45.0,
                max: 55.0,
            })
            .is_ok());
        assert!(rejected_validation(&cfg.validate_value(&MassValue::Exact(
            m(MassUnit::Pound, f64::INFINITY)
        ))));
        assert!(rejected_validation(&cfg.validate_value(&MassValue::Range {
            unit: MassUnit::Pound,
            min: 55.0,
            max: 45.0, // inverted
        })));
    }
}
