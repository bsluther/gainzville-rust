use generation::{Arbitrary, SimulationContext};
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
