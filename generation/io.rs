use chrono::{DateTime, Utc};
use gv_core::io::{Clock, Io};
use rand::{RngExt, SeedableRng, rngs::ChaCha8Rng};
use std::sync::Mutex;

#[derive(Debug)]
pub struct SimIo {
    clock: SimClock,
    rng: Mutex<ChaCha8Rng>,
}

impl SimIo {
    pub fn new(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        SimIo {
            clock: SimClock::new(rng.random()),
            rng: Mutex::new(rng),
        }
    }
}

impl Clock for SimIo {
    fn current_time_wall_clock(&self) -> DateTime<Utc> {
        self.clock.current_time_wall_clock()
    }
}

impl Io for SimIo {
    fn uuid(&self) -> uuid::Uuid {
        uuid::Builder::from_random_bytes(self.rng.lock().unwrap().random()).into_uuid()
    }
}

#[derive(Debug)]
pub struct SimClock {
    curr_time: Mutex<DateTime<Utc>>,
    rng: Mutex<ChaCha8Rng>,
    min_tick_micros: u64,
    max_tick_micros: u64,
}

impl SimClock {
    pub fn new(seed: u64) -> Self {
        SimClock {
            curr_time: Mutex::new(
                DateTime::<Utc>::from_timestamp(0, 0).expect("unix epoch is a valid timestamp"),
            ),
            rng: Mutex::new(ChaCha8Rng::seed_from_u64(seed)),
            min_tick_micros: 1,
            max_tick_micros: 1_000,
        }
    }

    pub fn advance(&self, by: chrono::Duration) {
        *self.curr_time.lock().unwrap() += by;
    }
}

impl Clock for SimClock {
    fn current_time_wall_clock(&self) -> DateTime<Utc> {
        let mut time = self.curr_time.lock().unwrap();
        let micros = self
            .rng
            .lock()
            .unwrap()
            .random_range(self.min_tick_micros..self.max_tick_micros);
        *time += std::time::Duration::from_micros(micros);
        *time
    }
}
