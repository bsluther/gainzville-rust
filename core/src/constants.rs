use uuid::uuid;

/// The single user seeded into a fresh database. A stand-in for the authenticated
/// user until an auth module lands: the app treats this as "the only user" and
/// owns std-lib content, default activities, etc. under this id.
pub const DEFAULT_USER_ID: uuid::Uuid = uuid!("eee9e6ae-6531-4580-8356-427604a0dc02");

/// Deprecated alias retained during the system-actor → default-user migration.
/// New code should use [`DEFAULT_USER_ID`]. Remove once all references migrate.
pub const SYSTEM_ACTOR_ID: uuid::Uuid = DEFAULT_USER_ID;
