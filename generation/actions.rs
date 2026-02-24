use rand::Rng;
use uuid::Uuid;

use crate::{Arbitrary, ArbitraryFrom, GenerationContext, pick};
use gv_core::{
    actions::{Action, CreateActivity, CreateAttribute, CreateEntry, CreateUser, CreateValue, MoveEntry},
    models::{
        activity::Activity,
        attribute::{Attribute, Value},
        entry::{Entry, Position, Temporal},
        user::User,
    },
    validation::{Email, Username},
};

impl ArbitraryFrom<(&[Uuid], &[Activity], &[Entry], &[Attribute])> for Action {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        (actor_ids, activities, entries, attributes): (&[Uuid], &[Activity], &[Entry], &[Attribute]),
    ) -> Self {
        // Actions that are always available: CreateUser, CreateActivity, CreateEntry, CreateAttribute
        // Actions that require non-empty entries: MoveEntry
        // Actions that require non-empty entries and attributes: CreateValue
        let mut choices: Vec<u8> = vec![0, 1, 2, 3];
        if !entries.is_empty() {
            choices.push(4);
            if !attributes.is_empty() {
                choices.push(5);
            }
        }
        let choice = pick(&choices, rng).unwrap();
        match choice {
            0 => CreateUser::arbitrary(rng, context).into(),
            1 => CreateActivity::arbitrary_from(rng, context, actor_ids).into(),
            2 => CreateEntry::arbitrary_from(rng, context, (actor_ids, activities, entries)).into(),
            3 => CreateAttribute::arbitrary_from(rng, context, actor_ids).into(),
            4 => MoveEntry::arbitrary_from(rng, context, entries).into(),
            5 => CreateValue::arbitrary_from(rng, context, (entries, attributes)).into(),
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

impl ArbitraryFrom<&[Uuid]> for CreateAttribute {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        actor_ids: &[Uuid],
    ) -> Self {
        Attribute::arbitrary_from(rng, context, actor_ids).into()
    }
}

impl ArbitraryFrom<(&[Entry], &[Attribute])> for CreateValue {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        (entries, attributes): (&[Entry], &[Attribute]),
    ) -> Self {
        let entry = pick(entries, rng).expect("entries must not be empty");
        let value = Value::arbitrary_from(rng, context, (entries, attributes));
        CreateValue {
            actor_id: entry.owner_id,
            value,
        }
    }
}
