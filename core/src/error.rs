// Idea: consistency errors are violations of properties. Things like
// "sequence entries cannot be marked complete"
// "root entry must have defined start or end time"
// "template entries cannot have a start or end time (duration only)"
// "activity '0b7ed4a1-29c4-418c-94d5-9a5c73c633bd' has no template root"
// - All of thse are error msgs from mutators, good starting point for assembling Properties.
// Make a Property enum! Put it in the Consistency variant.
// That may also help with differentiating errors where the system correctly caught a violation vs.
// unexpected bad behavior.
// It also should help line things up to assert those properties in PBT, DST, etc.

#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    #[error("Database error: {0}")]
    Database(Box<dyn std::error::Error + Send + Sync>),
    #[error("Email already exists")]
    EmailAlreadyExists,
    #[error("Other: {0}")]
    Other(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),
    #[error("Consistency error: {0}")]
    Consistency(String),
    #[error("Attribute mismatch")]
    AttributeMismatch,
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
