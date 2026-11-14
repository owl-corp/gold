//! Price check: poe.ninja prices, and later the trade site search.
pub mod details;
pub mod ninja;

/// `floorToBracket`: the largest bracket not above `value` (the first if none).
pub fn floor_to_bracket(value: f64, brackets: &[f64]) -> f64 {
    let mut prev = brackets[0];
    for &num in brackets {
        if num > value {
            return prev;
        }
        prev = num;
    }
    prev
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets() {
        assert_eq!(floor_to_bracket(70.0, &[1.0, 68.0, 73.0, 75.0]), 68.0);
        assert_eq!(floor_to_bracket(0.0, &[1.0, 68.0]), 1.0);
        assert_eq!(floor_to_bracket(99.0, &[1.0, 68.0]), 68.0);
    }
}
