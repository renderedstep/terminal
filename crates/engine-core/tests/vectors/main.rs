//! Runs every case of every golden vector file in `vectors/` against this
//! crate, and fails on any case whose answer differs from the Ruby engine's.
//!
//! Each portion checks its `constants` too, so a Ruby table that changed
//! under an unchanged case list still fails here.

mod portions;

use serde_json::{json, Value};
use std::path::PathBuf;

/// The format versions this harness knows how to read.
const KNOWN_VERSIONS: &[u64] = &[1];

pub struct Vectors {
    pub constants: Value,
    pub cases: Vec<Value>,
}

/// Reads one portion's file and refuses one it cannot read faithfully.
pub fn load(portion: &str) -> Vectors {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("vectors")
        .join(format!("{portion}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    let document: Value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("parsing {}: {error}", path.display()));
    check_header(&document, portion).unwrap_or_else(|problem| panic!("{portion}.json: {problem}"));
    Vectors {
        constants: document["constants"].clone(),
        cases: document["cases"].as_array().expect("cases").clone(),
    }
}

/// The header every file carries. An unknown version fails rather than being
/// read as if it were a known one.
pub fn check_header(document: &Value, portion: &str) -> Result<(), String> {
    if document["format"] != "engine-vectors" {
        return Err(format!(
            "not an engine-vectors file: {}",
            document["format"]
        ));
    }
    let version = document["version"]
        .as_u64()
        .ok_or_else(|| format!("no version: {}", document["version"]))?;
    if !KNOWN_VERSIONS.contains(&version) {
        return Err(format!(
            "format version {version} is not one this harness knows ({KNOWN_VERSIONS:?})"
        ));
    }
    if document["portion"] != portion {
        return Err(format!("portion is {}, not {portion}", document["portion"]));
    }
    if !document["cases"].is_array() {
        return Err("no cases".into());
    }
    Ok(())
}

/// Runs every case through `answer` and fails with the first few mismatches.
pub fn check(portion: &str, expected_constants: Value, answer: impl Fn(&Value) -> Value) {
    let vectors = load(portion);
    assert_eq!(
        vectors.constants, expected_constants,
        "{portion}: the constants differ from this crate's tables"
    );
    let mut failures = Vec::new();
    for case in &vectors.cases {
        let got = answer(&case["input"]);
        if got != case["output"] {
            failures.push(format!(
                "  {}\n    input:    {}\n    expected: {}\n    got:      {}",
                case["name"], case["input"], case["output"], got
            ));
        }
    }
    let total = vectors.cases.len();
    println!("{portion}: {}/{total} cases pass", total - failures.len());
    assert!(
        failures.is_empty(),
        "{portion}: {} of {total} cases differ; the first:\n{}",
        failures.len(),
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn an_unknown_format_version_is_refused() {
    let document =
        json!({ "format": "engine-vectors", "version": 2, "portion": "roll", "cases": [] });
    let problem = check_header(&document, "roll").unwrap_err();
    assert!(problem.contains("version 2"), "{problem}");
}

#[test]
fn a_file_that_is_not_vectors_is_refused() {
    let document =
        json!({ "format": "something-else", "version": 1, "portion": "roll", "cases": [] });
    assert!(check_header(&document, "roll").is_err());
}

#[test]
fn every_file_in_the_directory_has_a_portion() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vectors");
    let mut files: Vec<String> = std::fs::read_dir(directory)
        .expect("vectors directory")
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect();
    files.sort();
    let mut known: Vec<String> = portions::NAMES.iter().map(|n| n.to_string()).collect();
    known.sort();
    assert_eq!(
        files, known,
        "a vector file with no test, or a test with no file"
    );
}
