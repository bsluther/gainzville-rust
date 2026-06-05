// Note: may want to round timestamp precision here to match the DB precision.

pub trait Io: Clock {}

pub trait Clock {}
