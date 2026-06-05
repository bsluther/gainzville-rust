use chrono::{DateTime, Utc};
use fractional_index::FractionalIndex;
use rand::RngExt;
use rand::seq::IteratorRandom;
use rand_distr::{Distribution, Normal};
use uuid::Uuid;

use crate::{Arbitrary, ArbitraryFrom, GenerationContext, arbitrary_actor_id, gen_random_name};
use gv_core::{
    forest::Forest,
    models::entry::{Entry, Position, Temporal},
};

impl Arbitrary for Entry {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let model = context.model();
        let choose_anonymous = rng.random_bool(0.1);
        let activity_choice = if choose_anonymous {
            None
        } else {
            model.activities().choose(rng)
        };

        let owner_id = activity_choice
            // If we chose an activity, the entry is owned by the activity's owner.
            .map(|a| a.owner_id)
            .unwrap_or_else(|| arbitrary_actor_id(rng, context));

        let name = if activity_choice.is_none() {
            Some(gen_random_name(rng, " "))
        } else {
            if rng.random_bool(0.5) {
                Some(gen_random_name(rng, " "))
            } else {
                None
            }
        };

        let is_sequence = rng.random_bool(0.5);
        let is_complete = if is_sequence {
            false
        } else {
            rng.random_bool(0.5)
        };

        Entry {
            owner_id: owner_id,
            activity_id: activity_choice.map(|a| a.id),
            id: Uuid::arbitrary(rng, context),
            name,
            display_as_sets: rng.random_bool(0.5),
            is_sequence,
            is_complete,
            is_template: false,
            position: Option::<Position>::arbitrary(rng, context),
            temporal: Temporal::arbitrary(rng, context),
        }
    }
}

impl Arbitrary for FractionalIndex {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        // Found the terminator in the fractional_index internals, seems to work.
        const TERMINATOR: u8 = 0b1000_0000;
        let n_bytes = rng.random_range(1..128);
        let mut bytes = vec![0u8; n_bytes];
        let last = n_bytes - 1;
        rng.fill(&mut bytes[..last]);
        bytes[last] = TERMINATOR;
        FractionalIndex::from_bytes(bytes.to_vec())
            .expect("bytes should be a valid fractional index")
    }
}

impl ArbitraryFrom<&[FractionalIndex]> for FractionalIndex {
    /// Given n fractional indices there are n+1 possible insertion positions: before the first
    /// element, between adjacent elements, and after the last element; generate one at random.
    fn arbitrary_from<R: RngExt, C: GenerationContext>(
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

impl Arbitrary for Option<Position> {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let model = context.model();
        if rng.random_bool(0.5) {
            // Choose a root position half the time.
            return None;
        }
        // TODO: consider using p_valid to choose invalid, non-sequence parents.
        let parent = model.entries().filter(|e| e.is_sequence).choose(rng)?;
        // Choose a child position.
        let entries: Vec<Entry> = model.entries().cloned().collect();
        let forest = Forest::from(entries);
        let sibling_findices: Vec<FractionalIndex> = forest
            .children(parent.id)
            .iter()
            .filter_map(|e| e.frac_index().cloned())
            .collect();
        Some(Position {
            parent_id: parent.id,
            frac_index: FractionalIndex::arbitrary_from(rng, context, &sibling_findices),
        })
    }
}

/// Generate a random duration in milliseconds by sampling from a random distribution with mean
/// 20 minutes and standard deviation 40 mins and setting all negatives values to 0.
pub fn gen_random_exercise_duration_ms<R: RngExt>(rng: &mut R) -> u32 {
    let distribution = Normal::new(20. * 60_000., 40. * 60_000.).unwrap();
    (distribution.sample(rng) as f32).max(0.) as u32
}

// TODO: this doesn't enforce that start <= end. Should impl ArbitraryFrom<Range>
impl Arbitrary for Temporal {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
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
        let context = SimulationContext::default();

        for _ in 0..10_000 {
            Temporal::arbitrary(&mut rng, &context);
        }
    }

    #[test]
    /// Test that generation of fractional indices doesn't panic.
    fn test_arbitrary_fractional_index_does_not_panic() {
        let mut rng = rand::rng();
        let context = SimulationContext::default();

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
        let context = SimulationContext::default();

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
}
