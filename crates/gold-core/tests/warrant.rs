//! Mercenary warrants: one off-by-default filter per skill and support; enabling them searches
//! for the matching trade ids.
use gold_core::data::GameData;
use gold_core::parser::parse_clipboard;
use gold_core::price_check::filters::presets::{create_presets, PresetOptions};
use gold_core::price_check::trade::request::create_trade_request;
use std::path::Path;

#[test]
fn warrant_search() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let data = GameData::load(&root.join("../../assets"), "en").unwrap();
    let text = std::fs::read_to_string(root.join("tests/warrant.txt")).unwrap();
    let item = parse_clipboard(&data, &text).unwrap();
    let opts = PresetOptions {
        league: "Covens".into(),
        currency: None,
        collapse_listings: "api",
        activate_stock_filter: false,
    };
    let presets = create_presets(&data, &item, &opts).unwrap();
    let preset = &presets.presets[0];
    assert_eq!(preset.stats.len(), 23);
    assert!(preset.stats.iter().all(|s| s.disabled));
    assert_eq!(preset.stats[0].text, "Ice Shot");
    assert_eq!(preset.stats[1].text, "Greater Multiple Projectiles (Tier: 3)");

    let off = create_trade_request(&data, &preset.filters, &preset.stats).unwrap();
    let mut stats = preset.stats.clone();
    stats[0].disabled = false;
    stats[1].disabled = false;
    let on = create_trade_request(&data, &preset.filters, &stats).unwrap();
    let ids = |q: &serde_json::Value| -> Vec<String> {
        q["query"]["stats"][0]["filters"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["disabled"] == false)
            .map(|f| f["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(ids(&off).is_empty());
    assert_eq!(ids(&on), ["mercenary.skill_11495", "mercenary.support_49419"]);
    println!("{}", serde_json::to_string(&on).unwrap());
}
