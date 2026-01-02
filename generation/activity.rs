use uuid::Uuid;

use gv_core::models::activity::{Activity, ActivityName};
use crate::{Arbitrary, ArbitraryFrom, gen_random_text, pick};

impl Arbitrary for ActivityName {
    fn arbitrary<R: rand::Rng, C: super::GenerationContext>(rng: &mut R, context: &C) -> Self {
        ActivityName::parse(gen_random_text(rng, 1..5).to_string())
            .expect("failed to parse randomly generated text")
    }
}

impl ArbitraryFrom<&Vec<Uuid>> for Activity {
    fn arbitrary_from<R: rand::Rng, C: super::GenerationContext>(
        rng: &mut R,
        context: &C,
        actor_ids: &Vec<Uuid>,
    ) -> Self {
        let desc = if rng.random_bool(0.8) {
            Some(gen_random_text(rng, 0..100))
        } else {
            None
        };
        Activity {
            id: Uuid::arbitrary(rng, context),
            owner_id: pick(&actor_ids, rng)
                .expect("owner_ids must not be empty")
                .clone(),
            source_activity_id: None,
            name: ActivityName::arbitrary(rng, context),
            description: desc,
        }
    }
}
