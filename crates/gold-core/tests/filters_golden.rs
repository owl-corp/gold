//! Compares filter presets and trade requests with the golden outputs on the same
//! corpus as `parser_golden.rs`.
use gold_core::data::GameData;
use gold_core::parser::parse_clipboard;
use gold_core::price_check::filters::presets::{create_presets, PresetOptions, Unsupported};
use gold_core::price_check::trade::request::create_trade_request;
use regex::Regex;
use serde_json::{json, Value};
use std::path::Path;

fn normalise(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(normalise).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalise(v))).collect()),
        other => other,
    }
}

fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => {
                        if let Some(d) = first_diff(p, q, &format!("{path}.{k}")) {
                            return Some(d);
                        }
                    }
                    (p, q) => return Some(format!("{path}.{k}: rust={p:?} ts={q:?}")),
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{path}: array length rust={} ts={}", x.len(), y.len()));
            }
            x.iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (p, q))| first_diff(p, q, &format!("{path}[{i}]")))
        }
        (Value::Number(x), Value::Number(y)) => {
            // sums can differ in the last bit between the two languages
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            ((x - y).abs() > 1e-9 * x.abs().max(y.abs()).max(1.0)).then(|| format!("{path}: rust={x} ts={y}"))
        }
        _ if a == b => None,
        _ => Some(format!("{path}: rust={a} ts={b}")),
    }
}

fn split_corpus(input: &str) -> Vec<String> {
    Regex::new(r"\n=====\n?")
        .unwrap()
        .split(input)
        .map(|t| t.trim_start_matches('\n').to_string())
        .filter(|t| !t.trim().is_empty())
        .collect()
}

/// The defaults of the price check widget.
fn options() -> PresetOptions {
    PresetOptions {
        league: "Covens".into(),
        merchant_only: false,
        currency: None,
        collapse_listings: "api",
        activate_stock_filter: false,
        search_stat_range: 10.0,
        use_en: false,
    }
}

#[test]
fn rust_filters_match_golden() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let data = GameData::load(&root.join("../../assets"), "en").unwrap();

    let (mut checked, mut skipped) = (0, 0);
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(root.join("tests/corpus")).unwrap() {
        let corpus = entry.unwrap().path();
        let name = corpus.file_stem().unwrap().to_str().unwrap().to_string();
        let texts = split_corpus(&std::fs::read_to_string(&corpus).unwrap());
        let golden: Vec<Value> = serde_json::from_str(
            &std::fs::read_to_string(root.join(format!("tests/golden/{name}.filters.json"))).unwrap(),
        )
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
            let title = text.lines().take(3).collect::<Vec<_>>().join(" | ");

            let item = match parse_clipboard(&data, text) {
                Ok(item) => item,
                Err(e) => {
                    if expected["error"] != json!(e.code()) {
                        failures.push(format!(
                            "{name}[{i}] ({title}): parse error {} vs ts {:?}",
                            e.code(),
                            expected["error"]
                        ));
                    }
                    continue;
                }
            };

            let built = create_presets(&data, &item, &options()).and_then(|p| {
                let presets = p
                    .presets
                    .iter()
                    .map(|preset| {
                        let request = create_trade_request(&data, &preset.filters, &preset.stats)?;
                        Ok(json!({ "id": preset.id, "filters": preset.filters, "stats": preset.stats, "request": request }))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(json!({ "text": text, "active": p.active, "presets": presets }))
            });

            let outcome = match built {
                Ok(v) => v,
                Err(e) if e.is::<Unsupported>() => {
                    skipped += 1;
                    continue;
                }
                Err(e) => json!({ "text": text, "thrown": format!("{e:#}") }),
            };

            checked += 1;
            if outcome.get("thrown").is_some() || expected.get("thrown").is_some() {
                // both sides failing is enough; the messages differ between languages
                if outcome.get("thrown").is_some() != expected.get("thrown").is_some() {
                    failures.push(format!(
                        "{name}[{i}] ({title}): rust={:?} ts={:?}",
                        outcome.get("thrown"),
                        expected.get("thrown")
                    ));
                }
                continue;
            }
            if let Some(diff) = first_diff(&normalise(outcome), &normalise(expected.clone()), "") {
                failures.push(format!("{name}[{i}] ({title}): {diff}"));
            }
        }
    }

    eprintln!("checked {checked} items, skipped {skipped} unsupported");
    // an item wrongly reported as unsupported would otherwise drop out of the comparison unnoticed
    assert_eq!(skipped, 0, "no corpus item is expected to be unsupported");
    assert!(
        failures.is_empty(),
        "{} of {checked} items differ from the golden files:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(checked > 0);
}
