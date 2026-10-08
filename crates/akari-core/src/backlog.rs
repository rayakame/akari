const FIRST_WARNING: usize = 10_000;

pub(crate) struct Backlog {
    next_warning: usize,
}

impl Default for Backlog {
    fn default() -> Self {
        Self {
            next_warning: FIRST_WARNING,
        }
    }
}

impl Backlog {
    pub(crate) fn warns_at(&mut self, buffered: usize) -> bool {
        if buffered < FIRST_WARNING {
            self.next_warning = FIRST_WARNING;
            return false;
        }
        if buffered < self.next_warning {
            return false;
        }
        self.next_warning = self.next_warning.saturating_mul(2);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warnings(backlog: &mut Backlog, counts: impl IntoIterator<Item = usize>) -> Vec<usize> {
        counts
            .into_iter()
            .filter(|count| backlog.warns_at(*count))
            .collect()
    }

    #[test]
    fn warns_at_ten_thousand_and_every_doubling() {
        let mut backlog = Backlog::default();

        assert_eq!(
            warnings(&mut backlog, 1..=100_000),
            [10_000, 20_000, 40_000, 80_000]
        );
    }

    #[test]
    fn a_drained_backlog_warns_again() {
        let mut backlog = Backlog::default();
        warnings(&mut backlog, 1..=25_000);

        assert!(warnings(&mut backlog, [9_999]).is_empty());
        assert_eq!(warnings(&mut backlog, 10_000..=10_001), [10_000]);
    }

    #[test]
    fn a_shrinking_backlog_above_the_threshold_stays_quiet() {
        let mut backlog = Backlog::default();
        warnings(&mut backlog, 1..=25_000);

        assert!(warnings(&mut backlog, (15_000..25_000).rev()).is_empty());
    }
}
