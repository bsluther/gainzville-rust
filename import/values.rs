//! Conversion of day-document attribute values (loose JSON) into typed core
//! [`AttributeValue`]s, per the target attribute's config. The importer
//! normalizes on the way in — 2-decimal rounding, unit-name parsing,
//! case-insensitive select-option matching — then core's `validate_value`
//! has the final word on the apply path.

use gv_core::models::attribute::{
    Attribute, AttributeConfig, AttributeValue, LengthMeasurement, LengthUnit, LengthValue,
    MassMeasurement, MassUnit, MassValue, NumericValue, SelectValue,
};
use serde_json::Value as Json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValueError {
    #[error("attribute '{attribute}': {message}")]
    Shape { attribute: String, message: String },
    #[error("attribute '{attribute}': '{given}' is not one of the select options {options:?}")]
    NotAnOption {
        attribute: String,
        given: String,
        options: Vec<String>,
    },
    #[error("attribute '{attribute}': unknown unit '{0}'", .unit)]
    UnknownUnit { attribute: String, unit: String },
}

/// Round to core's 2-decimal precision cap (values finer than that are
/// rejected in core; the importer rounds — docs/import-design.md).
fn round2(v: f64) -> f64 {
    if v.trunc() == v || (v * 100.0).round() / 100.0 == v {
        v
    } else {
        (v * 100.0).round() / 100.0
    }
}

/// Convert a document value into the typed value the attribute's config
/// expects.
pub fn convert(attribute: &Attribute, raw: &Json) -> Result<AttributeValue, ValueError> {
    let name = attribute.name.clone();
    let shape = |message: &str| ValueError::Shape {
        attribute: name.clone(),
        message: message.to_string(),
    };

    match &attribute.config {
        AttributeConfig::Text(_) => {
            let s = raw.as_str().ok_or_else(|| shape("expected a string"))?;
            Ok(AttributeValue::Text(s.to_string()))
        }
        AttributeConfig::Select(config) => match raw {
            Json::String(s) => Ok(AttributeValue::Select(SelectValue::Exact(match_option(
                &name,
                &config.options,
                s,
            )?))),
            Json::Object(o) => {
                let min = str_field(o, "min").ok_or_else(|| shape("range needs string min/max"))?;
                let max = str_field(o, "max").ok_or_else(|| shape("range needs string min/max"))?;
                Ok(AttributeValue::Select(SelectValue::Range {
                    min: match_option(&name, &config.options, min)?,
                    max: match_option(&name, &config.options, max)?,
                }))
            }
            _ => Err(shape("expected a string or {min,max}")),
        },
        AttributeConfig::Multiselect(config) => {
            let items = raw
                .as_array()
                .ok_or_else(|| shape("expected an array of strings"))?;
            let mut chosen = Vec::with_capacity(items.len());
            for item in items {
                let s = item.as_str().ok_or_else(|| shape("expected string items"))?;
                chosen.push(match_option(&name, &config.options, s)?);
            }
            Ok(AttributeValue::Multiselect(chosen))
        }
        AttributeConfig::Numeric(_) => match raw {
            Json::Number(n) => {
                let v = n.as_f64().ok_or_else(|| shape("expected a finite number"))?;
                Ok(AttributeValue::Numeric(NumericValue::Exact(round2(v))))
            }
            Json::Object(o) => {
                let min = num_field(o, "min").ok_or_else(|| shape("range needs numeric min/max"))?;
                let max = num_field(o, "max").ok_or_else(|| shape("range needs numeric min/max"))?;
                Ok(AttributeValue::Numeric(NumericValue::Range {
                    min: round2(min),
                    max: round2(max),
                }))
            }
            _ => Err(shape("expected a number or {min,max}")),
        },
        AttributeConfig::Mass(_) => {
            let o = raw
                .as_object()
                .ok_or_else(|| shape("expected {value,unit} or {min,max,unit}"))?;
            let unit = parse_mass_unit(&name, str_field(o, "unit").ok_or_else(|| shape("missing unit"))?)?;
            if let Some(v) = num_field(o, "value") {
                Ok(AttributeValue::Mass(MassValue::Exact(MassMeasurement {
                    unit,
                    value: round2(v),
                })))
            } else if let (Some(min), Some(max)) = (num_field(o, "min"), num_field(o, "max")) {
                Ok(AttributeValue::Mass(MassValue::Range {
                    unit,
                    min: round2(min),
                    max: round2(max),
                }))
            } else {
                Err(shape("expected {value,unit} or {min,max,unit}"))
            }
        }
        AttributeConfig::Length(_) => {
            let o = raw
                .as_object()
                .ok_or_else(|| shape("expected {value,unit} or {min,max,unit}"))?;
            let unit =
                parse_length_unit(&name, str_field(o, "unit").ok_or_else(|| shape("missing unit"))?)?;
            if let Some(v) = num_field(o, "value") {
                Ok(AttributeValue::Length(LengthValue::Exact(LengthMeasurement {
                    unit,
                    value: round2(v),
                })))
            } else if let (Some(min), Some(max)) = (num_field(o, "min"), num_field(o, "max")) {
                Ok(AttributeValue::Length(LengthValue::Range {
                    unit,
                    min: round2(min),
                    max: round2(max),
                }))
            } else {
                Err(shape("expected {value,unit} or {min,max,unit}"))
            }
        }
    }
}

/// Case-insensitively match a string onto a select/multiselect option,
/// returning the option's canonical casing.
fn match_option(attribute: &str, options: &[String], given: &str) -> Result<String, ValueError> {
    options
        .iter()
        .find(|o| o.eq_ignore_ascii_case(given))
        .cloned()
        .ok_or_else(|| ValueError::NotAnOption {
            attribute: attribute.to_string(),
            given: given.to_string(),
            options: options.to_vec(),
        })
}

fn str_field<'a>(o: &'a serde_json::Map<String, Json>, key: &str) -> Option<&'a str> {
    o.get(key).and_then(|v| v.as_str())
}

fn num_field(o: &serde_json::Map<String, Json>, key: &str) -> Option<f64> {
    o.get(key).and_then(|v| v.as_f64())
}

fn parse_mass_unit(attribute: &str, s: &str) -> Result<MassUnit, ValueError> {
    match s.to_lowercase().trim_end_matches('s') {
        "g" | "gram" => Ok(MassUnit::Gram),
        "kg" | "kilogram" | "kilo" => Ok(MassUnit::Kilogram),
        "lb" | "pound" => Ok(MassUnit::Pound),
        _ => Err(ValueError::UnknownUnit {
            attribute: attribute.to_string(),
            unit: s.to_string(),
        }),
    }
}

fn parse_length_unit(attribute: &str, s: &str) -> Result<LengthUnit, ValueError> {
    match s.to_lowercase().trim_end_matches('s') {
        "mm" | "millimeter" => Ok(LengthUnit::Millimeter),
        "cm" | "centimeter" => Ok(LengthUnit::Centimeter),
        "m" | "meter" => Ok(LengthUnit::Meter),
        "km" | "kilometer" => Ok(LengthUnit::Kilometer),
        "in" | "inch" | "inche" => Ok(LengthUnit::Inch),
        "ft" | "foot" | "feet" => Ok(LengthUnit::Foot),
        "yd" | "yard" => Ok(LengthUnit::Yard),
        "mi" | "mile" => Ok(LengthUnit::Mile),
        _ => Err(ValueError::UnknownUnit {
            attribute: attribute.to_string(),
            unit: s.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gv_core::models::attribute::{NumericConfig, SelectConfig, TextConfig};
    use serde_json::json;
    use uuid::Uuid;

    fn attr(config: impl Into<AttributeConfig>) -> Attribute {
        Attribute {
            id: Uuid::nil(),
            owner_id: Uuid::nil(),
            name: "Test".to_string(),
            description: None,
            config: config.into(),
        }
    }

    #[test]
    fn select_matches_case_insensitively_to_canonical() {
        let a = attr(SelectConfig {
            options: vec!["flash".into(), "sent".into()],
            ordered: false,
            default: None,
        });
        assert_eq!(
            convert(&a, &json!("Flash")).unwrap(),
            AttributeValue::Select(SelectValue::Exact("flash".into()))
        );
        assert!(convert(&a, &json!("onsight")).is_err());
    }

    #[test]
    fn numeric_rounds_to_two_decimals() {
        let a = attr(NumericConfig {
            min: None,
            max: None,
            integer: false,
            default: None,
        });
        assert_eq!(
            convert(&a, &json!(8.333333)).unwrap(),
            AttributeValue::Numeric(NumericValue::Exact(8.33))
        );
    }

    #[test]
    fn mass_parses_unit_names() {
        let a = attr(gv_core::models::attribute::MassConfig {
            default_unit: MassUnit::Pound,
        });
        assert_eq!(
            convert(&a, &json!({"value": 194.4, "unit": "lbs"})).unwrap(),
            AttributeValue::Mass(MassValue::Exact(MassMeasurement {
                unit: MassUnit::Pound,
                value: 194.4
            }))
        );
    }

    #[test]
    fn text_takes_strings() {
        let a = attr(TextConfig {
            default: None,
            autocomplete: true,
        });
        assert_eq!(
            convert(&a, &json!("Challenging, but fun!")).unwrap(),
            AttributeValue::Text("Challenging, but fun!".into())
        );
    }
}
