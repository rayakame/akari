// Hosts parse on their own threads, and message content comes from strangers.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

use super::bounds::{deep_contexts, pathological_at};
use super::corpus::{cases, corpus_dir, files};

const STACK: usize = 256 * 1024;
const PROBE: usize = 2048 * 1024;

fn read_dir(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut inputs: Vec<String> = entries
        .filter_map(|entry| fs::read(entry.ok()?.path()).ok())
        .filter_map(|bytes| String::from_utf8(bytes).ok())
        .collect();
    inputs.sort();
    inputs
}

fn inputs() -> Vec<String> {
    let mut inputs: Vec<String> = files(&corpus_dir())
        .iter()
        .flat_map(|(name, text)| cases(name, text))
        .map(|case| case.input)
        .collect();
    for (before, after) in deep_contexts() {
        for input in pathological_at(1) {
            inputs.push(format!("{before}{input}{after}"));
        }
    }
    inputs.extend(read_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fuzz-found"),
    ));
    inputs
}

#[inline(never)]
fn padded(kb: usize, f: &mut dyn FnMut()) {
    let block = std::hint::black_box([0u8; 1024]);
    if kb == 0 {
        f();
    } else {
        padded(kb - 1, f);
    }
    std::hint::black_box(&block);
}

fn here() -> usize {
    let marker = 0u8;
    std::hint::black_box(std::ptr::addr_of!(marker)) as usize
}

fn padding_used(kb: usize) -> usize {
    thread::Builder::new()
        .stack_size(PROBE)
        .spawn(move || {
            let top = here();
            let mut bottom = top;
            padded(kb, &mut || bottom = here());
            top - bottom
        })
        .unwrap()
        .join()
        .unwrap()
}

fn parse_all(inputs: Vec<String>, stack: usize, pad_kb: usize) {
    thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            padded(pad_kb, &mut || {
                for input in &inputs {
                    crate::parse(input);
                    crate::source_range(input, 0..input.len());
                }
            });
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn every_input_parses_on_a_256_kb_stack() {
    parse_all(inputs(), STACK, 0);
}

fn with_local_fuzz_corpus() -> Vec<String> {
    let fuzz = Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz");
    let mut inputs = inputs();
    inputs.extend(read_dir(&fuzz.join("corpus/parse")));
    inputs.extend(read_dir(&fuzz.join("artifacts/parse")));
    inputs
}

// Bisects in child processes, since an overflow aborts; with nothing to parse, the largest padding
// that fits measures the thread itself.
fn largest_padding(inputs: &str) -> usize {
    let (mut fits, mut overflows) = (0, PROBE / 1024);
    while overflows - fits > 1 {
        let pad = (fits + overflows) / 2;
        let ok = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "tests::stack::stack_peak"])
            .env("AKARI_STACK_PAD_KB", pad.to_string())
            .env("AKARI_STACK_INPUTS", inputs)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success();
        if ok {
            fits = pad;
        } else {
            overflows = pad;
        }
    }
    fits
}

#[test]
#[ignore = "prints the stack the inputs need, with the local fuzz corpus if there is one"]
fn stack_peak() {
    if let Some(pad) = std::env::var("AKARI_STACK_PAD_KB")
        .ok()
        .and_then(|kb| kb.parse().ok())
    {
        let inputs = match std::env::var("AKARI_STACK_INPUTS").as_deref() {
            Ok("all") => with_local_fuzz_corpus(),
            _ => Vec::new(),
        };
        parse_all(inputs, PROBE, pad);
        return;
    }
    parse_all(with_local_fuzz_corpus(), STACK, 0);
    let thread = padding_used(largest_padding("none"));
    let left = padding_used(largest_padding("all"));
    println!(
        "the inputs need {} KB, give or take {} bytes",
        (thread - left) / 1024,
        padding_used(2) - padding_used(1)
    );
}
