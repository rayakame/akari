//! The parser's work is linear in the input (D73), counted in steps so it holds on any machine.

use crate::parse_with_steps;

// About 200 steps per byte at worst: a rejected masked link's text is parsed to check it, and
// that text holds the next link to check, 64 levels deep (asterisk runs: 96, nesting italics).
const K: u64 = 256;

fn steps(source: &str) -> u64 {
    parse_with_steps(source).1
}

pub(crate) fn assert_bounded(source: &str) {
    let steps = steps(source);
    let bound = K * (source.len() as u64 + 1);
    assert!(
        steps <= bound,
        "{steps} steps for {} bytes (bound {bound}): {:?}",
        source.len(),
        source.chars().take(40).collect::<String>()
    );
}

/// Inputs that make a careless parser quadratic, about 4,000 bytes each at `scale` 1; at 2 each
/// is twice as long and made the same way.
pub(crate) fn pathological_at(scale: usize) -> Vec<String> {
    let r = |s: &str, n: usize| s.repeat(n * scale);
    let increasing_runs: String = (1..=88)
        .map(|n| "`".repeat(n) + "a")
        .collect::<String>()
        .repeat(scale);
    vec![
        r("`", 4000),
        r("` ``", 1000),
        "``` ".to_owned() + &r("a", 4000),
        r("```", 1333),
        r("`a", 2000) + "``",
        r("``a", 1333),
        increasing_runs,
        r("*", 4000),
        r("_", 4000),
        r("~", 4000),
        r("|", 4000),
        r("*_", 2000),
        r("_*", 2000),
        r("**a", 1333),
        r("__a", 1333),
        r("||a", 1333),
        r("~~a", 1333),
        r("*a", 2000),
        r("_a", 2000),
        r("\\*", 1333),
        r("**\\", 1000),
        r("~~a~~_", 666),
        r("> a\n", 1000),
        r("> ", 2000),
        r("- a\n", 1000),
        r("- ", 2000),
        r("1. ", 1333),
        r(&("1. ".repeat(12) + "a\n"), 100),
        r("-# # ", 800),
        r(">>> ", 1000),
        r("# \n", 1333),
        r("- a\n  ", 571),
        r("<@1", 1333),
        r("<t:", 1333),
        "<t:".to_owned() + &r("1", 4000),
        r("<:a:", 1000),
        "<:".to_owned() + &r("a", 4000),
        r("</a:", 1000),
        "</".to_owned() + &r("a ", 2000),
        r("<id:", 1000),
        r("@", 4000),
        r("😀", 1000),
        r("😀\u{301}", 666),
        r("🇦", 1000),
        r("1\u{20e3}", 1000),
        r("\u{301}", 2000),
        r("👨\u{200d}", 571),
        r(":a", 2000),
        r(":", 4000),
        r(":a:", 1333),
        r("©", 2000),
        r("日本語", 444),
        r("[", 4000),
        r("]", 4000),
        r("[a](", 1000),
        r("[a](x \"", 571),
        r("[a](x '", 571),
        r("](", 2000),
        r("[a]", 1333),
        r("[", 1000) + &r("](https://a.co)", 200),
        r("[<@1>](https://a.co)", 200),
        r("[[<@1>]](https://a.co)", 181),
        "[a](https://a.co/".to_owned() + &r("(", 4000),
        "[a](https://a.co/".to_owned() + &r("(b", 2000) + ")",
        "[a](".to_owned() + &r("\\", 2000),
        r("<", 4000),
        r("<a", 2000),
        r("<a:/", 1000),
        "https://".to_owned() + &r("(", 2000) + &r(")", 2000),
        "https://a.co/".to_owned() + &r("x", 4000),
        r("http://", 571),
        r("<a@b.co", 571),
        r("<+1", 1333),
    ]
}

#[test]
fn pathological_inputs_stay_linear() {
    for input in pathological_at(1) {
        assert_bounded(&input);
    }
}

#[test]
fn steps_grow_linearly() {
    for (once, twice) in pathological_at(1).into_iter().zip(pathological_at(2)) {
        let (a, b) = (steps(&once), steps(&twice));
        assert!(
            b * 10 <= a * 22 + 100,
            "{a} steps, then {b} for twice the input: {:?}",
            once.chars().take(40).collect::<String>()
        );
    }
}

#[test]
#[ignore = "prints the steps per byte of each pathological input, to set K"]
fn step_ratios() {
    for input in pathological_at(1) {
        let ratio = steps(&input) as f64 / (input.len() + 1) as f64;
        println!(
            "{ratio:6.2} {:?}",
            input.chars().take(30).collect::<String>()
        );
    }
}

#[test]
fn asterisk_runs_nest_at_most_64_deep() {
    // 2n + 2 asterisks nest n italics around "**"; at 65 the innermost level stays text.
    let doc = crate::parse(&"*".repeat(132));
    let inline = &doc.blocks()[0].inlines[0];
    assert_eq!(
        inline.kind,
        crate::InlineKind::Text(crate::Span { start: 64, end: 68 })
    );
    assert_eq!(inline.style, crate::Style::ITALIC);
}

#[test]
fn line_rules_nest_at_most_64_deep() {
    // Subtext and headings alternate without limit; the 65th level stays text.
    let source = "-# # ".repeat(40) + "a";
    let doc = crate::parse(&source);
    assert_eq!(doc.blocks().len(), 1);
    let block = &doc.blocks()[0];
    assert_eq!(
        block.kind,
        crate::BlockKind::Heading(crate::HeadingLevel::One)
    );
    let crate::InlineKind::Text(shown) = block.inlines[0].kind else {
        panic!("text expected: {block:?}");
    };
    assert_eq!(&source[shown.range()], "-# # ".repeat(8) + "a");
}
