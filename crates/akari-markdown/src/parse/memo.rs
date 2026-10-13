// D73. A region (one Parser::region call) has a fixed end and context, so a search that failed
// from a position fails from any later one.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

#[derive(Default)]
pub(super) struct Frame {
    pub(super) start: usize,
    pub(super) no_fence_from: Option<usize>,
    pub(super) no_bold_from: Option<usize>,
    pub(super) no_underline_from: Option<usize>,
    pub(super) no_strike_from: Option<usize>,
    pub(super) no_spoiler_from: Option<usize>,
    pub(super) doomed_star: Option<Doomed>,
    pub(super) brackets: Option<Brackets>,
    pub(super) links_from: HashMap<usize, Option<(usize, UrlPart)>>,
    pub(super) url_failed: HashSet<usize>,
    pub(super) title_closes: Option<Vec<(usize, usize)>>,
    pub(super) close_paren: Option<(usize, Option<usize>)>,
    pub(super) colon_space_gt: Option<(usize, usize)>,
    pub(super) space_gt: Option<(usize, usize)>,
    pub(super) gt_or_space: Option<(usize, usize)>,
    pub(super) doomed_underscore: Option<Doomed>,
    pub(super) runs: Option<Runs>,
    pub(super) code_dead_until: usize,
}

impl Frame {
    // Built in its own call, so its temporaries never sit in region's frame.
    pub(super) fn boxed(start: usize) -> Box<Self> {
        Box::new(Self {
            start,
            ..Self::default()
        })
    }
}

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

    pub(super) fn find(&self, len: usize, from: usize) -> Option<usize> {
        let first = self.starts.partition_point(|&s| s < from);
        let runs = self.by_len.get(&len)?;
        let i = runs.partition_point(|&r| r < first);
        runs.get(i).map(|&r| self.starts[r])
    }
}

// A later italic search that reaches a doomed position goes on exactly like the failed one.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UrlPart {
    pub(super) url: Range<usize>,
    pub(super) suppress: bool,
    pub(super) title: Option<Range<usize>>,
    pub(super) end: usize,
}

pub(super) struct Brackets {
    end: usize,
    opens: Vec<usize>,
    closes: Vec<usize>,
    text_ends: HashMap<usize, usize>,
}

impl Brackets {
    pub(super) fn new(bytes: &[u8], range: Range<usize>, steps: &mut u64) -> Self {
        *steps += range.len() as u64;
        let mut opens = Vec::new();
        let mut closes = Vec::new();
        for (i, &b) in bytes[range.clone()].iter().enumerate() {
            match b {
                b'[' => opens.push(range.start + i),
                b']' => closes.push(range.start + i),
                _ => {}
            }
        }
        Self {
            end: range.end,
            opens,
            closes,
            text_ends: HashMap::new(),
        }
    }

    fn next(list: &[usize], from: usize) -> Option<usize> {
        list.get(list.partition_point(|&p| p < from)).copied()
    }

    pub(super) fn next_open(&self, from: usize) -> Option<usize> {
        Self::next(&self.opens, from)
    }

    pub(super) fn closes_in(&self, range: Range<usize>) -> impl Iterator<Item = usize> + '_ {
        let first = self.closes.partition_point(|&p| p < range.start);
        let last = self.closes.partition_point(|&p| p < range.end);
        self.closes[first..last].iter().copied()
    }

    // Where `(?:\[[^\]]*\]|[^\[\]])*` stops.
    pub(super) fn text_end(&mut self, from: usize, steps: &mut u64) -> usize {
        let mut visited = Vec::new();
        let mut pos = from;
        let end = loop {
            *steps += 1;
            if let Some(&known) = self.text_ends.get(&pos) {
                break known;
            }
            visited.push(pos);
            let open = Self::next(&self.opens, pos);
            let close = Self::next(&self.closes, pos);
            match (open, close) {
                (None, None) => break self.end,
                (_, Some(close)) if open.is_none_or(|open| close < open) => break close,
                (Some(open), _) => match Self::next(&self.closes, open + 1) {
                    Some(group_end) => pos = group_end + 1,
                    None => break open,
                },
                (None, Some(_)) => unreachable!("handled above"),
            }
        };
        for pos in visited {
            self.text_ends.insert(pos, end);
        }
        end
    }
}

impl Frame {
    pub(super) fn next_close_paren(
        &mut self,
        bytes: &[u8],
        from: usize,
        steps: &mut u64,
    ) -> Option<usize> {
        if let Some((searched, found)) = self.close_paren
            && from >= searched
            && found.is_none_or(|f| from <= f)
        {
            return found;
        }
        let found = bytes[from..]
            .iter()
            .position(|&b| b == b')')
            .map(|i| from + i);
        *steps += (found.unwrap_or(bytes.len()) - from) as u64 + 1;
        self.close_paren = Some((from, found));
        found
    }

    pub(super) fn next_colon_space_or_gt(
        &mut self,
        bytes: &[u8],
        from: usize,
        steps: &mut u64,
    ) -> usize {
        next_of(&mut self.colon_space_gt, bytes, from, steps, |b| {
            matches!(b, b':' | b' ' | b'>')
        })
    }

    pub(super) fn next_gt_or_space(&mut self, bytes: &[u8], from: usize, steps: &mut u64) -> usize {
        next_of(&mut self.gt_or_space, bytes, from, steps, |b| {
            b == b'>' || b.is_ascii_whitespace()
        })
    }

    pub(super) fn next_space_or_gt(&mut self, bytes: &[u8], from: usize, steps: &mut u64) -> usize {
        next_of(&mut self.space_gt, bytes, from, steps, |b| {
            matches!(b, b' ' | b'>')
        })
    }

    // The title's lazy `.*?['"]\s*\)`: its first quote that whitespace and ")" follow.
    pub(super) fn title_close(
        &mut self,
        bytes: &[u8],
        text: &str,
        from: usize,
        steps: &mut u64,
    ) -> Option<(usize, usize)> {
        let closes = self.title_closes.get_or_insert_with(|| {
            let mut closes = Vec::new();
            for (i, &b) in bytes.iter().enumerate().skip(self.start) {
                if matches!(b, b'\'' | b'"') {
                    let after = i
                        + 1
                        + text[i + 1..]
                            .chars()
                            .take_while(|c| c.is_whitespace())
                            .map(char::len_utf8)
                            .sum::<usize>();
                    *steps += (after - i) as u64;
                    if bytes.get(after) == Some(&b')') {
                        closes.push((i, after + 1));
                    }
                }
            }
            *steps += (bytes.len() - self.start) as u64;
            closes
        });
        closes
            .get(closes.partition_point(|&(q, _)| q < from))
            .copied()
    }
}

fn next_of(
    memo: &mut Option<(usize, usize)>,
    bytes: &[u8],
    from: usize,
    steps: &mut u64,
    stop: impl Fn(u8) -> bool,
) -> usize {
    if let Some((searched, found)) = *memo
        && (searched..=found).contains(&from)
    {
        return found;
    }
    let found = from
        + bytes[from..]
            .iter()
            .position(|&b| stop(b))
            .unwrap_or(bytes.len() - from);
    *steps += (found - from) as u64 + 1;
    *memo = Some((from, found));
    found
}
