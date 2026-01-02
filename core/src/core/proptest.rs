use crate::core::models::activity::{Activity, ActivityName};

use super::models::user::User;
use super::validation::{Email, Username};
use proptest::prelude::*;
use uuid::Uuid;

impl Arbitrary for Email {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        use proptest::sample::select;

        let local = "[a-z][a-z0-9._]{0,19}";
        let domains = select(vec![
            "gmail.com",
            "example.com",
            "test.org",
            "protonmail.com",
            "outlook.com",
        ]);

        (local, domains)
            .prop_map(|(l, d)| {
                Email::parse(format!("{}@{}", l, d)).expect("Generated email should be valid")
            })
            .boxed()
    }
}

impl Arbitrary for Username {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        "[a-zA-Z0-9_-]{1,50}"
            .prop_map(|s| Username::parse(s).expect("Generated username should be valid"))
            .boxed()
    }
}

pub fn arb_uuid() -> impl Strategy<Value = Uuid> {
    prop::array::uniform16(any::<u8>())
        .prop_map(|bytes| uuid::Builder::from_random_bytes(bytes).into_uuid())
}

impl Arbitrary for User {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (arb_uuid(), any::<Username>(), any::<Email>())
            .prop_map(|(actor_id, username, email)| User {
                actor_id,
                username,
                email,
            })
            .boxed()
    }
}

impl Arbitrary for ActivityName {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        "[a-zA-Z0-9_-]{1,49}"
            .prop_map(|s| ActivityName::parse(s).expect("Activity name is invalid"))
            .boxed()
    }
}

impl Arbitrary for Activity {
    type Parameters = Uuid;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(owner_id: Self::Parameters) -> Self::Strategy {
        (arb_uuid(), any::<ActivityName>(), "[a-zA-Z0-9_-]{1,50}")
            .prop_map(move |(id, name, description)| Activity {
                id,
                owner_id: owner_id.clone(),
                source_activity_id: None,
                name,
                description: Some(description),
            })
            .boxed()
    }
}
