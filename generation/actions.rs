use uuid::Uuid;

use gv_core::{actions::CreateActivity, models::activity::Activity};
use crate::ArbitraryFrom;

impl ArbitraryFrom<Vec<Uuid>> for CreateActivity {
    fn arbitrary_from<R: rand::Rng, C: super::GenerationContext>(
        rng: &mut R,
        context: &C,
        actor_ids: Vec<Uuid>,
    ) -> Self {
        Activity::arbitrary_from(rng, context, &actor_ids).into()
    }
}
