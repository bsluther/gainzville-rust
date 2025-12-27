use gv_rust_2025_12::core::{models::activity::ActivityName, validation::Email};
use uuid::Uuid;

fn main() {
    println!("Hello world");
    let email = Email::parse("test@test.com".to_string());
    println!("email = {:?}", email);

    let act_name = ActivityName::parse(" test".to_string());
    println!("activity name = {:?}", act_name.unwrap().to_string());
}
