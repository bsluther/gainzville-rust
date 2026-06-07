//! `*Column` wrapper newtypes that own the sqlx encoding rule for each
//! leaf type that crosses the DB boundary.
//!
//! Each wrapper holds a core domain value and implements
//! `sqlx::Type` / `Encode` / `Decode` generically over any `sqlx::Database`
//! that supports the underlying primitive. This keeps `gv_core` free of
//! sqlx-shaped knowledge while giving `gv_sql` a single uniform shape:
//! every `Row` field is a `*Column`, no exceptions.

use chrono::{DateTime, Utc};
use fractional_index::FractionalIndex;
use gv_core::models::activity::ActivityName;
use gv_core::validation::{Email, Username};
use uuid::Uuid;

/// Macro: emit generic `Type` / `Encode` / `Decode` impls that delegate
/// to a sqlx-known primitive type (`String`, `Uuid`, `DateTime<Utc>`, ...).
///
/// `$column` is the newtype, `$primitive` is the wire-level type whose
/// sqlx impls we forward to, and the two closures convert between the
/// inner core value and `$primitive`.
///
/// `encode_to` takes `&Inner` and returns owned `$primitive`. `decode_from`
/// takes `$primitive` and returns `Result<Inner, BoxDynError>`.
macro_rules! impl_column_via {
    (
        column: $column:ident,
        primitive: $primitive:ty,
        encode_to: $encode_to:expr,
        decode_from: $decode_from:expr $(,)?
    ) => {
        impl<DB> sqlx::Type<DB> for $column
        where
            DB: sqlx::Database,
            $primitive: sqlx::Type<DB>,
        {
            fn type_info() -> DB::TypeInfo {
                <$primitive as sqlx::Type<DB>>::type_info()
            }
            fn compatible(ty: &DB::TypeInfo) -> bool {
                <$primitive as sqlx::Type<DB>>::compatible(ty)
            }
        }

        impl<'q, DB> sqlx::Encode<'q, DB> for $column
        where
            DB: sqlx::Database,
            $primitive: sqlx::Encode<'q, DB>,
        {
            fn encode_by_ref(
                &self,
                buf: &mut <DB as sqlx::Database>::ArgumentBuffer<'q>,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                let encoded: $primitive = ($encode_to)(&self.0);
                <$primitive as sqlx::Encode<'q, DB>>::encode(encoded, buf)
            }
        }

        impl<'r, DB> sqlx::Decode<'r, DB> for $column
        where
            DB: sqlx::Database,
            $primitive: sqlx::Decode<'r, DB>,
        {
            fn decode(
                value: <DB as sqlx::Database>::ValueRef<'r>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                let raw = <$primitive as sqlx::Decode<'r, DB>>::decode(value)?;
                let inner = ($decode_from)(raw)?;
                Ok($column(inner))
            }
        }
    };
}

// --- Validated string newtypes ---

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmailColumn(pub Email);

impl From<Email> for EmailColumn {
    fn from(v: Email) -> Self {
        EmailColumn(v)
    }
}
impl From<EmailColumn> for Email {
    fn from(c: EmailColumn) -> Self {
        c.0
    }
}

impl_column_via! {
    column: EmailColumn,
    primitive: String,
    encode_to: |e: &Email| e.as_str().to_string(),
    decode_from: |s: String| Email::parse(s)
        .map_err(|e| Box::new(e) as sqlx::error::BoxDynError),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UsernameColumn(pub Username);

impl From<Username> for UsernameColumn {
    fn from(v: Username) -> Self {
        UsernameColumn(v)
    }
}
impl From<UsernameColumn> for Username {
    fn from(c: UsernameColumn) -> Self {
        c.0
    }
}

impl_column_via! {
    column: UsernameColumn,
    primitive: String,
    encode_to: |u: &Username| u.as_str().to_string(),
    decode_from: |s: String| Username::parse(s)
        .map_err(|e| Box::new(e) as sqlx::error::BoxDynError),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActivityNameColumn(pub ActivityName);

impl From<ActivityName> for ActivityNameColumn {
    fn from(v: ActivityName) -> Self {
        ActivityNameColumn(v)
    }
}
impl From<ActivityNameColumn> for ActivityName {
    fn from(c: ActivityNameColumn) -> Self {
        c.0
    }
}

impl_column_via! {
    column: ActivityNameColumn,
    primitive: String,
    encode_to: |n: &ActivityName| n.to_string(),
    decode_from: |s: String| ActivityName::parse(s)
        .map_err(|e| Box::new(e) as sqlx::error::BoxDynError),
}

// --- Fractional index (string-encoded on both backends) ---

#[derive(Debug, Clone, PartialEq)]
pub struct FractionalIndexColumn(pub FractionalIndex);

impl From<FractionalIndex> for FractionalIndexColumn {
    fn from(v: FractionalIndex) -> Self {
        FractionalIndexColumn(v)
    }
}
impl From<FractionalIndexColumn> for FractionalIndex {
    fn from(c: FractionalIndexColumn) -> Self {
        c.0
    }
}

impl_column_via! {
    column: FractionalIndexColumn,
    primitive: String,
    encode_to: |f: &FractionalIndex| f.to_string(),
    decode_from: |s: String| FractionalIndex::from_string(&s)
        .map_err(|e| Box::new(e) as sqlx::error::BoxDynError),
}

// --- Sqlx-native primitives wrapped to land the encoding rule in gv_sql ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UuidColumn(pub Uuid);

impl From<Uuid> for UuidColumn {
    fn from(v: Uuid) -> Self {
        UuidColumn(v)
    }
}
impl From<UuidColumn> for Uuid {
    fn from(c: UuidColumn) -> Self {
        c.0
    }
}

impl<DB> sqlx::Type<DB> for UuidColumn
where
    DB: sqlx::Database,
    Uuid: sqlx::Type<DB>,
{
    fn type_info() -> DB::TypeInfo {
        <Uuid as sqlx::Type<DB>>::type_info()
    }
    fn compatible(ty: &DB::TypeInfo) -> bool {
        <Uuid as sqlx::Type<DB>>::compatible(ty)
    }
}

impl<'q, DB> sqlx::Encode<'q, DB> for UuidColumn
where
    DB: sqlx::Database,
    Uuid: sqlx::Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <Uuid as sqlx::Encode<'q, DB>>::encode_by_ref(&self.0, buf)
    }
}

impl<'r, DB> sqlx::Decode<'r, DB> for UuidColumn
where
    DB: sqlx::Database,
    Uuid: sqlx::Decode<'r, DB>,
{
    fn decode(
        value: <DB as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        Ok(UuidColumn(<Uuid as sqlx::Decode<'r, DB>>::decode(value)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateTimeColumn(pub DateTime<Utc>);

impl From<DateTime<Utc>> for DateTimeColumn {
    fn from(v: DateTime<Utc>) -> Self {
        DateTimeColumn(v)
    }
}
impl From<DateTimeColumn> for DateTime<Utc> {
    fn from(c: DateTimeColumn) -> Self {
        c.0
    }
}

impl<DB> sqlx::Type<DB> for DateTimeColumn
where
    DB: sqlx::Database,
    DateTime<Utc>: sqlx::Type<DB>,
{
    fn type_info() -> DB::TypeInfo {
        <DateTime<Utc> as sqlx::Type<DB>>::type_info()
    }
    fn compatible(ty: &DB::TypeInfo) -> bool {
        <DateTime<Utc> as sqlx::Type<DB>>::compatible(ty)
    }
}

impl<'q, DB> sqlx::Encode<'q, DB> for DateTimeColumn
where
    DB: sqlx::Database,
    DateTime<Utc>: sqlx::Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <DateTime<Utc> as sqlx::Encode<'q, DB>>::encode_by_ref(&self.0, buf)
    }
}

impl<'r, DB> sqlx::Decode<'r, DB> for DateTimeColumn
where
    DB: sqlx::Database,
    DateTime<Utc>: sqlx::Decode<'r, DB>,
{
    fn decode(
        value: <DB as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        Ok(DateTimeColumn(
            <DateTime<Utc> as sqlx::Decode<'r, DB>>::decode(value)?,
        ))
    }
}
