//! Compares the Rust parser with the golden outputs on a corpus of item
//! texts. `tests/golden/*.json` holds the expected output.
#![expect(clippy::unwrap_used, reason = "tests")]
use gold_core::data::GameData;
use gold_core::parser::parse_clipboard;
use regex::Regex;
use serde_json::Value;
use std::path::Path;

fn normalise(v: Value) -> Value {
    match v {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(normalise).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalise(v))).collect()),
        other => other,
    }
}

/// Path of the first difference, with both values.
fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(left), Value::Object(right)) => {
            for key in left.keys().chain(right.keys().filter(|k| !left.contains_key(*k))) {
                match (left.get(key), right.get(key)) {
                    (Some(lv), Some(rv)) => {
                        if let Some(d) = first_diff(lv, rv, &format!("{path}.{key}")) {
                            return Some(d);
                        }
                    }
                    (lv, rv) => return Some(format!("{path}.{key}: rust={lv:?} ts={rv:?}")),
                }
            }
            None
        }
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                return Some(format!("{path}: array length rust={} ts={}", left.len(), right.len()));
            }
            left.iter()
                .zip(right)
                .enumerate()
                .find_map(|(i, (lv, rv))| first_diff(lv, rv, &format!("{path}[{i}]")))
        }
        _ if a == b => None,
        _ => Some(format!("{path}: rust={a} ts={b}")),
    }
}

/// Splits the corpus into items.
fn split_corpus(input: &str) -> Vec<String> {
    let sep = Regex::new(r"\n=====\n?").unwrap();
    sep.split(input)
        .map(|t| t.trim_start_matches('\n').to_string())
        .filter(|t| !t.trim().is_empty())
        .collect()
}

#[test]
fn rust_parser_matches_golden() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let data = GameData::load(&root.join("../../assets")).unwrap();

    let mut checked = 0;
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(root.join("tests/corpus")).unwrap() {
        let corpus = entry.unwrap().path();
        let name = corpus.file_stem().unwrap().to_str().unwrap().to_string();
        let texts = split_corpus(&std::fs::read_to_string(&corpus).unwrap());
        let golden: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(root.join(format!("tests/golden/{name}.json"))).unwrap())
                .unwrap();
        assert_eq!(
            texts.len(),
            golden.len(),
            "{name}: corpus and golden file have different item counts"
        );

        for (i, (text, expected)) in texts.iter().zip(&golden).enumerate() {
            assert_eq!(
                expected["text"],
                Value::String(text.clone()),
                "{name}[{i}]: golden file is stale"
            );
            let outcome = match parse_clipboard(&data, text) {
                Ok(item) => serde_json::json!({ "text": text, "item": item }),
                Err(e) => serde_json::json!({ "text": text, "error": e.code() }),
            };
            if let Some(diff) = first_diff(&normalise(outcome), &normalise(expected.clone()), "") {
                let title = text.lines().take(3).collect::<Vec<_>>().join(" | ");
                failures.push(format!("{name}[{i}] ({title}): {diff}"));
            }
            checked += 1;
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {checked} items differ from the golden files:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(checked > 0);
}
