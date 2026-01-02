use fractional_index::FractionalIndex;
use rand::Rng;
use uuid::Uuid;

use gv_core::{
    actions::CreateEntry,
    models::{
        activity::Activity,
        entry::{Entry, Position},
    },
};
use crate::{Arbitrary, ArbitraryFrom, GenerationContext, pick, pick_index};

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

impl ArbitraryFrom<&Vec<Entry>> for Option<Position> {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        entries: &Vec<Entry>,
    ) -> Self {
        let parent_choice = pick(&entries, rng)?;
        let mut siblings = Forest::children_of(parent_choice, &entries);
        // Sort by fractional index.
        siblings.sort_by(|a, b| a.frac_index.cmp(&b.frac_index));

        if siblings.is_empty() {
            return Some(Position {
                parent_id: parent_choice.id,
                frac_index: FractionalIndex::default(),
            });
        }

        // There are n+1 possible positions: the first position (no predecessor) and the position
        // after each sibling.
        if rng.random_bool((1 / (siblings.len() + 1)) as f64) {
            // Probability 1/(n+1): the first position, between None and siblings[0].
            assert!(
                siblings.first().unwrap().frac_index.is_some(),
                "child entry must have a frac_index"
            );
            return Some(Position {
                parent_id: parent_choice.id,
                frac_index: FractionalIndex::new_before(
                    &siblings
                        .first()
                        .expect("siblings to be non-empty")
                        .frac_index
                        .clone()
                        .unwrap(),
                ),
            });
        } else {
            // Choose a predecessor and insert between pred and pred's successor
            let predecessor_ix = pick_index(siblings.len(), rng);
            let predecessor = siblings
                .get(predecessor_ix)
                .and_then(|e| e.frac_index.as_ref());
            let successor = siblings
                .get(predecessor_ix + 1)
                .and_then(|e| e.frac_index.as_ref());
            return Some(Position {
                parent_id: parent_choice.id,
                frac_index: FractionalIndex::new(predecessor, successor)
                    .expect("frac_index should be valid"),
            });
        }
    }
}

impl ArbitraryFrom<(&Vec<Activity>, &Vec<Entry>)> for Entry {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        (activities, entries): (&Vec<Activity>, &Vec<Entry>),
    ) -> Self {
        // This unwrap is not great, but for now I need to get the owner_id from somewhere.
        let activity_choice = pick(&activities, rng).expect("activities must not be empty");
        let position = Option::<Position>::arbitrary_from(rng, context, entries);
        let position = match rng.random_bool(0.5) {
            true => None,
            false => position,
        };

        let (parent_id, frac_index) = position.map(|p| (p.parent_id, p.frac_index)).unzip();

        Entry {
            owner_id: activity_choice.owner_id,
            activity_id: Some(activity_choice.id),
            id: Uuid::arbitrary(rng, context),
            display_as_sets: rng.random_bool(0.5),
            is_sequence: rng.random_bool(0.5),
            is_template: false,
            parent_id,
            frac_index,
        }
    }
}

impl ArbitraryFrom<(Vec<Activity>, Vec<Entry>)> for CreateEntry {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        t: (Vec<Activity>, Vec<Entry>),
    ) -> Self {
        unimplemented!()
    }
}
