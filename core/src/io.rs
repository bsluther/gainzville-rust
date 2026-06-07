// Note: may want to round timestamp precision here to match the DB precision.

use std::fmt::Debug;

use chrono::{DateTime, Utc};
use uuid::Uuid;

pub trait Io: Debug + Clock + Send + Sync {
    fn uuid(&self) -> Uuid;
}

pub trait Clock {
    fn current_time_wall_clock(&self) -> DateTime<Utc>;
}

#[derive(Default, Debug)]
pub struct SystemIo {}

impl Clock for SystemIo {
    fn current_time_wall_clock(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

impl Io for SystemIo {
    fn uuid(&self) -> Uuid {
        Uuid::new_v4()
    }
}
