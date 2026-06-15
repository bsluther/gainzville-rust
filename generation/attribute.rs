use gv_core::models::{
    attribute::{
        Attribute, AttributeConfig, AttributeValue, LengthConfig, LengthMeasurement, LengthUnit,
        LengthValue, MAX_MULTISELECT_OPTION_LEN, MassConfig, MassMeasurement, MassUnit, MassValue,
        MultiselectConfig, NumericConfig, NumericValue, SelectConfig, SelectValue, TextConfig,
        Value,
    },
    entry::Entry,
};
use uuid::Uuid;

use rand::seq::IndexedRandom;
use rand::{RngExt, seq::IteratorRandom};

use crate::{Arbitrary, ArbitraryFrom, GenerationContext, gen_random_text, maybe, pick};

impl Arbitrary for Attribute {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let owner_id = context
            .model()
            .actors()
            .choose(rng)
            .map(|a| a.actor_id)
            .unwrap_or_else(|| Uuid::arbitrary(rng, context));
        Attribute {
            id: Uuid::arbitrary(rng, context),
            owner_id,
            name: gen_random_text(rng, 1..5).to_string(),
            description: maybe(rng, 0.5, |rng| gen_random_text(rng, 1..8).to_string()),
            config: AttributeConfig::arbitrary(rng, context),
        }
    }
}

impl Arbitrary for AttributeConfig {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        match rng.random_range(0..=5) {
            0 => AttributeConfig::Numeric(NumericConfig::arbitrary(rng, context)),
            1 => AttributeConfig::Select(SelectConfig::arbitrary(rng, context)),
            2 => AttributeConfig::Multiselect(MultiselectConfig::arbitrary(rng, context)),
            3 => AttributeConfig::Mass(MassConfig::arbitrary(rng, context)),
            4 => AttributeConfig::Length(LengthConfig::arbitrary(rng, context)),
            _ => AttributeConfig::Text(TextConfig::arbitrary(rng, context)),
        }
    }
}

impl Arbitrary for NumericConfig {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        let integer: bool = rng.random_bool(0.5);

        let rand_val = |rng: &mut R, lo: f64, hi: f64| -> f64 {
            let v = rng.random_range(lo..hi);
            // Snapping to the 2-decimal grid keeps v within [lo, hi]: lo/hi are
            // themselves grid points (or generated bounds-free), and rounding
            // is monotonic.
            if integer {
                v.round()
            } else {
                (v * 100.0).round() / 100.0
            }
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
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        let n = rng.random_range(1..=8);
        // Dedupe: short random text can collide, and the config rejects
        // duplicate options. First push always lands, so options is non-empty.
        let mut options: Vec<String> = Vec::with_capacity(n);
        for _ in 0..n {
            let option = gen_random_text(rng, 1..3);
            if !options.contains(&option) {
                options.push(option);
            }
        }
        let ordered = rng.random_bool(0.5);
        let default = maybe(rng, 0.5, |rng| pick(&options, rng).unwrap().clone());
        SelectConfig {
            options,
            ordered,
            default,
        }
    }
}

impl Arbitrary for MultiselectConfig {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        let n = rng.random_range(0..=8);
        // Dedupe and cap length: short random text can collide, and the config
        // rejects duplicate or over-long options. An empty option list is valid.
        let mut options: Vec<String> = Vec::with_capacity(n);
        for _ in 0..n {
            let option = gen_random_text(rng, 1..3);
            if option.chars().count() <= MAX_MULTISELECT_OPTION_LEN && !options.contains(&option) {
                options.push(option);
            }
        }
        // A default is a random subset of the options, kept in option order so
        // it validates (membership + no duplicates). An empty subset collapses
        // to `None` — an empty default is treated as no default.
        let default = maybe(rng, 0.5, |rng| {
            options
                .iter()
                .filter(|_| rng.random_bool(0.5))
                .cloned()
                .collect::<Vec<String>>()
        })
        .filter(|d: &Vec<String>| !d.is_empty());
        MultiselectConfig { options, default }
    }
}

impl Arbitrary for MassConfig {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        let all_units = [MassUnit::Gram, MassUnit::Kilogram, MassUnit::Pound];
        MassConfig {
            default_unit: pick(&all_units[..], rng).unwrap().clone(),
        }
    }
}

/// All length units, in the menu order used by the Swift picker.
const ALL_LENGTH_UNITS: [LengthUnit; 8] = [
    LengthUnit::Millimeter,
    LengthUnit::Centimeter,
    LengthUnit::Meter,
    LengthUnit::Kilometer,
    LengthUnit::Inch,
    LengthUnit::Foot,
    LengthUnit::Yard,
    LengthUnit::Mile,
];

impl Arbitrary for LengthConfig {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        LengthConfig {
            default_unit: pick(&ALL_LENGTH_UNITS[..], rng).unwrap().clone(),
        }
    }
}

impl Arbitrary for TextConfig {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        TextConfig {
            default: maybe(rng, 0.5, |rng| gen_random_text(rng, 1..6)),
            autocomplete: rng.random_bool(0.5),
        }
    }
}

impl Arbitrary for Value {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let model = context.model();

        // `create_value` requires `entry.owner_id == attribute.owner_id`, so we
        // need an owner-matched (entry, attribute) pair. Prefer real references —
        // fabricating only what the model can't supply keeps the value legal
        // against a Model that enforces FK/owner invariants:
        //   - entry: prefer one whose owner also owns an attribute (so we can
        //     form a full real pair), else any entry, else a fabricated id.
        //   - attribute: the chosen entry's owner-matched attribute if one
        //     exists, else a fabricated id + config.
        let eligible: Vec<&Entry> = model
            .entries()
            .filter(|e| model.attributes().any(|a| a.owner_id == e.owner_id))
            .collect();
        let entry = eligible
            .choose(rng)
            .copied()
            .or_else(|| model.entries().choose(rng));

        let entry_id = entry
            .map(|e| e.id)
            .unwrap_or_else(|| Uuid::arbitrary(rng, context));

        let attribute = entry.and_then(|e| {
            model
                .attributes()
                .filter(|a| a.owner_id == e.owner_id)
                .choose(rng)
        });
        let (attribute_id, config) = match attribute {
            Some(a) => (a.id, a.config.clone()),
            None => (
                Uuid::arbitrary(rng, context),
                AttributeConfig::arbitrary(rng, context),
            ),
        };

        let plan = maybe(rng, 0.5, |rng| {
            AttributeValue::arbitrary_from(rng, context, &config)
        });
        let actual = maybe(rng, 0.5, |rng| {
            AttributeValue::arbitrary_from(rng, context, &config)
        });
        // TODO: generate appropriate index_float / index_string based on attribute config.
        Value {
            entry_id,
            attribute_id,
            index_float: None,
            index_string: None,
            plan,
            actual,
        }
    }
}

impl ArbitraryFrom<&AttributeConfig> for AttributeValue {
    fn arbitrary_from<R: RngExt, C: GenerationContext>(
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
            // A multiselect value is a random subset of the options, kept in
            // option order so it validates (membership + no duplicates). The
            // value carries no `*Value` type, so build the `Vec<String>` here.
            AttributeConfig::Multiselect(c) => AttributeValue::Multiselect(
                c.options
                    .iter()
                    .filter(|_| rng.random_bool(0.5))
                    .cloned()
                    .collect(),
            ),
            AttributeConfig::Mass(c) => {
                AttributeValue::Mass(MassValue::arbitrary_from(rng, context, c))
            }
            AttributeConfig::Length(c) => {
                AttributeValue::Length(LengthValue::arbitrary_from(rng, context, c))
            }
            // Text has no `*Value` type or config constraint on the value, so
            // generate a bare string directly (well under the length cap).
            AttributeConfig::Text(_) => AttributeValue::Text(gen_random_text(rng, 1..8)),
        }
    }
}

impl ArbitraryFrom<&NumericConfig> for NumericValue {
    fn arbitrary_from<R: RngExt, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        config: &NumericConfig,
    ) -> Self {
        let rand_val = |rng: &mut R| -> f64 {
            let lo = config.min.unwrap_or(0.0);
            let hi = config.max.unwrap_or(lo + 1000.0);
            let v = rng.random_range(lo..hi);
            // A valid config's bounds sit on the 2-decimal grid, so the snap
            // stays within them (rounding is monotonic).
            if config.integer {
                v.round()
            } else {
                (v * 100.0).round() / 100.0
            }
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
    fn arbitrary_from<R: RngExt, C: GenerationContext>(
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
    fn arbitrary_from<R: RngExt, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        _config: &MassConfig,
    ) -> Self {
        let all_units = [MassUnit::Gram, MassUnit::Kilogram, MassUnit::Pound];
        let rand_unit = |rng: &mut R| pick(&all_units[..], rng).unwrap().clone();
        let rand_magnitude = |rng: &mut R| -> f64 {
            let v: f64 = rng.random_range(0.0..500.0);
            (v * 100.0).round() / 100.0
        };

        match rng.random_range(0..=1) {
            0 => MassValue::Exact(MassMeasurement {
                unit: rand_unit(rng),
                value: rand_magnitude(rng),
            }),
            _ => {
                let a = rand_magnitude(rng);
                let b = rand_magnitude(rng);
                let (min, max) = if a <= b { (a, b) } else { (b, a) };
                MassValue::Range {
                    unit: rand_unit(rng),
                    min,
                    max,
                }
            }
        }
    }
}

impl ArbitraryFrom<&LengthConfig> for LengthValue {
    fn arbitrary_from<R: RngExt, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        _config: &LengthConfig,
    ) -> Self {
        let rand_unit = |rng: &mut R| pick(&ALL_LENGTH_UNITS[..], rng).unwrap().clone();
        let rand_magnitude = |rng: &mut R| -> f64 {
            let v: f64 = rng.random_range(0.0..500.0);
            (v * 100.0).round() / 100.0
        };

        match rng.random_range(0..=1) {
            0 => LengthValue::Exact(LengthMeasurement {
                unit: rand_unit(rng),
                value: rand_magnitude(rng),
            }),
            _ => {
                let a = rand_magnitude(rng);
                let b = rand_magnitude(rng);
                let (min, max) = if a <= b { (a, b) } else { (b, a) };
                LengthValue::Range {
                    unit: rand_unit(rng),
                    min,
                    max,
                }
            }
        }
    }
}
