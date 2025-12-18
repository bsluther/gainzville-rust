#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Email already exists")]
    EmailAlreadyExists,
    #[error("Invalid email: {0}")]
    InvalidEmail(String),
    #[error("Invalid username: {0}")]
    InvalidUsername(String),
    #[error("Other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, DomainError>;
