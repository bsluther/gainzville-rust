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

    pub fn expect_length(&self) -> Result<&LengthConfig> {
        match &self.config {
            AttributeConfig::Length(c) => Ok(c),
            _ => Err(DomainError::Rejected(RejectReason::AttributeMismatch)),
        }
    }

    /// The scalar config default mapped to an `AttributeValue`, if this type has
    /// one. Numeric and Select carry a scalar default; Mass and Length have
    /// only `default_unit` and return `None` here (use `seed_value` to build a
    /// measurement seed from the unit).
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
            AttributeConfig::Length(_) => None,
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
            (AttributeConfig::Length(c), AttributeValue::Length(v)) => c.validate_value(v),
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
            AttributeConfig::Length(c) => Some(AttributeValue::Length(LengthValue::Exact(
                LengthMeasurement {
                    unit: c.default_unit.clone(),
                    value: 0.0,
                },
            ))),
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
    Length(LengthConfig),
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
impl From<LengthConfig> for AttributeConfig {
    fn from(value: LengthConfig) -> Self {
        AttributeConfig::Length(value)
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
            // Mass and Length have no cross-field coherence to check: any
            // single `default_unit` is valid.
            AttributeConfig::Mass(_) => Ok(()),
            AttributeConfig::Length(_) => Ok(()),
        }
    }

    pub fn data_type(&self) -> &'static str {
        match self {
            AttributeConfig::Numeric(_) => "Numeric",
            AttributeConfig::Select(_) => "Select",
            AttributeConfig::Mass(_) => "Mass",
            AttributeConfig::Length(_) => "Length",
        }
    }
}

/// Numeric magnitudes (values and config bounds) are capped at two decimal
/// places — finer precision has no real use and makes inputs unwieldy. On an
/// f64, "has at most 2 decimals" means: the nearest double to some 2-decimal
/// number, checked by round-tripping through that rounding. (A digit test like
/// `(v * 100.0).trunc() == v * 100.0` falsely rejects user-typed values such
/// as 0.29, whose product is 28.999…96.) The integer guard covers magnitudes
/// where `v * 100.0` overflows to infinity.
fn at_most_two_decimals(v: f64) -> bool {
    v.trunc() == v || (v * 100.0).round() / 100.0 == v
}

/// Round to the 2-decimal cap. Values already at the cap pass through
/// untouched — that branch also keeps huge magnitudes intact, where `v * 100.0`
/// would overflow to infinity.
fn round_to_two_decimals(v: f64) -> f64 {
    if at_most_two_decimals(v) {
        v
    } else {
        (v * 100.0).round() / 100.0
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
    /// `integer`), at most 2 decimal places, and ordered (min <= max); the
    /// default, when present, must be valid as a value.
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
                if !at_most_two_decimals(v) {
                    return Err(ValidationError::InvalidNumericConfig(format!(
                        "{label} ({v}) must have at most 2 decimal places"
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
    /// config demands it, at most 2 decimal places, within min/max. Range
    /// endpoints are each checked and must be ordered (min <= max).
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
            if !at_most_two_decimals(v) {
                return Err(ValidationError::InvalidValue(format!(
                    "value ({v}) must have at most 2 decimal places"
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
    /// Validate a mass value against this config: magnitudes must be finite
    /// with at most 2 decimal places, and range endpoints ordered (min <= max)
    /// — both endpoints share one unit, so no conversion is involved. The unit
    /// itself is unconstrained; `default_unit` only picks the presentation
    /// default.
    pub fn validate_value(&self, value: &MassValue) -> Result<()> {
        let check = |label: &str, v: f64| -> Result<()> {
            if !v.is_finite() {
                return Err(ValidationError::InvalidValue(format!(
                    "{label} magnitude ({v}) must be finite"
                ))
                .into());
            }
            if !at_most_two_decimals(v) {
                return Err(ValidationError::InvalidValue(format!(
                    "{label} magnitude ({v}) must have at most 2 decimal places"
                ))
                .into());
            }
            Ok(())
        };
        match value {
            MassValue::Exact(m) => check("mass value", m.value),
            MassValue::Range { unit: _, min, max } => {
                check("mass range min", *min)?;
                check("mass range max", *max)?;
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

impl MassUnit {
    /// Conversion factor to the SI base unit: kilograms per 1 of this unit.
    /// All factors are exact by definition (the international avoirdupois
    /// pound is defined as 0.45359237 kg).
    pub fn kilograms_per_unit(&self) -> f64 {
        match self {
            MassUnit::Gram => 0.001,
            MassUnit::Kilogram => 1.0,
            MassUnit::Pound => 0.45359237,
        }
    }

    /// Convert a magnitude from `self` to `to` at full precision, routing
    /// through the base unit (`self` → kg → `to`) so each new unit adds one
    /// factor rather than a row of pairwise conversions.
    pub fn convert(&self, value: f64, to: &MassUnit) -> f64 {
        value * self.kilograms_per_unit() / to.kilograms_per_unit()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LengthConfig {
    /// Unit used for the attach-time seed value and for presenting an empty
    /// (cleared) value. A stored value carries its own unit and may differ.
    pub default_unit: LengthUnit,
}

impl LengthConfig {
    /// Validate a length value against this config: magnitudes must be finite
    /// with at most 2 decimal places, and range endpoints ordered (min <= max)
    /// — both endpoints share one unit, so no conversion is involved. The unit
    /// itself is unconstrained; `default_unit` only picks the presentation
    /// default. Mirrors `MassConfig::validate_value`.
    pub fn validate_value(&self, value: &LengthValue) -> Result<()> {
        let check = |label: &str, v: f64| -> Result<()> {
            if !v.is_finite() {
                return Err(ValidationError::InvalidValue(format!(
                    "{label} magnitude ({v}) must be finite"
                ))
                .into());
            }
            if !at_most_two_decimals(v) {
                return Err(ValidationError::InvalidValue(format!(
                    "{label} magnitude ({v}) must have at most 2 decimal places"
                ))
                .into());
            }
            Ok(())
        };
        match value {
            LengthValue::Exact(m) => check("length value", m.value),
            LengthValue::Range { unit: _, min, max } => {
                check("length range min", *min)?;
                check("length range max", *max)?;
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
pub enum LengthUnit {
    Millimeter,
    Centimeter,
    Meter,
    Kilometer,
    Inch,
    Foot,
    Yard,
    Mile,
}

impl LengthUnit {
    /// Conversion factor to the SI base unit: meters per 1 of this unit. The
    /// imperial factors are exact by definition (1 inch = 0.0254 m exactly, so
    /// foot/yard/mile follow as 0.3048 / 0.9144 / 1609.344).
    pub fn meters_per_unit(&self) -> f64 {
        match self {
            LengthUnit::Millimeter => 0.001,
            LengthUnit::Centimeter => 0.01,
            LengthUnit::Meter => 1.0,
            LengthUnit::Kilometer => 1000.0,
            LengthUnit::Inch => 0.0254,
            LengthUnit::Foot => 0.3048,
            LengthUnit::Yard => 0.9144,
            LengthUnit::Mile => 1609.344,
        }
    }

    /// Convert a magnitude from `self` to `to` at full precision, routing
    /// through the base unit (`self` → m → `to`) so each new unit adds one
    /// factor rather than a row of pairwise conversions.
    pub fn convert(&self, value: f64, to: &LengthUnit) -> f64 {
        value * self.meters_per_unit() / to.meters_per_unit()
    }
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
    Length(LengthValue),
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

    pub fn expect_length(self) -> Result<LengthValue> {
        match self {
            AttributeValue::Length(v) => Ok(v),
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
    Range {
        unit: MassUnit,
        min: f64,
        max: f64,
    },
}

impl MassValue {
    pub fn unit(&self) -> &MassUnit {
        match self {
            MassValue::Exact(m) => &m.unit,
            MassValue::Range { unit, .. } => unit,
        }
    }

    /// A copy of this value re-expressed in `unit`. Magnitudes are rounded to
    /// the 2-decimal cap so the result is writable as-is — `validate_value`
    /// would reject a full-precision conversion. Same-unit conversion returns
    /// the value unchanged, so repeated re-selection never drifts.
    pub fn converted_to(&self, unit: MassUnit) -> MassValue {
        if *self.unit() == unit {
            return self.clone();
        }
        match self {
            MassValue::Exact(m) => MassValue::Exact(MassMeasurement {
                value: round_to_two_decimals(m.unit.convert(m.value, &unit)),
                unit,
            }),
            // Positive factors and monotone rounding preserve min <= max.
            MassValue::Range {
                unit: from,
                min,
                max,
            } => MassValue::Range {
                min: round_to_two_decimals(from.convert(*min, &unit)),
                max: round_to_two_decimals(from.convert(*max, &unit)),
                unit,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassMeasurement {
    pub unit: MassUnit,
    pub value: f64,
}

/// One measurement in one unit, mirroring `MassValue`. Composite formats
/// (e.g. "5 ft 4 in") are a presentation concern — see
/// docs/attributes-design.md "Single measurement per value".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LengthValue {
    Exact(LengthMeasurement),
    /// Both endpoints share one unit, so ordering needs no conversion.
    Range {
        unit: LengthUnit,
        min: f64,
        max: f64,
    },
}

impl LengthValue {
    pub fn unit(&self) -> &LengthUnit {
        match self {
            LengthValue::Exact(m) => &m.unit,
            LengthValue::Range { unit, .. } => unit,
        }
    }

    /// A copy of this value re-expressed in `unit`. Magnitudes are rounded to
    /// the 2-decimal cap so the result is writable as-is — `validate_value`
    /// would reject a full-precision conversion. Same-unit conversion returns
    /// the value unchanged, so repeated re-selection never drifts. Mirrors
    /// `MassValue::converted_to`.
    pub fn converted_to(&self, unit: LengthUnit) -> LengthValue {
        if *self.unit() == unit {
            return self.clone();
        }
        match self {
            LengthValue::Exact(m) => LengthValue::Exact(LengthMeasurement {
                value: round_to_two_decimals(m.unit.convert(m.value, &unit)),
                unit,
            }),
            // Positive factors and monotone rounding preserve min <= max.
            LengthValue::Range {
                unit: from,
                min,
                max,
            } => LengthValue::Range {
                min: round_to_two_decimals(from.convert(*min, &unit)),
                max: round_to_two_decimals(from.convert(*max, &unit)),
                unit,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LengthMeasurement {
    pub unit: LengthUnit,
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
        assert!(
            cfg.validate_value(&NumericValue::Range { min: 1.0, max: 9.0 })
                .is_ok()
        );
        assert!(rejected_validation(
            &cfg.validate_value(&NumericValue::Exact(2.5))
        )); // non-integer
        assert!(rejected_validation(
            &cfg.validate_value(&NumericValue::Exact(-1.0))
        )); // below min
        assert!(rejected_validation(
            &cfg.validate_value(&NumericValue::Exact(11.0))
        )); // above max
        assert!(rejected_validation(
            &cfg.validate_value(&NumericValue::Exact(f64::NAN))
        ));
        assert!(rejected_validation(
            &cfg.validate_value(&NumericValue::Range { min: 9.0, max: 1.0 })
        ));
    }

    #[test]
    fn numeric_precision_validation() {
        let cfg = NumericConfig {
            min: None,
            max: None,
            integer: false,
            default: None,
        };
        // 0.29 is not exactly representable (0.29 * 100.0 == 28.999…96); the
        // check must not reject values the user can actually type.
        assert!(cfg.validate_value(&NumericValue::Exact(0.29)).is_ok());
        assert!(cfg.validate_value(&NumericValue::Exact(4.52)).is_ok());
        assert!(cfg.validate_value(&NumericValue::Exact(-3.99)).is_ok());
        // Huge magnitudes are integers; v * 100.0 overflowing must not reject.
        assert!(cfg.validate_value(&NumericValue::Exact(1e307)).is_ok());
        assert!(rejected_validation(
            &cfg.validate_value(&NumericValue::Exact(4.523))
        ));
        assert!(rejected_validation(&cfg.validate_value(
            &NumericValue::Range {
                min: 1.001,
                max: 9.0
            }
        )));
        assert!(rejected_validation(&cfg.validate_value(
            &NumericValue::Range {
                min: 1.0,
                max: 8.999
            }
        )));
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
        assert!(
            opts(false)
                .validate_value(&SelectValue::Exact("mid".to_string()))
                .is_ok()
        );
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
        // Bounds and defaults are capped at 2 decimal places like values.
        assert!(NumericConfig::new(Some(0.125), None, false, None).is_err());
        assert!(NumericConfig::new(None, Some(9.999), false, None).is_err());
        assert!(NumericConfig::new(None, None, false, Some(1.234)).is_err());
        assert!(NumericConfig::new(Some(0.25), Some(9.75), false, Some(1.29)).is_ok());

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
        assert!(
            cfg.validate_value(&MassValue::Exact(m(MassUnit::Kilogram, 20.0)))
                .is_ok()
        );
        assert!(
            cfg.validate_value(&MassValue::Range {
                unit: MassUnit::Pound,
                min: 45.0,
                max: 55.0,
            })
            .is_ok()
        );
        assert!(rejected_validation(&cfg.validate_value(&MassValue::Exact(
            m(MassUnit::Pound, f64::INFINITY)
        ))));
        assert!(rejected_validation(&cfg.validate_value(
            &MassValue::Range {
                unit: MassUnit::Pound,
                min: 55.0,
                max: 45.0, // inverted
            }
        )));
        // Magnitudes share the 2-decimal precision cap.
        assert!(
            cfg.validate_value(&MassValue::Exact(m(MassUnit::Kilogram, 8.29)))
                .is_ok()
        );
        assert!(rejected_validation(&cfg.validate_value(&MassValue::Exact(
            m(MassUnit::Kilogram, 8.872)
        ))));
        assert!(rejected_validation(&cfg.validate_value(
            &MassValue::Range {
                unit: MassUnit::Pound,
                min: 45.005,
                max: 55.0,
            }
        )));
    }

    #[test]
    fn mass_conversion() {
        let m = |unit, value| MassValue::Exact(MassMeasurement { unit, value });
        // Exact factor between metric units.
        assert_eq!(
            m(MassUnit::Kilogram, 1.5).converted_to(MassUnit::Gram),
            m(MassUnit::Gram, 1500.0)
        );
        // 20 lb = 9.0718474 kg, rounded to the 2-decimal cap.
        assert_eq!(
            m(MassUnit::Pound, 20.0).converted_to(MassUnit::Kilogram),
            m(MassUnit::Kilogram, 9.07)
        );
        // Same-unit conversion is identity, not a rounded round trip.
        assert_eq!(
            m(MassUnit::Pound, 20.0).converted_to(MassUnit::Pound),
            m(MassUnit::Pound, 20.0)
        );
        // Ranges convert both endpoints in one move.
        assert_eq!(
            MassValue::Range {
                unit: MassUnit::Pound,
                min: 45.0,
                max: 55.0,
            }
            .converted_to(MassUnit::Kilogram),
            MassValue::Range {
                unit: MassUnit::Kilogram,
                min: 20.41,
                max: 24.95,
            }
        );
    }

    #[test]
    fn length_validation() {
        let cfg = LengthConfig {
            default_unit: LengthUnit::Meter,
        };
        let m = |unit, value| LengthMeasurement { unit, value };
        // The value's unit is free to differ from the config default.
        assert!(
            cfg.validate_value(&LengthValue::Exact(m(LengthUnit::Kilometer, 5.0)))
                .is_ok()
        );
        assert!(
            cfg.validate_value(&LengthValue::Range {
                unit: LengthUnit::Mile,
                min: 3.0,
                max: 5.0,
            })
            .is_ok()
        );
        assert!(rejected_validation(&cfg.validate_value(
            &LengthValue::Exact(m(LengthUnit::Meter, f64::INFINITY))
        )));
        assert!(rejected_validation(&cfg.validate_value(
            &LengthValue::Range {
                unit: LengthUnit::Meter,
                min: 5.0,
                max: 3.0, // inverted
            }
        )));
        // Magnitudes share the 2-decimal precision cap.
        assert!(
            cfg.validate_value(&LengthValue::Exact(m(LengthUnit::Millimeter, 8.29)))
                .is_ok()
        );
        assert!(rejected_validation(&cfg.validate_value(
            &LengthValue::Exact(m(LengthUnit::Millimeter, 8.872))
        )));
    }

    #[test]
    fn length_conversion() {
        let m = |unit, value| LengthValue::Exact(LengthMeasurement { unit, value });
        // Exact factor between metric units.
        assert_eq!(
            m(LengthUnit::Kilometer, 1.5).converted_to(LengthUnit::Meter),
            m(LengthUnit::Meter, 1500.0)
        );
        // 1 mile = 1609.344 m, rounded to the 2-decimal cap.
        assert_eq!(
            m(LengthUnit::Mile, 1.0).converted_to(LengthUnit::Meter),
            m(LengthUnit::Meter, 1609.34)
        );
        // 12 inches = 1 foot exactly.
        assert_eq!(
            m(LengthUnit::Inch, 12.0).converted_to(LengthUnit::Foot),
            m(LengthUnit::Foot, 1.0)
        );
        // Same-unit conversion is identity, not a rounded round trip.
        assert_eq!(
            m(LengthUnit::Inch, 12.0).converted_to(LengthUnit::Inch),
            m(LengthUnit::Inch, 12.0)
        );
        // Ranges convert both endpoints in one move.
        assert_eq!(
            LengthValue::Range {
                unit: LengthUnit::Kilometer,
                min: 1.0,
                max: 2.0,
            }
            .converted_to(LengthUnit::Meter),
            LengthValue::Range {
                unit: LengthUnit::Meter,
                min: 1000.0,
                max: 2000.0,
            }
        );
    }
}
