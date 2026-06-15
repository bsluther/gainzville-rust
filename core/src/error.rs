// Errors split by *who is at fault*, which is what the simulator/PBT harness
// needs to know:
//
// - `Rejected`           — the action was correctly refused; a precondition on
//                          the current state was not met. Expected. A
//                          well-behaved caller can hit these, and a simulator
//                          continues past them.
// - `InvariantViolation` — we *observed* an already-broken invariant in stored
//                          state. A bug, not a rejected action; a simulator/PBT
//                          harness halts and reports.
// - `Database`           — the backend/machinery failed (sqlx, connection,
//                          decode). Not a domain outcome.

#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    /// Backend / infrastructure failure (sqlx, connection, decode) — not a
    /// domain outcome. The machinery failed, not the action.
    #[error("Database error: {0}")]
    Database(Box<dyn std::error::Error + Send + Sync>),
    /// The action was correctly refused: a precondition on the current state was
    /// not met (auth, uniqueness, missing reference, state guard). Expected; a
    /// well-behaved caller can hit these and a simulator continues past them.
    #[error("{0}")]
    Rejected(RejectReason),
    /// We *observed* an already-broken invariant in stored state — a bug, not a
    /// rejected action; a simulator/PBT harness halts and reports on these.
    /// `invariant` is a stable identity (string-first; the seed for a future
    /// `Property`); `context` carries the offending id(s).
    #[error("invariant violation: {invariant} ({context})")]
    InvariantViolation {
        invariant: &'static str,
        context: String,
    },
}

/// Why an action was rejected. Every variant is an *expected* refusal — the
/// system correctly prevented an illegal transition. Distinct from an observed
/// invariant violation (already-broken stored state), which is a bug, not a
/// rejection.
#[derive(thiserror::Error, Debug)]
pub enum RejectReason {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Email already exists")]
    EmailExists,
    #[error("Attribute mismatch")]
    AttributeMismatch,
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),
    #[error("Not found: {0}")]
    NotFound(String),
    /// A precondition for the attempted transition did not hold in the current
    /// state (e.g. "root entry must have defined start or end time"). Catch-all
    /// for state guards. `&'static str` because every guard message is a stable
    /// literal — it doubles as a grep-able identity and the seed for a future
    /// named reason; specific guards graduate to their own variant when a test
    /// or client reaches for them.
    #[error("{0}")]
    Precondition(&'static str),
}

// `From` is not transitive: `RejectReason: From<ValidationError>` does not give
// `DomainError: From<ValidationError>`. This manual impl keeps every existing
// `ValidationError::…().into()` / `?` site working through the `Rejected` layer.
impl From<ValidationError> for DomainError {
    fn from(e: ValidationError) -> Self {
        DomainError::Rejected(RejectReason::Validation(e))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    #[error("Invalid email: {0}")]
    InvalidEmail(String),
    #[error("Invalid username: {0}")]
    InvalidUsername(String),
    #[error("Invalid activity name: {0}")]
    InvalidActivityName(String),
    #[error("Invalid numeric config: {0}")]
    InvalidNumericConfig(String),
    #[error("Invalid select config: {0}")]
    InvalidSelectConfig(String),
    #[error("Invalid multiselect config: {0}")]
    InvalidMultiselectConfig(String),
    #[error("Invalid value: {0}")]
    InvalidValue(String),
    #[error("Other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, DomainError>;

/// Convert any concrete error into a `DomainError::Database`. Used at the
/// DB boundary to lift backend errors (sqlx, etc.) into the domain error
/// type without core having to know about those backends.
pub trait DbErr<T> {
    fn db_err(self) -> Result<T>;
}

impl<T, E> DbErr<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn db_err(self) -> Result<T> {
        self.map_err(|e| {
            tracing::warn!(error = %e, "db error at boundary");
            DomainError::Database(Box::new(e))
        })
    }
}
