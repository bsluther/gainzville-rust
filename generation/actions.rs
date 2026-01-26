use rand::Rng;
use uuid::Uuid;

use crate::{Arbitrary, ArbitraryFrom, GenerationContext, pick};
use gv_core::{
    actions::{Action, CreateActivity, CreateEntry, CreateUser, MoveEntry},
    models::{
        activity::Activity,
        entry::{Entry, Position, Temporal},
        user::User,
    },
    validation::{Email, Username},
};

impl ArbitraryFrom<(&[Uuid], &[Activity], &[Entry])> for Action {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        (actor_ids, activities, entries): (&[Uuid], &[Activity], &[Entry]),
    ) -> Self {
        // Do not choose MoveEntry if entries is empty.
        let n = if entries.is_empty() { 3 } else { 4 };
        match rng.random_range(0..n) {
            0 => CreateUser::arbitrary(rng, context).into(),
            1 => CreateActivity::arbitrary_from(rng, context, actor_ids).into(),
            2 => CreateEntry::arbitrary_from(rng, context, (actor_ids, activities, entries)).into(),
            3 => MoveEntry::arbitrary_from(rng, context, entries).into(),
            _ => unreachable!(),
        }
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

impl ArbitraryFrom<&[Uuid]> for CreateActivity {
    /// Generate an arbitrary activity owned by one of the provided uuids.
    fn arbitrary_from<R: rand::Rng, C: super::GenerationContext>(
        rng: &mut R,
        context: &C,
        actor_ids: &[Uuid],
    ) -> Self {
        Activity::arbitrary_from(rng, context, actor_ids).into()
    }
}

impl ArbitraryFrom<(&[Uuid], &[Activity], &[Entry])> for CreateEntry {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        (actor_ids, activities, entries): (&[Uuid], &[Activity], &[Entry]),
    ) -> Self {
        Entry::arbitrary_from(rng, context, (actor_ids, activities, entries)).into()
    }
}

/// Provided entries must be non-empty.
impl ArbitraryFrom<&[Entry]> for MoveEntry {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        entries: &[Entry],
    ) -> Self {
        // Choose an entry to move.
        // Perhaps I should return an option so it's safe to call this with no entries.
        // - Another idea is to have a No-Op action.
        let entry = pick(entries, rng).expect("entres must be non-empty");

        MoveEntry {
            // For now, choose the owner of the moving entry as the actor.
            actor_id: entry.owner_id,
            entry_id: entry.id,
            // Some of these position/temporal combinations will be invalid, the mutator should catch
            // it.
            position: Option::<Position>::arbitrary_from(rng, context, entries),
            temporal: Temporal::arbitrary(rng, context),
        }
    }
}
