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
        self.map_err(|e| DomainError::Database(Box::new(e)))
    }
}
