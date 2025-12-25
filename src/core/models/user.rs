use crate::core::{
    delta::Delta,
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

impl User {
    pub fn update(&self) -> UserUpdateBuilder {
        UserUpdateBuilder {
            id: self.actor_id,
            old: self.clone(),
            new: self.clone(),
        }
    }
}

#[derive(Debug)]
pub struct UserUpdateBuilder {
    id: Uuid,
    old: User,
    new: User,
}

impl UserUpdateBuilder {
    pub fn username(mut self, username: Username) -> Self {
        self.new.username = username;
        self
    }

    pub fn email(mut self, email: Email) -> Self {
        self.new.email = email;
        self
    }

    pub fn build(self) -> Delta<User> {
        Delta::<User>::Update {
            id: self.id,
            old: self.old,
            new: self.new,
        }
    }
}

#[derive(Debug)]
pub struct UserPatch {
    pub username: Option<Username>,
    pub email: Option<Email>,
}
