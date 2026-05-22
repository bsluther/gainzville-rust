use crate::error::{DomainError, ValidationError};

// NOTE: Mostly AI generated.

/// A very naive type representing an email. Not production ready, but good enough for now.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Email(String);
impl Email {
    pub fn parse(email: String) -> Result<Self, DomainError> {
        let email = email.trim().to_lowercase();

        // Split on @
        let parts: Vec<&str> = email.split('@').collect();
        if parts.len() != 2 {
            return Err(ValidationError::InvalidEmail("must have exactly one @".into()).into());
        }

        let local = parts[0];
        let domain = parts[1];

        // Local part: alphanumeric, dots, underscores
        if local.is_empty() || local.len() > 64 {
            return Err(ValidationError::InvalidEmail("local part length invalid".into()).into());
        }
        if !local
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '-')
        {
            return Err(ValidationError::InvalidEmail(
                format!("invalid characters in local part '{local}'").into(),
            )
            .into());
        }

        // Domain: alphanumeric with dots and dashes, must have a dot
        if domain.is_empty() || domain.len() > 255 {
            return Err(ValidationError::InvalidEmail("domain length invalid".into()).into());
        }
        if !domain.contains('.') {
            return Err(ValidationError::InvalidEmail("domain must have a dot".into()).into());
        }
        if !domain
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
        {
            return Err(
                ValidationError::InvalidEmail("invalid characters in domain".into()).into(),
            );
        }

        Ok(Self(email))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Username(String);

impl Username {
    pub fn parse(username: String) -> Result<Self, DomainError> {
        if username.is_empty() {
            return Err(ValidationError::InvalidUsername("empty".to_string()).into());
        }

        if username.len() > 50 {
            return Err(ValidationError::InvalidUsername("too long".to_string()).into());
        }

        // Check for valid characters (alphanumeric, underscore, dash)
        if !username
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(ValidationError::InvalidUsername("invalid characters".to_string()).into());
        }

        Ok(Self(username))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
