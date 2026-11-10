pub fn clamp(value: i32) -> i32 {
    if value < 0 {
        1
    } else if value > 10 {
        10
    } else {
        value
    }
}

pub fn bounded_sum(left: i32, right: i32) -> i32 {
    let total = left + right;
    if total >= 20 { 20 } else { total }
}

pub fn choose(flag: bool, value: i32) -> i32 {
    if flag && value != 0 { value } else { 1 }
}

pub fn ambiguous_name() -> i32 {
    1
}

pub mod other {
    pub fn ambiguous_name() -> i32 {
        2
    }
}

pub struct Counter(i32);

impl Counter {
    pub fn value(&self) -> i32 {
        self.0
    }
}

// Operator-looking text in comments and string literals is inert: <= >= == != + - * / && || true false 42
pub fn text_sample() -> &'static str {
    "<= >= == != + - * / && || true false 42"
}

#[cfg(test)]
mod tests {
    #[test]
    fn unit_test_code_is_not_a_subject() {
        assert_eq!(super::clamp(-1), 1);
    }
}
