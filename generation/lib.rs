use std::num::NonZeroUsize;

use fractional_index::FractionalIndex;
use rand::{Rng, rand_core};
use uuid::Uuid;

use gv_core::core::{
    actions::CreateEntry,
    models::{
        activity::Activity,
        entry::{Entry, Position},
        user::User,
    },
};

pub mod actions;
pub mod activity;
pub mod entry;

pub trait GenerationContext {}

pub struct SimulationContext {}
impl GenerationContext for SimulationContext {}

pub trait Arbitrary {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, context: &C) -> Self;
}

pub trait ArbitraryFrom<T> {
    fn arbitrary_from<R: Rng, C: GenerationContext>(rng: &mut R, context: &C, t: T) -> Self;
}

pub trait ArbitraryFromMaybe<T> {
    fn arbitrary_from_maybe<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        t: T,
    ) -> Option<Self>
    where
        Self: Sized;
}

// Unifiormly pick an element from a slice. Panics when the slice is empty.
pub fn pick<'a, T, R: Rng>(choices: &'a [T], rng: &mut R) -> Option<&'a T> {
    if choices.is_empty() {
        None
    } else {
        Some(&choices[rng.random_range(0..choices.len())])
    }
}

pub fn pick_index<R: Rng>(choices: usize, rng: &mut R) -> usize {
    rng.random_range(0..choices)
}

pub fn gen_random_name<R: Rng>(rng: &mut R) -> String {
    anarchist_readable_name_generator_lib::readable_name_custom(" ", rng)
}

pub fn gen_random_text<R: Rng + rand::RngCore + rand_core::RngCore>(
    rng: &mut R,
    range: std::ops::Range<usize>,
) -> String {
    let n = rng.random_range(range);
    let mut text = String::new();
    for i in 0..(n / 2) {
        let s = anarchist_readable_name_generator_lib::readable_name_custom(" ", &mut *rng);
        if i > 0 {
            text.push(' ');
        }
        text.push_str(&s);
    }
    if n % 2 != 0 {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(
            anarchist_readable_name_generator_lib::readable_name_custom(" ", &mut *rng)
                .split(' ')
                .next()
                .unwrap_or(""),
        );
    }
    text
}

impl Arbitrary for Uuid {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        uuid::Builder::from_random_bytes(rng.random()).into_uuid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arbitary_entry() {
        let mut rng = rand::rng();
        let choices = vec![1, 2, 3];
        let picked = pick(&choices, &mut rng).unwrap();
        assert!(choices.contains(picked));
    }

    #[test]
    fn test_arbitary_text() {
        let mut rng = rand::rng();
        for _ in 0..1000 {
            let text = gen_random_text(&mut rng, 0..100);
            assert!(
                text.split(" ").count() < 100,
                "Length of text should be in range 0..100, found length = {}",
                text.len()
            );
        }
    }
}
