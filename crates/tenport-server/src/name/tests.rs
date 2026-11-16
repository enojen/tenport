use super::*;

#[test]
fn trims_the_name() {
    assert_eq!(Name::parse("  Wegobi ").unwrap().as_str(), "Wegobi");
}

#[test]
fn refuses_an_empty_name() {
    assert_eq!(Name::parse("   "), Err(NameError::Empty));
}

#[test]
fn refuses_a_name_that_is_too_long() {
    assert_eq!(Name::parse(&"a".repeat(201)), Err(NameError::TooLong));
    assert!(Name::parse(&"ş".repeat(200)).is_ok());
}

#[test]
fn json_with_an_empty_name_does_not_read() {
    assert!(serde_json::from_str::<Name>("\" \"").is_err());
}
