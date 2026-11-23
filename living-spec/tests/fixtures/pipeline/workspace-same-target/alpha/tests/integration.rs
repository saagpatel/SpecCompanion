fn value_length() -> usize {
    [1, 2, 3].len()
}

#[test]
fn smoke() {
    assert_eq!(value_length(), 3);
}

#[test]
fn unique() {
    assert_eq!(value_length(), 3);
}
