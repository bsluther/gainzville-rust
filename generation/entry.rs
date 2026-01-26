use chrono::{DateTime, Utc};
use fractional_index::FractionalIndex;
use rand::Rng;
use rand_distr::{Distribution, Normal};
use uuid::Uuid;

use crate::{Arbitrary, ArbitraryFrom, GenerationContext, pick};
use gv_core::{
    SYSTEM_ACTOR_ID,
    models::{
        activity::Activity,
        entry::{Entry, Position, Temporal},
    },
};

// TODO: could use a trait, i.e. ForestNode, which keeps only the forest structure. Then Entry can
// implement ForestNode, but so can a collection of Positions.
struct Forest {}
impl Forest {
    #[allow(unused)]
    pub fn siblings<'a>(entry: &Entry, entries: &'a [Entry]) -> Vec<&'a Entry> {
        // CONSIDER:
        // Currently this returns an empty array if the entry is at the root. In some sense this is
        // correct, because root entries don't have frac_indices. In another sense, every root entry
        // is a sibling.
        // What if everythiing had a frac_index? I.e. root is no lnger time ordered. Then, the user
        // can just press "move to time" to put it in that order. But wait, that order doesn't exist.
        // Probably the way i have it is good.
        let Some(parent_id) = entry.parent_id() else {
            return Vec::new();
        };
        entries
            .iter()
            .filter(|e| e.parent_id().is_some_and(|id| id == parent_id))
            .collect::<Vec<_>>()
    }
    pub fn children_of<'a>(entry: &Entry, entries: &'a [Entry]) -> Vec<&'a Entry> {
        entries
            .iter()
            .filter(|e| e.parent_id().is_some_and(|id| id == entry.id))
            .collect::<Vec<_>>()
    }
}

impl Arbitrary for FractionalIndex {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        // Found the terminator in the fractional_index internals, seems to work.
        const TERMINATOR: u8 = 0b1000_0000;
        let n_bytes = rng.random_range(1..128);
        let mut bytes = Vec::<u8>::new();
        for _ in 0..n_bytes - 1 {
            bytes.push(rng.random())
        }
        bytes.push(TERMINATOR);

        FractionalIndex::from_bytes(bytes.to_vec())
            .expect("bytes should be a valid fractional index")
    }
}

// TODO: should generate all Options variants probabilistically.
impl Arbitrary for Entry {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        Entry {
            owner_id: Uuid::arbitrary(rng, context),
            id: Uuid::arbitrary(rng, context),
            activity_id: Some(Uuid::arbitrary(rng, context)),
            display_as_sets: rng.random_bool(0.5),
            is_sequence: rng.random_bool(0.5),
            is_template: rng.random_bool(0.5),
            position: Some(Position {
                parent_id: Uuid::arbitrary(rng, context),
                frac_index: FractionalIndex::arbitrary(rng, context),
            }),
            // TODO: generate arbitrarily
            temporal: Temporal::None,
        }
    }
}

impl ArbitraryFrom<&[FractionalIndex]> for FractionalIndex {
    /// Given n fractional indices there are n+1 possible insertion positions: before the first
    /// element, between adjacent elements, and after the last element; generate one at random.
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        frac_indices: &[FractionalIndex],
    ) -> Self {
        if frac_indices.is_empty() {
            return FractionalIndex::default();
        }
        let mut frac_indices = frac_indices.to_vec();
        frac_indices.sort();

        let n = frac_indices.len();
        let pos = rng.random_range(0..=n); // pos $\in$ 0..n+1

        if pos == 0 {
            let successor = frac_indices.first().unwrap();
            return FractionalIndex::new_before(successor);
        }
        if pos == n {
            let predecessor = frac_indices.last().unwrap();
            return FractionalIndex::new_after(predecessor);
        }
        let predecessor = frac_indices.get(n - 2);
        let successor = frac_indices.get(n - 1);
        FractionalIndex::new(predecessor, successor).unwrap()
    }
}
impl ArbitraryFrom<&[Entry]> for Option<Position> {
    /// Generate an arbitrary position within the given entries with probability 0.5 of choosing a
    /// root position, otherwise a child position. If the provided entries slice is empty, always
    /// generates a root position (e.g. returns None).
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        entries: &[Entry],
    ) -> Self {
        if rng.random_bool(0.5) {
            // Choose a root position.
            return None;
        }
        // Choose a child position, if possible.
        let parent_choice = pick(entries, rng)?;
        let sibling_findices: Vec<FractionalIndex> = Forest::children_of(parent_choice, entries)
            .iter()
            .filter_map(|e| e.frac_index().cloned())
            .collect();
        Some(Position {
            parent_id: parent_choice.id,
            frac_index: FractionalIndex::arbitrary_from(rng, context, &sibling_findices),
        })
    }
}
// CONSIDER: taking Option<&[...]> to let the caller omit options.
impl ArbitraryFrom<(&[Uuid], &[Activity], &[Entry])> for Entry {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        (actor_ids, activities, entries): (&[Uuid], &[Activity], &[Entry]),
    ) -> Self {
        let choose_anonymous = rng.random_bool(0.1);
        let activity_choice = if choose_anonymous || activities.is_empty() {
            None
        } else {
            Some(pick(&activities, rng).unwrap())
        };

        let owner_id = activity_choice
            // If we chose an actvitity, make the genereted activity owned by the activity's owner.
            // - Once library's are implemented, should relax ths condition and just pick any valid
            // owner (i.e. allow creating an entry of another user's activity).
            .map(|a| a.owner_id)
            .unwrap_or_else(|| {
                // Choose from the provided actor ids.
                pick(actor_ids, rng)
                    // If there are no actors, default to SYSTEM actor.
                    .unwrap_or_else(|| &SYSTEM_ACTOR_ID)
                    .clone()
            });

        Entry {
            owner_id: owner_id,
            activity_id: activity_choice.map(|a| a.id),
            id: Uuid::arbitrary(rng, context),
            display_as_sets: rng.random_bool(0.5),
            is_sequence: rng.random_bool(0.5),
            is_template: false,
            position: Option::<Position>::arbitrary_from(rng, context, entries),
            temporal: Temporal::arbitrary(rng, context),
        }
    }
}

/// Generate a random duration in milliseconds by sampling from a random distribution with mean
/// 20 minutes and standard deviation 40 mins and setting all negatives values to 0.
pub fn gen_random_exercise_duration_ms<R: Rng>(rng: &mut R) -> u32 {
    let distribution = Normal::new(20. * 60_000., 40. * 60_000.).unwrap();
    (distribution.sample(rng) as f32).max(0.) as u32
}

// TODO: this doesn't enforce that start <= end. Should impl ArbitraryFrom<Range>
impl Arbitrary for Temporal {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let t = match rng.random_range(0..=6) {
            0 => Temporal::None,
            1 => Temporal::Start {
                start: DateTime::<Utc>::arbitrary(rng, context),
            },
            2 => Temporal::End {
                end: DateTime::<Utc>::arbitrary(rng, context),
            },
            3 => Temporal::Duration {
                duration: rng.random(),
            },
            4 => Temporal::StartAndEnd {
                start: DateTime::<Utc>::arbitrary(rng, context),
                end: DateTime::<Utc>::arbitrary(rng, context),
            },
            5 => Temporal::StartAndDuration {
                start: DateTime::<Utc>::arbitrary(rng, context),
                duration_ms: rng.random(),
            },
            6 => {
                let d = gen_random_exercise_duration_ms(rng);

                Temporal::DurationAndEnd {
                    duration_ms: d,
                    end: DateTime::<Utc>::arbitrary(rng, context),
                }
            }
            _ => unreachable!(),
        };

        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationContext;

    #[test]
    /// Test that generation of temporals doesn't panic.
    fn test_arbitrary_temporal_does_not_panic() {
        let mut rng = rand::rng();
        let context = SimulationContext {};

        for _ in 0..10_000 {
            Temporal::arbitrary(&mut rng, &context);
        }
    }

    #[test]
    /// Test that generation of fractional indices doesn't panic.
    fn test_arbitrary_fractional_index_does_not_panic() {
        let mut rng = rand::rng();
        let context = SimulationContext {};

        for _ in 0..10_000 {
            FractionalIndex::arbitrary(&mut rng, &context);
        }
    }

    #[test]
    /// This test is perhaps a bit silly, but was a fun exercise. Check that generated findexes
    /// distribute across available postions as expected.
    /// A better approach would be to check each position has the expected number of occurrences.
    fn test_arbitrary_frac_index_from_frac_indices() {
        let mut rng = rand::rng();
        let context = SimulationContext {};

        let mut findices: Vec<FractionalIndex> = Vec::new();
        findices.push(FractionalIndex::default());
        let n_findices = 10;
        for _ in 1..n_findices {
            let prev = findices.last().unwrap();
            findices.push(FractionalIndex::new_after(prev));
        }
        println!(
            "findices: {:?}",
            findices
                .iter()
                .map(|fi| fi.to_string())
                .collect::<Vec<String>>()
        );

        // There n+1 positions. If we generate one, we should expect there to be a 2 / (n+1)
        // probability that the generated findex is at the beginning or end.
        // Generate k indices and check that the observed number of endpoints is within some epsilon
        // of the expected number of endpoints.
        let k = 10_000;
        let mut internal_count = 0;
        let mut end_count = 0;
        for _ in 0..k {
            let new_findex = FractionalIndex::arbitrary_from(&mut rng, &context, &findices);
            let mut local_findices = findices.clone();
            local_findices.push(new_findex.clone());
            local_findices.sort();
            let inserted_ix = local_findices
                .iter()
                .position(|fi| fi == &new_findex)
                .unwrap();

            if inserted_ix == 0 || inserted_ix == (local_findices.len() - 1) {
                end_count += 1;
            } else {
                internal_count += 1;
            }
        }
        assert!(
            internal_count + end_count == k,
            "internal_count + end_count should equal k; if this fails the test is setup incorrectly"
        );
        let epsilon = 0.1;
        let expected_end_count = k as f32 * 2.0 / (n_findices + 1) as f32;
        let error = ((end_count as f32 - expected_end_count) / expected_end_count).abs();
        assert!(
            error < epsilon,
            "observed count should be within the given epsilon of the expected count"
        );
    }

    #[test]
    fn test_arbitary_position_from_entries() {
        // Nor there yet, I think I need more building blocks first.
        let mut rng = rand::rng();
        let context = SimulationContext {};
        let entry = Entry::arbitrary(&mut rng, &context);
        println!("{:?}", entry);
        let entries: Vec<Entry> = (0..100)
            .map(|_| Entry::arbitrary(&mut rng, &context))
            .collect();

        let _position = Option::<Position>::arbitrary_from(&mut rng, &context, entries.as_slice());
    }
}
