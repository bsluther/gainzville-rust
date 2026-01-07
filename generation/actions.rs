use uuid::Uuid;

use crate::ArbitraryFrom;
use gv_core::{actions::CreateActivity, models::activity::Activity};

impl ArbitraryFrom<Vec<Uuid>> for CreateActivity {
    /// Generate an arbitrary activity owned by one of the provided uuids.
    fn arbitrary_from<R: rand::Rng, C: super::GenerationContext>(
        rng: &mut R,
        context: &C,
        actor_ids: Vec<Uuid>,
    ) -> Self {
        Activity::arbitrary_from(rng, context, &actor_ids).into()
    }
}
