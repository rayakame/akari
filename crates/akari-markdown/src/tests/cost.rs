//! Parse time, printed rather than asserted (D71); run it in release:
//! `cargo test --release -p akari-markdown -- --ignored parse_cost --nocapture`.

use std::hint::black_box;
use std::time::{Duration, Instant};

use super::bounds::pathological_at;
use super::corpus::{cases, corpus_dir};
use crate::parse;

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn time(sources: &[&str], rounds: usize) -> Duration {
    median(
        (0..rounds)
            .map(|_| {
                let start = Instant::now();
                for source in sources {
                    black_box(parse(black_box(source)));
                }
                start.elapsed()
            })
            .collect(),
    )
}

#[test]
#[ignore = "prints parse times; run in release"]
fn parse_cost() {
    let text = std::fs::read_to_string(corpus_dir().join("samples.txt")).unwrap();
    let samples = cases("samples.txt", &text);
    let mut per_sample: Vec<(Duration, &str, usize)> = samples
        .iter()
        .map(|s| {
            (
                time(&[s.input.as_str()], 201),
                s.name.as_str(),
                s.input.len(),
            )
        })
        .collect();
    per_sample.sort();
    let us = |d: Duration| d.as_secs_f64() * 1e6;
    println!("samples: {}", per_sample.len());
    println!(
        "per sample: median {:.1} µs, worst {:.1} µs ({}, {} bytes)",
        us(per_sample[per_sample.len() / 2].0),
        us(per_sample.last().unwrap().0),
        per_sample.last().unwrap().1,
        per_sample.last().unwrap().2
    );

    let typical: Vec<&str> = samples
        .iter()
        .filter(|s| s.input.len() < 1000)
        .map(|s| s.input.as_str())
        .collect();
    let page: Vec<&str> = typical.iter().cycle().take(50).copied().collect();
    let bytes: usize = page.iter().map(|s| s.len()).sum();
    println!(
        "page of 50 typical samples ({bytes} bytes): {:.1} µs",
        us(time(&page, 101))
    );
    let all: Vec<&str> = samples
        .iter()
        .map(|s| s.input.as_str())
        .cycle()
        .take(50)
        .collect();
    let bytes: usize = all.iter().map(|s| s.len()).sum();
    println!(
        "page of 50 samples, the 4,000-character one included ({bytes} bytes): {:.1} µs",
        us(time(&all, 101))
    );

    let mut worst: Vec<(Duration, String)> = pathological_at(1)
        .iter()
        .map(|p| (time(&[p.as_str()], 21), p.chars().take(24).collect()))
        .collect();
    worst.sort();
    for (d, name) in worst.iter().rev().take(5) {
        println!("pathological: {:.1} µs for {name:?}…", us(*d));
    }
}
