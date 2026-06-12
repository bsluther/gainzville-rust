use generation::{Arbitrary, SimulationContext};
use gv_core::models::attribute::{MassConfig, MassMeasurement, MassUnit, MassValue};
use gv_core::models::entry::Entry;
use gv_core::models::user::User;
use gv_core::validation::Username;
use gv_sql::rows::EntryRow;
use hegel::extras::rand::randoms;
use hegel::generators as gs;
use hegel::{TestCase, extras::rand::HegelRandom};
use tracing_subscriber::{fmt, prelude::*};

pub struct HegelHarness {
    pub rng: HegelRandom,
    ctx: SimulationContext,
}

impl HegelHarness {
    pub fn new(tc: &TestCase) -> Self {
        Self {
            rng: tc.draw(randoms()),
            ctx: SimulationContext::default(),
        }
    }

    pub fn arbitrary<T: Arbitrary>(&mut self) -> T {
        T::arbitrary(&mut self.rng, &self.ctx)
    }
}

#[hegel::test]
fn find_descendants(tc: TestCase) {
    let mut h = HegelHarness::new(&tc);
}

fn mass_unit(tc: &TestCase) -> MassUnit {
    tc.draw(gs::sampled_from(vec![
        MassUnit::Gram,
        MassUnit::Kilogram,
        MassUnit::Pound,
    ]))
}

/// A magnitude valid under the 2-decimal precision cap, drawn as integer
/// hundredths so it's exact by construction.
fn mass_magnitude(tc: &TestCase) -> f64 {
    tc.draw(
        gs::integers::<i64>()
            .min_value(-10_000_000)
            .max_value(10_000_000),
    ) as f64
        / 100.0
}

#[hegel::test]
fn mass_unit_conversion_round_trip(tc: TestCase) {
    let from = mass_unit(&tc);
    let to = mass_unit(&tc);
    // Bounded well below where the kg → g leg overflows to infinity — the
    // round trip genuinely doesn't hold there, and no physical mass is close.
    let v = tc.draw(gs::floats::<f64>().min_value(-1e12).max_value(1e12));
    // Near-subnormal magnitudes are remapped to 0: the g → kg leg pushes them
    // subnormal, where float arithmetic loses the relative precision this
    // property asserts. Found by hegel at 2.2e-313.
    let v = if v.abs() < 1e-300 { 0.0 } else { v };
    let back = to.convert(from.convert(v, &to), &from);
    assert!(
        (back - v).abs() <= v.abs() * 1e-12,
        "{v} {from:?} -> {to:?} -> {back}"
    );
}

#[hegel::test]
fn mass_value_converted_to_round_trip(tc: TestCase) {
    let from = mass_unit(&tc);
    let to = mass_unit(&tc);
    let v = mass_magnitude(&tc);
    let original = MassValue::Exact(MassMeasurement {
        unit: from.clone(),
        value: v,
    });
    let back = original
        .converted_to(to.clone())
        .converted_to(from.clone());
    if from == to {
        assert_eq!(back, original);
        return;
    }
    // Each leg rounds to 2 decimals: up to 0.005 in `to` units going out
    // (worth that times the factor ratio back in `from` units) plus 0.005 in
    // `from` units coming back.
    let tolerance = 0.005 * to.kilograms_per_unit() / from.kilograms_per_unit() + 0.005 + 1e-9;
    let MassValue::Exact(m) = back else {
        panic!("exact in, exact out");
    };
    assert!(
        (m.value - v).abs() <= tolerance,
        "{v} {from:?} -> {to:?} -> {} (tolerance {tolerance})",
        m.value
    );
}

#[hegel::test]
fn mass_value_converted_to_stays_valid(tc: TestCase) {
    let cfg = MassConfig {
        default_unit: MassUnit::Kilogram,
    };
    let from = mass_unit(&tc);
    let to = mass_unit(&tc);
    let a = mass_magnitude(&tc);
    let b = mass_magnitude(&tc);
    let value = if tc.draw(gs::booleans()) {
        MassValue::Exact(MassMeasurement {
            unit: from,
            value: a,
        })
    } else {
        MassValue::Range {
            unit: from,
            min: a.min(b),
            max: a.max(b),
        }
    };
    cfg.validate_value(&value).unwrap();
    // Conversion of a valid value stays valid: finite, 2-decimal magnitudes,
    // range endpoints still ordered.
    cfg.validate_value(&value.converted_to(to)).unwrap();
}

#[hegel::test]
fn entry_round_trip(tc: TestCase) {
    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_test_writer())
        .try_init();
    let mut h = HegelHarness::new(&tc);
    let entry: Entry = h.arbitrary();
    let row = EntryRow::from_entry(&entry);
    let got = row.to_entry().unwrap();
    tracing::info!(?entry, ?got);
    assert_eq!(entry, got);
}
