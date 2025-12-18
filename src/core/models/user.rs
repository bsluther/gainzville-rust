use crate::core::{
    model::Model,
    validation::{Email, Username},
};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub actor_id: Uuid,
    pub username: Username,
    pub email: Email,
}

pub struct UserPatch {
    pub username: Option<Username>,
    pub email: Option<Email>,
}

impl Model for User {
    const MODEL_NAME: &'static str = "user";
    const PRIMARY_KEY: &'static str = "actor_id";
    type Patch = UserPatch;
}
