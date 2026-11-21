//! Chronicle of Atzoatl: area level bracket plus one filter per room.
#![expect(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]
use gold_core::data::GameData;
use gold_core::parser::parse_clipboard;
use gold_core::price_check::filters::presets::{create_presets, PresetOptions};
use gold_core::price_check::trade::request::create_trade_request;
use std::path::Path;

#[test]
fn atzoatl_search() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let data = GameData::load(&root.join("../../assets")).unwrap();
    let text = std::fs::read_to_string(root.join("tests/atzoatl.txt")).unwrap();
    let item = parse_clipboard(&data, &text).unwrap();
    let opts = PresetOptions {
        league: "Covens".into(),
        currency: None,
        collapse_listings: "api",
        activate_stock_filter: false,
    };
    let presets = create_presets(&data, &item, &opts).unwrap();
    let preset = &presets.presets[0];
    let on: Vec<_> = preset
        .stats
        .iter()
        .filter(|s| !s.disabled)
        .map(|s| s.text.as_str())
        .collect();
    // "Cradle of Creation (Tier 3)" is not in the shipped stats, so it is an unknown modifier
    assert_eq!(preset.stats.len(), 11);
    assert_eq!(on, ["Wealth of the Vaal (Tier 3)", "Apex of Atzoatl"]);
    let req = serde_json::to_value(create_trade_request(&data, &preset.filters, &preset.stats).unwrap()).unwrap();
    assert_eq!(
        req["query"]["filters"]["map_filters"]["filters"]["area_level"]["min"],
        80.0
    );
}
