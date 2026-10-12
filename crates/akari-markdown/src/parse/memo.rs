//! What one region's earlier searches found, so later searches in the same region don't scan the
//! same text again (D73). A region is one call of `Parser::region`: its end and its context are
//! fixed, so a search that failed from some position fails from any later one too.

use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Frame {
    /// No fence at or after this position.
    pub(super) no_fence_from: Option<usize>,
    pub(super) no_bold_from: Option<usize>,
    pub(super) no_underline_from: Option<usize>,
    pub(super) no_strike_from: Option<usize>,
    pub(super) no_spoiler_from: Option<usize>,
    pub(super) doomed_star: Option<Doomed>,
    pub(super) doomed_underscore: Option<Doomed>,
    pub(super) runs: Option<Runs>,
    /// Inline code failed in the run that ends here, so the rest of the run fails too.
    pub(super) code_dead_until: usize,
}

/// The region's maximal runs of backticks.
pub(super) struct Runs {
    starts: Vec<usize>,
    by_len: HashMap<usize, Vec<usize>>,
}

impl Runs {
    pub(super) fn new(bytes: &[u8], from: usize, steps: &mut u64) -> Self {
        let mut starts = Vec::new();
        let mut by_len: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut i = from;
        while i < bytes.len() {
            *steps += 1;
            if bytes[i] == b'`' {
                let len = bytes[i..].iter().take_while(|&&b| b == b'`').count();
                *steps += len as u64;
                by_len.entry(len).or_default().push(starts.len());
                starts.push(i);
                i += len;
            } else {
                i += 1;
            }
        }
        Self { starts, by_len }
    }

    /// The first run of exactly `len` backticks starting at or after `from`.
    pub(super) fn find(&self, len: usize, from: usize) -> Option<usize> {
        let first = self.starts.partition_point(|&s| s < from);
        let runs = self.by_len.get(&len)?;
        let i = runs.partition_point(|&r| r < first);
        runs.get(i).map(|&r| self.starts[r])
    }
}

/// Positions an italic search has passed without finding its closer. A later search that reaches
/// one would go on exactly like the earlier one, so it fails there.
pub(super) struct Doomed {
    start: usize,
    bits: Vec<u64>,
}

impl Doomed {
    pub(super) fn new(range: std::ops::Range<usize>) -> Self {
        Self {
            start: range.start,
            bits: vec![0; (range.len() + 1).div_ceil(64)],
        }
    }

    pub(super) fn is(&self, at: usize) -> bool {
        let i = at - self.start;
        self.bits
            .get(i / 64)
            .is_some_and(|w| w & (1 << (i % 64)) != 0)
    }

    pub(super) fn set(&mut self, at: usize) {
        let i = at - self.start;
        if let Some(w) = self.bits.get_mut(i / 64) {
            *w |= 1 << (i % 64);
        }
    }
}
