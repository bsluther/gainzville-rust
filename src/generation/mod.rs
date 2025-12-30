use fractional_index::FractionalIndex;
use rand::Rng;
use uuid::Uuid;

use crate::core::{
    actions::CreateEntry,
    models::{
        activity::Activity,
        entry::{Entry, Position},
        user::User,
    },
};

pub trait GenerationContext {}

pub trait Arbitrary {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, context: &C) -> Self;
}

pub trait ArbitraryFrom<T> {
    fn arbitrary_from<R: Rng, C: GenerationContext>(rng: &mut R, context: &C, t: T) -> Self;
}

// Unifiromly pick an element from a slice. Panics when the slice is empty.
pub fn pick<'a, T, R: Rng>(choices: &'a [T], rng: &mut R) -> &'a T {
    &choices[rng.random_range(0..choices.len())]
}

pub fn pick_index<R: Rng>(choices: usize, rng: &mut R) -> usize {
    rng.random_range(0..choices)
}

impl Arbitrary for Uuid {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        uuid::Builder::from_random_bytes(rng.random()).into_uuid()
    }
}

// TODO: could use a trait, i.e. ForestNode, which keeps only the forest structure. Then Entry can
// implement ForestNode, but so can a collection of Positions.
struct Forest {}
impl Forest {
    pub fn siblings<'a>(entry: &Entry, entries: &'a [Entry]) -> Vec<&'a Entry> {
        if entry.parent_id.is_none() {
            return Vec::new();
        }
        entries
            .iter()
            .filter(|e| e.parent_id == entry.parent_id)
            .collect::<Vec<_>>()
    }
    pub fn children_of<'a>(entry: &Entry, entries: &'a [Entry]) -> Vec<&'a Entry> {
        entries
            .iter()
            .filter(|e| e.parent_id == Some(entry.id))
            .collect::<Vec<_>>()
    }
}

impl ArbitraryFrom<Vec<Entry>> for Position {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        entries: Vec<Entry>,
    ) -> Self {
        let parent_choice = pick(&entries, rng);
        let mut siblings = Forest::children_of(parent_choice, &entries);
        siblings.sort_by(|a, b| a.frac_index.cmp(&b.frac_index));
        // No siblings
        if siblings.is_empty() {
            return Position {
                parent_id: parent_choice.id,
                frac_index: FractionalIndex::default(),
            };
        }

        // There are n+1 possible positions, the first position has no predecessor.
        if rng.random_bool((1 / siblings.len() + 1) as f64) {
            // Place before the first sibling.
            assert!(
                siblings.first().unwrap().frac_index.is_some(),
                "child entry must have a frac_index"
            );
            return Position {
                parent_id: parent_choice.id,
                frac_index: FractionalIndex::new_before(
                    &siblings
                        .first()
                        .expect("siblings to be non-empty")
                        .frac_index
                        .clone()
                        .unwrap(),
                ),
            };
        } else {
            // Choose a predecessor and insert between pred and pred's successor
            let predecessor_ix = pick_index(siblings.len(), rng);
            let predecessor = siblings
                .get(predecessor_ix)
                .and_then(|e| e.frac_index.as_ref());
            let successor = siblings
                .get(predecessor_ix + 1)
                .and_then(|e| e.frac_index.as_ref());
            return Position {
                parent_id: parent_choice.id,
                frac_index: FractionalIndex::new(predecessor, successor)
                    .expect("frac_index should be valid"),
            };
        }
    }
}

impl ArbitraryFrom<(Vec<Activity>, Vec<Entry>)> for CreateEntry {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        (activities, entries): (Vec<Activity>, Vec<Entry>),
    ) -> Self {
        let choice = pick(&activities, rng);

        let position = match rng.random_bool(0.5) {
            true => None,
            false => Some(Position::arbitrary_from(rng, context, entries)),
        };

        let (parent_id, frac_index) = position.map(|p| (p.parent_id, p.frac_index)).unzip();

        // YOU ARE HERE: change this into an AbitraryFrom for Entry, then compose CreateEntry from
        // it.
        CreateEntry {
            actor_id: choice.owner_id,
            entry: Entry {
                owner_id: choice.owner_id,
                activity_id: Some(choice.id),
                id: Uuid::arbitrary(rng, context),
                display_as_sets: rng.random_bool(0.5),
                is_sequence: rng.random_bool(0.5),
                is_template: false,
                parent_id,
                frac_index,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arbitary_entry() {
        let mut rng = rand::rng();
        let choices = vec![1, 2, 3];
        let picked = pick(&choices, &mut rng);
        assert!(choices.contains(picked));
    }
}
