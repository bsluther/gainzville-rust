use crate::core::error::DomainError;
use sqlx::{Decode, Postgres, Sqlite, Type};

// NOTE: Type and Decode implementaions are all AI generated.

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Email(String);
impl Email {
    pub fn parse(email: String) -> Result<Self, DomainError> {
        // Basic validation (use a crate like `validator` or `email_address` in real code)
        if !email.contains('@') {
            return Err(DomainError::InvalidEmail("missing @".into()));
        }

        if email.len() > 255 {
            return Err(DomainError::InvalidEmail("too long".into()));
        }

        if email.trim() != email {
            return Err(DomainError::InvalidEmail("whitespace".into()));
        }

        Ok(Self(email.to_lowercase())) // Normalize
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Type<Postgres> for Email {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as Type<Postgres>>::type_info()
    }
}

impl<'r> Decode<'r, Postgres> for Email {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s: String = Decode::<Postgres>::decode(value)?;
        Email::parse(s).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

impl Type<Sqlite> for Email {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, Sqlite> for Email {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s: String = Decode::<Sqlite>::decode(value)?;
        Email::parse(s).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Username(String);

impl Username {
    pub fn parse(username: String) -> Result<Self, DomainError> {
        if username.is_empty() {
            return Err(DomainError::InvalidUsername("empty".into()));
        }

        if username.len() > 50 {
            return Err(DomainError::InvalidUsername("too long".into()));
        }

        // Check for valid characters (alphanumeric, underscore, dash)
        if !username
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(DomainError::InvalidUsername("invalid characters".into()));
        }

        Ok(Self(username))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Type<Postgres> for Username {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as Type<Postgres>>::type_info()
    }
}

impl<'r> Decode<'r, Postgres> for Username {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let s: String = Decode::<Postgres>::decode(value)?;
        Username::parse(s).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

impl Type<Sqlite> for Username {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, Sqlite> for Username {
    fn decode(
        value: sqlx::sqlite::SqliteValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let s: String = Decode::<Sqlite>::decode(value)?;
        Username::parse(s).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}
