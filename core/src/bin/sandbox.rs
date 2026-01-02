use gv_core::core::{models::activity::ActivityName, validation::Email};
use rand::prelude::*;

fn main() {
    println!("Hello world");
    let email = Email::parse("test@test.com".to_string());
    println!("email = {:?}", email);

    let act_name = ActivityName::parse(" test".to_string());
    println!("activity name = {:?}", act_name.unwrap().to_string());

    let mut rng = rand::rng();
    let random_bytes = rng.random();
    let _ = uuid::Builder::from_random_bytes(random_bytes).into_uuid();

    let xs = vec![1, 2, 3, 4, 5];
    let choices = rng.random_range(0..0);
    println!("choices = {choices}");
}
