//! Q5 verification: does `rust_decimal::Decimal` hash equal for values
//! that compare equal?
//!
//! See `docs/notes/questions/q05-rust-decimal-hash.md`.
//!
//! Only load-bearing if ADR 0001 ends up carrying `rust_decimal` as the
//! `Price` representation. Q1-Q4 lean toward integer storage, in which
//! case this check documents whether the fallback is safe.

use rust_decimal::Decimal;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

fn hash_of(value: &Decimal) -> u64 {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

// The core question. Hash contract: `a == b` implies `hash(a) == hash(b)`.
// If Decimal violates this, using it as a HashMap key is a latent bug:
// insert under one representation, lookup under another, no result.
#[test]
fn equal_decimals_must_hash_equal() {
    let a = Decimal::from_str("1.10").unwrap();
    let b = Decimal::from_str("1.1").unwrap();

    println!("a       = {}  (scale = {})", a, a.scale());
    println!("b       = {}  (scale = {})", b, b.scale());
    println!("a == b  ? {}", a == b);
    println!("hash(a) = {}", hash_of(&a));
    println!("hash(b) = {}", hash_of(&b));

    assert_eq!(a, b, "precondition: Decimals should compare equal");
    assert_eq!(
        hash_of(&a),
        hash_of(&b),
        "Decimals compare equal but hash differently: Decimal is unsafe as a HashMap key"
    );
}

// Broader check: several representations of 1.
#[test]
fn multiple_representations_of_one_hash_equal() {
    let variants = [
        Decimal::from_str("1").unwrap(),
        Decimal::from_str("1.0").unwrap(),
        Decimal::from_str("1.00").unwrap(),
        Decimal::from_str("1.000000000000").unwrap(),
    ];

    let base = &variants[0];
    let base_hash = hash_of(base);
    for v in &variants[1..] {
        println!("variant {} scale={} hash={}", v, v.scale(), hash_of(v));
        assert_eq!(*v, *base, "variant {v} should compare equal to base");
        assert_eq!(
            hash_of(v),
            base_hash,
            "variant {v} hashes differently from base"
        );
    }
}

// The user-visible failure mode. If Hash and PartialEq disagree, a
// HashMap keyed on Decimal loses the entry as soon as you look up a
// value with a different scale-preserving spelling.
#[test]
fn hashmap_lookup_survives_equal_variants() {
    use std::collections::HashMap;
    let mut m: HashMap<Decimal, &'static str> = HashMap::new();
    m.insert(Decimal::from_str("1.10").unwrap(), "hi");
    let looked_up = m.get(&Decimal::from_str("1.1").unwrap());
    println!("lookup result = {:?}", looked_up);
    assert_eq!(
        looked_up,
        Some(&"hi"),
        "HashMap<Decimal, _> cannot round-trip equal-valued keys with different scales"
    );
}
