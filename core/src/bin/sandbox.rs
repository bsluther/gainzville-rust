use fractional_index::FractionalIndex;
use rand::prelude::*;

fn main() {
    let default_fi = FractionalIndex::default();
    println!("default: {:?}", default_fi.to_string());

    const TERMINATOR: u8 = 0b1000_0000;
    let mut rng = rand::rng();
    const SZ: usize = 1;
    let mut bytes = [0u8; SZ];
    rng.fill(&mut bytes);
    bytes[SZ - 1] = TERMINATOR;
    let random_fi = FractionalIndex::from_bytes(bytes.to_vec()).expect("Valid bytes");
    println!("Random index: {:?}", random_fi.to_string());
}
