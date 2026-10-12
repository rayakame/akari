//! The parser's work is linear in the input (D73), counted in steps so it holds on any machine.

use crate::parse_with_steps;

// 4,000 asterisks take 96 steps per byte: they nest 64 italics, each scanning its content again.
const K: u64 = 128;

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

pub(crate) fn pathological() -> Vec<String> {
    let increasing_runs: String = (1..=88).map(|n| "`".repeat(n) + "a").collect();
    vec![
        "`".repeat(4000),
        "` ``".repeat(1000),
        "``` ".to_owned() + &"a".repeat(4000),
        "```".repeat(1333),
        "`a".repeat(2000) + "``",
        "``a".repeat(1333),
        increasing_runs,
        "*".repeat(4000),
        "_".repeat(4000),
        "~".repeat(4000),
        "|".repeat(4000),
        "*_".repeat(2000),
        "_*".repeat(2000),
        "**a".repeat(1333),
        "__a".repeat(1333),
        "||a".repeat(1333),
        "~~a".repeat(1333),
        "*a".repeat(2000),
        "_a".repeat(2000),
        "\\*".repeat(1333),
        "**\\".repeat(1000),
        "~~a~~_".repeat(666),
    ]
}

#[test]
fn pathological_inputs_stay_linear() {
    for input in pathological() {
        assert_bounded(&input);
    }
}

#[test]
fn steps_grow_linearly() {
    for input in pathological() {
        let half = &input[..input.len() / 2];
        let (once, twice) = (steps(half), steps(&input));
        assert!(
            twice * 10 <= once * 22 + 100,
            "{once} steps for half, {twice} for all: {:?}",
            input.chars().take(40).collect::<String>()
        );
    }
}

#[test]
#[ignore = "prints the steps per byte of each pathological input, to set K"]
fn step_ratios() {
    for input in pathological() {
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
