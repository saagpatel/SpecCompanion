use mutation_sample_crate::clamp;

#[test]
fn weak_test_only_checks_output_is_nonnegative() {
    assert!(clamp(5) >= 0);
}
