// Uniform in [0, 1), for jitter.
pub(crate) fn unit() -> f64 {
    let mut bytes = [0; 8];
    // Only fails without any OS entropy source; zero jitter is harmless then.
    if aws_lc_rs::rand::fill(&mut bytes).is_err() {
        return 0.0;
    }
    fraction(u64::from_le_bytes(bytes))
}

fn fraction(draw: u64) -> f64 {
    (draw >> 11) as f64 / (1_u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_values_lie_in_the_half_open_interval_and_vary() {
        let values: Vec<f64> = (0..1000).map(|_| unit()).collect();

        assert!(values.iter().all(|value| (0.0..1.0).contains(value)));
        assert!(values.iter().any(|value| *value != values[0]));
    }

    #[test]
    fn fraction_of_the_largest_draw_stays_below_one() {
        assert!(fraction(u64::MAX) < 1.0);
        assert_eq!(fraction(0), 0.0);
    }
}
