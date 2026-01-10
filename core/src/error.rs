#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
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
}

#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    #[error("Invalid email: {0}")]
    InvalidEmail(String),
    #[error("Invalid username: {0}")]
    InvalidUsername(String),
    #[error("Invalid activity name: {0}")]
    InvalidActivityName(String),
    #[error("Other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, DomainError>;
