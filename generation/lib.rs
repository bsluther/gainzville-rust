use chrono::{DateTime, Duration, Utc};
use gv_core::validation::{Email, Username};
use rand::{Rng, rand_core};
use uuid::Uuid;

pub mod actions;
pub mod activity;
pub mod entry;

pub trait GenerationContext {}

pub struct SimulationContext {
    // rng: rand_chacha::ChaCha8Rng,
}
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

pub fn gen_random_name<R: Rng>(rng: &mut R, separator: &str) -> String {
    anarchist_readable_name_generator_lib::readable_name_custom(separator, rng)
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

impl Arbitrary for Email {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        let username = gen_random_name(rng, "-");
        let domain = gen_random_name(rng, "-");
        let tld = ".com";
        Email::parse(username + "@" + &domain + &tld)
            .expect("generated string should be a valid email")
    }
}

impl Arbitrary for Uuid {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        uuid::Builder::from_random_bytes(rng.random()).into_uuid()
    }
}

impl Arbitrary for Username {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        Username::parse(gen_random_name(rng, "-")).expect("generated username should be valid")
    }
}

impl Arbitrary for DateTime<Utc> {
    fn arbitrary<R: Rng, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        let min = chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
            .expect("RFC3339 string must be valid");
        let max = chrono::DateTime::parse_from_rfc3339("2050-12-12T23:59:59Z")
            .expect("RFC3339 string must be valid");

        let diff = max - min;
        let diff_seconds = diff.num_seconds();
        let random_seconds = rng.random_range(0..=diff_seconds);

        let dt = min + Duration::seconds(random_seconds);
        dt.to_utc()
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
                "Number of words should be in range 0..100, found length = {}",
                text.len()
            );
        }
    }

    #[test]
    /// Check DateTime<Utc> generation doesn't fail.
    fn test_arbitrary_datetime_shouldnt_panic() {
        let context = SimulationContext {};
        let mut rng = rand::rng();
        for _ in 0..10_000 {
            DateTime::<Utc>::arbitrary(&mut rng, &context);
        }
    }

    #[test]
    fn test_arbitrary_email_shouldnt_panic() {
        let context = SimulationContext {};
        let mut rng = rand::rng();
        for _ in 0..10_000 {
            Email::arbitrary(&mut rng, &context);
        }
    }
}
