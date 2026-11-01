//! `gv-ui`: screen-shaped but platform-independent logic, in the spirit of `matrix-sdk-ui`.
//!
//! Pure functions over domain data (today: the in-memory `Forest`), and the client questions they
//! answer (`queries`: `ChildSlots`, ...). No storage, no FFI: `gv-client` decides how a value is
//! backed and delivered, `gv-ffi` exposes the types, and the Swift app only renders them. The
//! query answers are `async` only to match `QueryExecutor`; they never suspend. `gv-core` stays
//! domain-only, so UI concepts such as drop slots never leak into the domain model.
//!
//! A separate crate (rather than a `ui` module in `gv-client`) so the dependency graph enforces
//! "no storage": this crate cannot reach sqlx, and its tests build and run without it.

pub mod child_slots;
pub mod queries;

pub use child_slots::{ChildSlot, child_slots, sets_members};
pub use queries::*;
