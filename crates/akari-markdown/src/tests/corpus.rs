use std::fmt;
use std::fs;
use std::path::Path;

use super::outline::outline;

pub(crate) struct Case {
    pub(crate) name: String,
    pub(crate) observe: bool,
    pub(crate) input: String,
    pub(crate) expected: String,
}

pub(crate) struct Failure {
    pub(crate) file: String,
    pub(crate) case: String,
    pub(crate) expected: String,
    pub(crate) actual: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} / {}", self.file, self.case)?;
        writeln!(f, "  expected:")?;
        for line in self.expected.lines() {
            writeln!(f, "    {line}")?;
        }
        writeln!(f, "  actual:")?;
        for line in self.actual.lines() {
            writeln!(f, "    {line}")?;
        }
        Ok(())
    }
}

pub(crate) fn corpus_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

pub(crate) fn files(dir: &Path) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "txt"))
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read_to_string(&path).unwrap())
        })
        .collect();
    files.sort();
    files
}

pub(crate) fn cases(file: &str, text: &str) -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    let mut lines = text.lines().enumerate().peekable();
    while let Some((n, line)) = lines.next() {
        let Some(name) = line.strip_prefix("== ") else {
            assert!(
                line.is_empty() || line.starts_with('#'),
                "{file}:{}: expected a case (== name), a comment or a blank line: {line:?}",
                n + 1
            );
            continue;
        };
        let (observe, name) = match name.strip_prefix("observe: ") {
            Some(name) => (true, name),
            None => (false, name),
        };
        assert!(
            cases.iter().all(|c| c.name != name),
            "{file}:{}: duplicate case {name:?}",
            n + 1
        );
        let input = match lines.peek() {
            Some((_, l)) if l.starts_with("input: \"") => {
                let (n, l) = lines.next().unwrap();
                let input = unescape(&l["input: ".len()..])
                    .unwrap_or_else(|e| panic!("{file}:{}: {e}", n + 1));
                let (n, sep) = lines.next().unwrap();
                assert_eq!(sep, "--", "{file}:{}: expected -- after input:", n + 1);
                input
            }
            _ => {
                let mut raw = Vec::new();
                loop {
                    let (n, l) = lines
                        .next()
                        .unwrap_or_else(|| panic!("{file}: case {name:?} has no --"));
                    if l == "--" {
                        break;
                    }
                    assert!(
                        !l.starts_with("== "),
                        "{file}:{}: case {name:?} has no --",
                        n + 1
                    );
                    raw.push(l);
                }
                raw.join("\n")
            }
        };
        let mut expected = Vec::new();
        while let Some((_, l)) = lines.peek() {
            if l.starts_with("== ") {
                break;
            }
            let (_, l) = lines.next().unwrap();
            if !l.is_empty() && !l.starts_with('#') {
                expected.push(l);
            }
        }
        cases.push(Case {
            name: name.to_owned(),
            observe,
            input,
            expected: expected.join("\n"),
        });
    }
    cases
}

pub(crate) fn unescape(literal: &str) -> Result<String, String> {
    let inner = literal
        .strip_prefix('"')
        .and_then(|l| l.strip_suffix('"'))
        .ok_or("input: needs a quoted string")?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('u') => {
                let rest: String = chars.by_ref().take_while(|&c| c != '}').collect();
                let hex = rest.strip_prefix('{').ok_or("\\u needs {")?;
                let code = u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
                out.push(char::from_u32(code).ok_or("not a char")?);
            }
            other => return Err(format!("unknown escape \\{other:?}")),
        }
    }
    Ok(out)
}

pub(crate) fn run_cases(files: &[(String, String)]) -> Vec<Failure> {
    let mut failures = Vec::new();
    for (file, text) in files {
        for case in cases(file, text) {
            let actual = outline(&crate::parse(&case.input), &case.input);
            if actual != case.expected {
                failures.push(Failure {
                    file: file.clone(),
                    case: case.name,
                    expected: case.expected,
                    actual,
                });
            }
        }
    }
    failures
}

#[test]
fn the_corpus_matches() {
    let failures = run_cases(&files(&corpus_dir()));
    let report: String = failures.iter().map(ToString::to_string).collect();
    assert!(
        failures.is_empty(),
        "{} corpus cases failed:\n{report}",
        failures.len()
    );
}

#[test]
fn the_corpus_reports_every_failure_by_name() {
    let text = "# a comment\n\n== right\nabc\n--\np: \"abc\"\n\n== wrong\nx\n--\n# R§0\np: \"y\"\n";
    let failures = run_cases(&[("mem.txt".to_owned(), text.to_owned())]);
    assert_eq!(failures.len(), 1);
    let failure = &failures[0];
    assert_eq!(failure.file, "mem.txt");
    assert_eq!(failure.case, "wrong");
    assert_eq!(failure.expected, "p: \"y\"");
    assert_eq!(failure.actual, "p: \"x\"");
    let shown = failure.to_string();
    assert!(shown.contains("mem.txt / wrong"), "{shown}");
}

#[test]
fn raw_and_escaped_inputs_are_the_same() {
    let text =
        "== raw\na\n  \"b\\\n--\np:\n\n== observe: escaped\ninput: \"a\\n  \\\"b\\\\\"\n--\np:\n";
    let cases = cases("mem.txt", text);
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].input, "a\n  \"b\\");
    assert_eq!(cases[0].input, cases[1].input);
    assert!(!cases[0].observe);
    assert!(cases[1].observe);
    assert_eq!(cases[1].name, "escaped");
}

#[test]
fn every_area_has_a_corpus_file() {
    let names: Vec<String> = files(&corpus_dir()).into_iter().map(|(n, _)| n).collect();
    for area in ["text.txt", "escapes.txt"] {
        assert!(names.iter().any(|n| n == area), "missing {area}");
    }
}
