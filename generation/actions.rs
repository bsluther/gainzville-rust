use uuid::Uuid;

use crate::{Arbitrary, ArbitraryFrom};
use gv_core::{
    actions::{CreateActivity, CreateUser},
    models::{activity::Activity, user::User},
    validation::{Email, Username},
};

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

impl Arbitrary for CreateUser {
    fn arbitrary<R: rand::Rng, C: crate::GenerationContext>(rng: &mut R, context: &C) -> Self {
        User {
            actor_id: Uuid::arbitrary(rng, context),
            email: Email::arbitrary(rng, context),
            username: Username::arbitrary(rng, context),
        }
        .into()
    }
}
