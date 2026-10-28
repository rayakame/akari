use std::ops::Range;

use super::{Input, Match, Node, Parser, Piece};
use crate::document::{InlineKind, Navigation, TimestampStyle};

const MAX_SECONDS: i64 = 8_640_000_000_000;

fn digits(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count()
}

fn parse_u64(text: &str) -> Option<u64> {
    text.parse().ok()
}

fn is_command_char(c: char) -> bool {
    matches!(c, '-' | '_' | '\'')
        || c.is_alphanumeric()
        || matches!(u32::from(c), 0x0900..=0x097F | 0xA8E0..=0xA8FF | 0x0E00..=0x0E7F)
}

impl Parser {
    fn token(&mut self, input: &Input, range: Range<usize>, kind: InlineKind) -> Match {
        self.steps += range.len() as u64;
        Match::new(
            Node::Token {
                kind,
                source: input.span(range.clone()),
            },
            range.end,
        )
    }

    // An ID beyond u64 can't be a snowflake; such a token stays text, consumed whole.
    fn literal(&mut self, input: &Input, range: Range<usize>) -> Match {
        self.steps += range.len() as u64;
        let mut pieces = Vec::new();
        input.pieces(range.clone(), |span| pieces.push(Piece::Plain(span)));
        Match::new(Node::Text(pieces), range.end)
    }

    pub(super) fn angle_in_link(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
    ) -> Option<Match> {
        self.timestamp(input, at, range)
            .or_else(|| self.custom_emoji(input, at, range))
    }

    // Without `mentions` (an embed's title), only slash commands.
    pub(super) fn angle_mention(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        mentions: bool,
    ) -> Option<Match> {
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        let rest = &text[at..];
        if !mentions {
            return rest
                .starts_with("</")
                .then(|| self.command(input, at, range))
                .flatten();
        }
        let (prefix, make): (usize, fn(u64) -> InlineKind) = if rest.starts_with("<@!") {
            (3, InlineKind::UserMention)
        } else if rest.starts_with("<@&") {
            (3, InlineKind::RoleMention)
        } else if rest.starts_with("<@") {
            (2, InlineKind::UserMention)
        } else if rest.starts_with("<#") {
            (2, InlineKind::ChannelMention)
        } else if rest.starts_with("</") {
            return self.command(input, at, range);
        } else if rest.starts_with("<id:") {
            return self.navigation(input, at, range);
        } else {
            return None;
        };
        let start = at + prefix;
        let n = digits(bytes, start);
        if n == 0 || bytes.get(start + n) != Some(&b'>') {
            return None;
        }
        let end = start + n + 1;
        Some(match parse_u64(&text[start..start + n]) {
            Some(id) => self.token(input, at..end, make(id)),
            None => self.literal(input, at..end),
        })
    }

    // No word boundary, as parse-discord has it (observe: "@everyonee").
    pub(super) fn everyone(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
    ) -> Option<Match> {
        let rest = &input.text[at..range.end];
        if rest.starts_with("@everyone") {
            Some(self.token(input, at..at + 9, InlineKind::Everyone))
        } else if rest.starts_with("@here") {
            Some(self.token(input, at..at + 5, InlineKind::Here))
        } else {
            None
        }
    }

    // <t:(-?\d+)(?::([tTdDfFsSR]))?>; out of JavaScript's date range it's text.
    fn timestamp(&mut self, input: &Input, at: usize, range: &Range<usize>) -> Option<Match> {
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        if !text[at..].starts_with("<t:") {
            return None;
        }
        let start = at + 3;
        let sign = usize::from(bytes.get(start) == Some(&b'-'));
        let n = digits(bytes, start + sign);
        if n == 0 {
            return None;
        }
        let mut pos = start + sign + n;
        let mut style = TimestampStyle::LongDateShortTime;
        if bytes.get(pos) == Some(&b':') {
            style = match bytes.get(pos + 1)? {
                b't' => TimestampStyle::ShortTime,
                b'T' => TimestampStyle::MediumTime,
                b'd' => TimestampStyle::ShortDate,
                b'D' => TimestampStyle::LongDate,
                b'f' => TimestampStyle::LongDateShortTime,
                b'F' => TimestampStyle::FullDateShortTime,
                b's' => TimestampStyle::ShortDateShortTime,
                b'S' => TimestampStyle::ShortDateMediumTime,
                b'R' => TimestampStyle::Relative,
                _ => return None,
            };
            pos += 2;
        }
        if bytes.get(pos) != Some(&b'>') {
            return None;
        }
        let end = pos + 1;
        let seconds = text[start..start + sign + n]
            .parse::<i64>()
            .ok()
            .filter(|s| (-MAX_SECONDS..=MAX_SECONDS).contains(s));
        Some(match seconds {
            Some(seconds) => self.token(input, at..end, InlineKind::Timestamp { seconds, style }),
            None => self.literal(input, at..end),
        })
    }

    // <(a?):([A-Za-z0-9_]+):(\d+)>
    fn custom_emoji(&mut self, input: &Input, at: usize, range: &Range<usize>) -> Option<Match> {
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        let animated = text[at..].starts_with("<a:");
        let name_start = if animated {
            at + 3
        } else if text[at..].starts_with("<:") {
            at + 2
        } else {
            return None;
        };
        let name_len = bytes[name_start..]
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_')
            .count();
        let id_start = name_start + name_len + 1;
        if name_len == 0 || bytes.get(id_start - 1) != Some(&b':') {
            return None;
        }
        let n = digits(bytes, id_start);
        if n == 0 || bytes.get(id_start + n) != Some(&b'>') {
            return None;
        }
        let end = id_start + n + 1;
        Some(match parse_u64(&text[id_start..id_start + n]) {
            Some(id) => self.token(
                input,
                at..end,
                InlineKind::CustomEmoji {
                    name: input.span(name_start..name_start + name_len),
                    id,
                    animated,
                },
            ),
            None => self.literal(input, at..end),
        })
    }

    // docs.discord.food's command mention: up to three words of 1-32 characters, IDs of 17-21 digits.
    fn command(&mut self, input: &Input, at: usize, range: &Range<usize>) -> Option<Match> {
        let text = &input.text[..range.end];
        let name_start = at + 2;
        let mut pos = name_start;
        for word in 0..3 {
            if word > 0 {
                if !text[pos..].starts_with(' ') {
                    break;
                }
                pos += 1;
            }
            let chars = text[pos..]
                .chars()
                .take_while(|&c| is_command_char(c))
                .take(33);
            let (count, len) = chars.fold((0, 0), |(n, l), c| (n + 1, l + c.len_utf8()));
            if count == 0 || count > 32 {
                return None;
            }
            pos += len;
        }
        let name_end = pos;
        if !text[pos..].starts_with(':') {
            return None;
        }
        let id_start = pos + 1;
        let n = digits(text.as_bytes(), id_start);
        if !(17..=21).contains(&n) || text.as_bytes().get(id_start + n) != Some(&b'>') {
            return None;
        }
        let id = parse_u64(&text[id_start..id_start + n])?;
        Some(self.token(
            input,
            at..id_start + n + 1,
            InlineKind::Command {
                name: input.span(name_start..name_end),
                id,
            },
        ))
    }

    fn navigation(&mut self, input: &Input, at: usize, range: &Range<usize>) -> Option<Match> {
        let text = &input.text[..range.end];
        let rest = &text[at + 4..];
        let fixed = [
            ("customize>", Navigation::Customize),
            ("browse>", Navigation::Browse),
            ("guide>", Navigation::Guide),
            ("home>", Navigation::Guide),
            ("linked-roles>", Navigation::LinkedRoles(None)),
        ];
        for (name, target) in fixed {
            if rest.starts_with(name) {
                return Some(self.token(
                    input,
                    at..at + 4 + name.len(),
                    InlineKind::Navigation(target),
                ));
            }
        }
        let role = rest.strip_prefix("linked-roles:")?;
        let n = digits(role.as_bytes(), 0);
        if n == 0 || role.as_bytes().get(n) != Some(&b'>') {
            return None;
        }
        let id = parse_u64(&role[..n])?;
        let end = at + 4 + "linked-roles:".len() + n + 1;
        Some(self.token(
            input,
            at..end,
            InlineKind::Navigation(Navigation::LinkedRoles(Some(id))),
        ))
    }
}
