use mutation_sample_crate::{bounded_sum, choose, clamp};

#[test]
fn strong_test_checks_boundaries_and_operators() {
    assert_eq!(clamp(-1), 0);
    assert_eq!(clamp(0), 0);
    assert_eq!(clamp(11), 10);
    assert_eq!(bounded_sum(8, 9), 17);
    assert_eq!(bounded_sum(15, 9), 20);
    assert_eq!(choose(true, 4), 4);
    assert_eq!(choose(false, 4), 1);
    assert_eq!(choose(true, 0), 1);
}
