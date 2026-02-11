use crate::{
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
    pub fn update(&self) -> UserUpdater {
        UserUpdater {
            old: self.clone(),
            new: self.clone(),
        }
    }
}

#[derive(Debug)]
pub struct UserUpdater {
    old: User,
    new: User,
}

impl UserUpdater {
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
            old: self.old,
            new: self.new,
        }
    }
}
