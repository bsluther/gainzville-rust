use gv_core::models::{
    attribute::{
        Attribute, AttributeConfig, AttributeValue, MassConfig, MassMeasurement, MassUnit,
        MassValue, NumericConfig, NumericValue, SelectConfig, SelectValue, Value,
    },
    entry::Entry,
};
use uuid::Uuid;

use rand::seq::SliceRandom;

use crate::{Arbitrary, ArbitraryFrom, GenerationContext, gen_random_text, maybe, pick};

impl ArbitraryFrom<&[Uuid]> for Attribute {
    fn arbitrary_from<R: rand::Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        actor_ids: &[Uuid],
    ) -> Self {
        Attribute {
            id: Uuid::arbitrary(rng, context),
            owner_id: pick(&actor_ids, rng)
                .expect("owner_ids must not be empty")
                .clone(),
            name: gen_random_text(rng, 1..5).to_string(),
            config: AttributeConfig::arbitrary(rng, context),
        }
    }
}

impl Arbitrary for AttributeConfig {
    fn arbitrary<R: rand::Rng, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        match rng.random_range(0..=2) {
            0 => AttributeConfig::Numeric(NumericConfig::arbitrary(rng, context)),
            1 => AttributeConfig::Select(SelectConfig::arbitrary(rng, context)),
            _ => AttributeConfig::Mass(MassConfig::arbitrary(rng, context)),
        }
    }
}

impl Arbitrary for NumericConfig {
    fn arbitrary<R: rand::Rng, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        let integer: bool = rng.random_bool(0.5);

        let rand_val = |rng: &mut R, lo: f64, hi: f64| -> f64 {
            let v = rng.random_range(lo..hi);
            if integer { v.round() } else { v }
        };

        let min: Option<f64> = maybe(rng, 0.5, |rng| rand_val(rng, 0.0, 1000.0));
        let max: Option<f64> = maybe(rng, 0.5, |rng| match min {
            Some(min) => rand_val(rng, min, min + 1000.0),
            None => rand_val(rng, 0.0, 1000.0),
        });
        let default: Option<f64> = maybe(rng, 0.5, |rng| match (min, max) {
            (Some(lo), Some(hi)) => rand_val(rng, lo, hi),
            (Some(lo), None) => rand_val(rng, lo, lo + 1000.0),
            (None, Some(hi)) => rand_val(rng, 0.0, hi),
            (None, None) => rand_val(rng, 0.0, 1000.0),
        });

        NumericConfig::new(min, max, integer, default)
            .expect("generated NumericConfig should be valid")
    }
}

impl Arbitrary for SelectConfig {
    fn arbitrary<R: rand::Rng, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        let n = rng.random_range(1..=8);
        let options: Vec<String> = (0..n).map(|_| gen_random_text(rng, 1..3)).collect();
        let ordered = rng.random_bool(0.5);
        let default = maybe(rng, 0.5, |rng| pick(&options, rng).unwrap().clone());
        SelectConfig {
            options,
            ordered,
            default,
        }
    }
}

impl Arbitrary for MassConfig {
    fn arbitrary<R: rand::Rng, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        let all_units = [MassUnit::Gram, MassUnit::Kilogram, MassUnit::Pound];
        let n = rng.random_range(1..=all_units.len());
        let mut units: Vec<MassUnit> = all_units.to_vec();
        units.shuffle(rng);
        units.truncate(n);
        MassConfig {
            default_units: units,
        }
    }
}

impl ArbitraryFrom<(&[Entry], &[Attribute])> for Value {
    fn arbitrary_from<R: rand::Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        (entries, attributes): (&[Entry], &[Attribute]),
    ) -> Self {
        // Pick a (entry, attribute) pair whose owners match — `create_value`
        // requires `entry.owner_id == attribute.owner_id`, so picking
        // independently from both lists frequently produces values that the
        // mutator then rejects.
        let entry = pick(entries, rng).expect("entries must not be empty");
        let owned_attrs: Vec<&Attribute> = attributes
            .iter()
            .filter(|a| a.owner_id == entry.owner_id)
            .collect();
        let attribute = pick(&owned_attrs[..], rng).expect(
            "no attribute matches the picked entry's owner; \
             ensure attributes and entries share at least one owner",
        );
        let plan = maybe(rng, 0.5, |rng| {
            AttributeValue::arbitrary_from(rng, context, &attribute.config)
        });
        // TEMPORARY - always generate an actual value for UI development.
        let actual = maybe(rng, 0.5, |rng| {
            AttributeValue::arbitrary_from(rng, context, &attribute.config)
        });
        // TODO: generate appropriate index_float / index_string based on attribute config.
        Value {
            entry_id: entry.id,
            attribute_id: attribute.id,
            index_float: None,
            index_string: None,
            plan,
            actual,
        }
    }
}

impl ArbitraryFrom<&AttributeConfig> for AttributeValue {
    fn arbitrary_from<R: rand::Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        config: &AttributeConfig,
    ) -> Self {
        match config {
            AttributeConfig::Numeric(c) => {
                AttributeValue::Numeric(NumericValue::arbitrary_from(rng, context, c))
            }
            AttributeConfig::Select(c) => {
                AttributeValue::Select(SelectValue::arbitrary_from(rng, context, c))
            }
            AttributeConfig::Mass(c) => {
                AttributeValue::Mass(MassValue::arbitrary_from(rng, context, c))
            }
        }
    }
}

impl ArbitraryFrom<&NumericConfig> for NumericValue {
    fn arbitrary_from<R: rand::Rng, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        config: &NumericConfig,
    ) -> Self {
        let rand_val = |rng: &mut R| -> f64 {
            let lo = config.min.unwrap_or(0.0);
            let hi = config.max.unwrap_or(lo + 1000.0);
            let v = rng.random_range(lo..hi);
            if config.integer { v.round() } else { v }
        };

        match rng.random_range(0..=1) {
            0 => NumericValue::Exact(rand_val(rng)),
            _ => {
                let a = rand_val(rng);
                let b = rand_val(rng);
                let (min, max) = if a <= b { (a, b) } else { (b, a) };
                NumericValue::Range { min, max }
            }
        }
    }
}

impl ArbitraryFrom<&SelectConfig> for SelectValue {
    fn arbitrary_from<R: rand::Rng, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        config: &SelectConfig,
    ) -> Self {
        let pick_option = |rng: &mut R| -> String {
            pick(&config.options, rng)
                .expect("options must not be empty")
                .clone()
        };

        if config.ordered && rng.random_bool(0.5) {
            let a = rng.random_range(0..config.options.len());
            let b = rng.random_range(0..config.options.len());
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            SelectValue::Range {
                min: config.options[lo].clone(),
                max: config.options[hi].clone(),
            }
        } else {
            SelectValue::Exact(pick_option(rng))
        }
    }
}

impl ArbitraryFrom<&MassConfig> for MassValue {
    fn arbitrary_from<R: rand::Rng, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        _config: &MassConfig,
    ) -> Self {
        let all_units = [MassUnit::Gram, MassUnit::Kilogram, MassUnit::Pound];
        let rand_measurements = |rng: &mut R| -> Vec<MassMeasurement> {
            let n = rng.random_range(1..=all_units.len());
            let mut units = all_units.to_vec();
            units.shuffle(rng);
            units.truncate(n);
            units
                .into_iter()
                .map(|unit| MassMeasurement {
                    unit,
                    value: rng.random_range(0.0..500.0),
                })
                .collect()
        };

        // TEMPORARY: always generate exact values for UI development.
        match rng.random_range(0..=0) {
            0 => MassValue::Exact(rand_measurements(rng)),
            _ => {
                let a = rand_measurements(rng);
                let b = rand_measurements(rng);
                let (min, max) = if a[0].value <= b[0].value {
                    (a, b)
                } else {
                    (b, a)
                };
                MassValue::Range { min, max }
            }
        }
    }
}
