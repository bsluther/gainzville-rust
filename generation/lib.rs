use chrono::{DateTime, Duration, Utc};
use gv_core::validation::{Email, Username};
use rand::{Rng, RngExt};
use rand_distr::{Distribution, Normal};
use uuid::Uuid;

use crate::model::Model;

pub mod actions;
pub mod activity;
pub mod attribute;
pub mod entry;
pub mod model;
pub mod samples;

pub trait GenerationContext {
    fn opts(&self) -> &Opts;
    fn model(&self) -> &Model;
}

/// Options for configuring how values are generated.
#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Opts {
    /// Probability of generating semantically meaningful data. May significantly decrease the space
    /// of possible values.
    pub p_semantic: f64,
    /// Probability of generating valid data.
    pub p_valid: f64,
    /// Expected value of generated timestamps.
    pub time_mean: DateTime<Utc>,
    /// Standard deviation of generated timestamps.
    pub time_std: Duration,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            p_semantic: 0.8,
            p_valid: 0.5,
            time_mean: DateTime::parse_from_rfc3339("2026-01-01T05:00:00Z")
                .unwrap()
                .into(),
            time_std: Duration::days(30),
        }
    }
}

impl Opts {
    pub fn time_now_tight_std() -> Self {
        Opts {
            time_mean: Utc::now(),
            time_std: Duration::hours(12),
            ..Default::default()
        }
    }
}

/// Weights used to decide which actions are generated during simulation.
// pub struct WorkloadProfile {}

pub struct SimulationContext {
    opts: Opts,
    model: Model,
}
impl Default for SimulationContext {
    fn default() -> Self {
        SimulationContext {
            opts: Opts::default(),
            model: Model::empty(),
        }
    }
}

impl SimulationContext {
    pub fn with_opts(opts: Opts) -> Self {
        SimulationContext {
            opts,
            model: Model::empty(),
        }
    }
}

impl GenerationContext for SimulationContext {
    fn opts(&self) -> &Opts {
        &self.opts
    }

    fn model(&self) -> &Model {
        &self.model
    }
}

/// RNG is separate from the GenerationContext so we can borrow the GenerationContext as read-only.
pub trait Arbitrary {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self;
}

pub trait ArbitraryFrom<T> {
    fn arbitrary_from<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C, t: T) -> Self;
}

pub trait ArbitraryFromMaybe<T> {
    fn arbitrary_from_maybe<R: RngExt, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        t: T,
    ) -> Option<Self>
    where
        Self: Sized;
}

/// Generate `Some(f(rng))` with probability `p_some`, otherwise `None`.
pub fn maybe<T, R: RngExt>(rng: &mut R, p_some: f64, f: impl FnOnce(&mut R) -> T) -> Option<T> {
    if rng.random_bool(p_some) {
        Some(f(rng))
    } else {
        None
    }
}

// Unifiormly pick an Some(element) from a slice; returns None if the slice is empty.
pub fn pick<'a, T, R: RngExt>(choices: &'a [T], rng: &mut R) -> Option<&'a T> {
    if choices.is_empty() {
        None
    } else {
        Some(&choices[rng.random_range(0..choices.len())])
    }
}

pub fn pick_index<R: RngExt>(choices: usize, rng: &mut R) -> usize {
    rng.random_range(0..choices)
}

/// Adapts a rand-0.10 RNG to the rand-0.9 `RngCore` that
/// `anarchist-readable-name-generator-lib` (pinned to rand 0.9) expects. Both
/// rand majors coexist in the tree; this newtype is the only bridge between them.
struct Rand09<'a, R: ?Sized>(&'a mut R);

impl<R: Rng + ?Sized> rand_core_0_9::RngCore for Rand09<'_, R> {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }
    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }
    fn fill_bytes(&mut self, dst: &mut [u8]) {
        self.0.fill_bytes(dst)
    }
}

pub fn gen_random_name<R: RngExt>(rng: &mut R, separator: &str) -> String {
    anarchist_readable_name_generator_lib::readable_name_custom(separator, Rand09(rng))
}

pub fn gen_random_text<R: RngExt>(
    rng: &mut R,
    range: std::ops::Range<usize>,
) -> String {
    let n = rng.random_range(range);
    let mut text = String::new();
    for i in 0..(n / 2) {
        let s = anarchist_readable_name_generator_lib::readable_name_custom(" ", Rand09(&mut *rng));
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
            anarchist_readable_name_generator_lib::readable_name_custom(" ", Rand09(&mut *rng))
                .split(' ')
                .next()
                .unwrap_or(""),
        );
    }
    text
}

impl Arbitrary for Email {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        let username = gen_random_name(rng, "-");
        let domain = gen_random_name(rng, "-");
        let tld = ".com";
        Email::parse(username + "@" + &domain + &tld)
            .expect("generated string should be a valid email")
    }
}

impl Arbitrary for Uuid {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        uuid::Builder::from_random_bytes(rng.random()).into_uuid()
    }
}

impl Arbitrary for Username {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, _context: &C) -> Self {
        Username::parse(gen_random_name(rng, "-")).expect("generated username should be valid")
    }
}

impl Arbitrary for DateTime<Utc> {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let mean_ms = context.opts().time_mean.timestamp_millis() as f64;
        let std_ms = context.opts().time_std.num_milliseconds() as f64;
        let normal = Normal::new(mean_ms, std_ms).unwrap();

        let ts_ms = (normal.sample(rng) as i64).clamp(
            DateTime::<Utc>::MIN_UTC.timestamp_millis(),
            DateTime::<Utc>::MAX_UTC.timestamp_millis(),
        );
        let dt = DateTime::from_timestamp_millis(ts_ms).unwrap();
        dt

        // let min = chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
        //     .expect("RFC3339 string must be valid");
        // let max = chrono::DateTime::parse_from_rfc3339("2050-12-12T23:59:59Z")
        //     .expect("RFC3339 string must be valid");

        // let diff = max - min;
        // let diff_seconds = diff.num_seconds();
        // let random_seconds = rng.random_range(0..=diff_seconds);

        // let dt = min + Duration::seconds(random_seconds);
        // dt.to_utc()
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
        let context = SimulationContext::default();
        let mut rng = rand::rng();
        for _ in 0..10_000 {
            DateTime::<Utc>::arbitrary(&mut rng, &context);
        }
    }

    #[test]
    fn test_arbitrary_email_shouldnt_panic() {
        let context = SimulationContext::default();
        let mut rng = rand::rng();
        for _ in 0..10_000 {
            Email::arbitrary(&mut rng, &context);
        }
    }
}
