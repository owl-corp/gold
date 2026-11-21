//! Rounding of roll values.

fn decimal_places(value: f64, dp: bool) -> i32 {
    if !dp || value.abs() >= 10.0 {
        0
    } else if value.abs() < 2.3 {
        2
    } else {
        1
    }
}

#[must_use]
pub fn round_roll(value: f64, dp: bool) -> f64 {
    let rounding = 10f64.powi(decimal_places(value, dp));
    (value * rounding).trunc() / rounding
}

pub fn percent_roll(value: f64, p: f64, method: fn(f64) -> f64, dp: bool) -> f64 {
    let res = value + value.abs() * p / 100.0;
    let rounding = 10f64.powi(decimal_places(value, dp));
    method((res + f64::EPSILON) * rounding) / rounding
}

pub fn percent_roll_delta(value: f64, delta: f64, p: f64, method: fn(f64) -> f64, dp: bool) -> f64 {
    let res = value + delta * p / 100.0;
    let rounding = 10f64.powi(decimal_places(value, dp));
    method((res + f64::EPSILON) * rounding) / rounding
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding() {
        assert_eq!(round_roll(5.678, true), 5.6);
        assert_eq!(round_roll(1.234, true), 1.23);
        assert_eq!(round_roll(12.9, true), 12.0);
        assert_eq!(percent_roll(100.0, -10.0, f64::floor, false), 90.0);
        assert_eq!(percent_roll(100.0, 10.0, f64::ceil, false), 110.0);
    }
}
