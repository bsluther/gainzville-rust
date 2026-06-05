use crate::{
    constants::DEFAULT_USER_ID,
    delta::Delta,
    validation::{Email, Username},
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub actor_id: Uuid,
    pub username: Username,
    pub email: Email,
}

impl User {
    /// The single default user seeded into a fresh database, standing in for the
    /// authenticated user until an auth module lands. See [`DEFAULT_USER_ID`].
    pub fn default_user() -> Self {
        User {
            actor_id: DEFAULT_USER_ID,
            username: Username::parse("default".to_string())
                .expect("default username should be valid"),
            email: Email::parse("default@gainzville.net".to_string())
                .expect("default email should be valid"),
        }
    }

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
