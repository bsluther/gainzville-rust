use rand::RngExt;
use uuid::Uuid;

use crate::{Arbitrary, arbitrary_actor_id, gen_random_text};
use gv_core::models::activity::{Activity, ActivityName};

impl Arbitrary for ActivityName {
    fn arbitrary<R: RngExt, C: super::GenerationContext>(rng: &mut R, _context: &C) -> Self {
        // gen_random_text bounds output by word count, not length, so clamp to the
        // cap rather than risk overshooting it.
        let text: String = gen_random_text(rng, 1..5)
            .chars()
            .take(ActivityName::MAX_LEN)
            .collect();
        ActivityName::parse(text).expect("clamped activity name should be valid")
    }
}

impl Arbitrary for Activity {
    fn arbitrary<R: RngExt, C: super::GenerationContext>(rng: &mut R, context: &C) -> Self {
        let desc = if rng.random_bool(0.8) {
            Some(gen_random_text(rng, 0..100))
        } else {
            None
        };
        let owner_id = arbitrary_actor_id(rng, context);

        Activity {
            id: Uuid::arbitrary(rng, context),
            owner_id: owner_id,
            source_activity_id: None,
            name: ActivityName::arbitrary(rng, context),
            description: desc,
        }
    }
}
