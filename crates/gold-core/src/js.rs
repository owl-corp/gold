//! JavaScript number conversions that the golden outputs rely on.

/// `parseInt(s, 10)`: the leading integer, `None` where JavaScript gives NaN.
pub fn parse_int(s: &str) -> Option<i64> {
    let s = s.trim_start_matches(is_js_space);
    let (neg, rest) = s
        .strip_prefix('-')
        .map_or_else(|| (false, s.strip_prefix('+').unwrap_or(s)), |r| (true, r));
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    let v: i64 = rest.get(..end)?.parse().ok()?;
    Some(if neg { -v } else { v })
}

/// `Number(s)` for a field that is always whole: `None` where JavaScript gives NaN or a fraction.
pub fn int(s: &str) -> Option<i64> {
    let t = s.trim_matches(is_js_space);
    if t.is_empty() {
        return Some(0);
    }
    t.parse().ok()
}

/// `parseFloat(s)`
pub fn parse_float(s: &str) -> f64 {
    let s = s.trim_start_matches(is_js_space);
    let b = s.as_bytes();
    let at = |k: usize| b.get(k).copied();
    let is_digit = |k: usize| at(k).is_some_and(|c| c.is_ascii_digit());
    let is_sign = |k: usize| matches!(at(k), Some(b'+' | b'-'));
    let mut i = usize::from(is_sign(0));
    if s.get(i..).is_some_and(|r| r.starts_with("Infinity")) {
        return if at(0) == Some(b'-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let int_start = i;
    while is_digit(i) {
        i += 1;
    }
    let mut digits = i - int_start;
    if at(i) == Some(b'.') {
        let mut j = i + 1;
        while is_digit(j) {
            j += 1;
        }
        digits += j - (i + 1);
        if digits > 0 {
            i = j;
        }
    }
    if digits == 0 {
        return f64::NAN;
    }
    if matches!(at(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if is_sign(j) {
            j += 1;
        }
        let exp_start = j;
        while is_digit(j) {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    s.get(..i).and_then(|t| t.parse().ok()).unwrap_or(f64::NAN)
}

/// `Number(s)`
pub fn number(s: &str) -> f64 {
    let t = s.trim_matches(is_js_space);
    if t.is_empty() {
        return 0.0;
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    if t.chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'))
    {
        t.parse().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

const fn is_js_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// `a === b` for numbers: false when either is NaN, `0 === -0`.
#[must_use]
pub fn eq(a: f64, b: f64) -> bool {
    a.partial_cmp(&b) == Some(std::cmp::Ordering::Equal)
}

/// `Math.round`: halves round up (toward +Infinity).
#[must_use]
pub fn round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// `Math.sign(x)`
#[must_use]
pub fn sign(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        x
    } else {
        x.signum()
    }
}

/// `Math.max(a, b)`: NaN if either is NaN (unlike `f64::max`).
#[must_use]
pub const fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// `Math.min(a, b)`: NaN if either is NaN (unlike `f64::min`).
#[must_use]
pub const fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn int_and_float_prefixes() {
        assert_eq!(parse_int("20% (augmented)"), Some(20));
        assert_eq!(parse_int("+20%"), Some(20));
        assert_eq!(parse_int("-5"), Some(-5));
        assert_eq!(parse_int("abc"), None);
        assert!(close(parse_float("5.5%"), 5.5));
        assert_eq!(parse_float("1.50 (augmented)"), 1.5);
        assert!(parse_float(".").is_nan());
    }

    #[test]
    fn number_conversion() {
        assert!(close(number(" 12 "), 12.0));
        assert!(close(number(""), 0.0));
        assert!(number("12 (Max)").is_nan());
        assert_eq!(int("12"), Some(12));
        assert_eq!(int(""), Some(0));
        assert_eq!(int("12 (Max)"), None);
    }
}
