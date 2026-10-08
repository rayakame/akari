use std::time::Duration;

// Attempt 0 is immediate; jitter in [0, 1] scales into the upper half of the delay.
pub(crate) fn delay(attempt: u32, base: Duration, max: Duration, jitter: f64) -> Duration {
    if attempt == 0 {
        return Duration::ZERO;
    }
    let factor = 2_u32.checked_pow(attempt - 1).unwrap_or(u32::MAX);
    base.saturating_mul(factor)
        .min(max)
        .mul_f64(0.5 + jitter / 2.0)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const BASE: Duration = Duration::from_secs(1);
    const MAX: Duration = Duration::from_secs(60);

    #[test]
    fn the_first_attempt_is_immediate() {
        assert_eq!(delay(0, BASE, MAX, 0.9), Duration::ZERO);
    }

    #[test]
    fn later_attempts_double_with_jitter_into_the_upper_half() {
        assert_eq!(delay(1, BASE, MAX, 0.0), Duration::from_millis(500));
        assert_eq!(delay(1, BASE, MAX, 1.0), BASE);
        assert_eq!(delay(3, BASE, MAX, 0.0), Duration::from_secs(2));
        assert_eq!(delay(3, BASE, MAX, 1.0), Duration::from_secs(4));
    }

    #[test]
    fn delays_never_exceed_the_maximum() {
        assert_eq!(delay(7, BASE, MAX, 1.0), MAX);
        assert_eq!(delay(200, BASE, MAX, 1.0), MAX);
        assert_eq!(delay(u32::MAX, BASE, MAX, 0.0), MAX / 2);
    }
}
