//! Viscera are newer than the shipped data and have no "Rarity:" line. Their modifiers
//! are read anyway; this is not covered by the golden corpus.
use gold_core::data::GameData;
use gold_core::parser::parse_clipboard;
use gold_core::price_check::filters::presets::{create_presets, PresetOptions};
use gold_core::price_check::trade::request::create_trade_request;
use std::path::Path;

const ITEM: &str = "Item Class: Viscera
Chaos Golem's Core
--------
Torso
--------
Item Level: 62
--------
{ Implicit Modifier }
7(7-6)% reduced Chaos Damage taken over time
--------
Take this to the Wickerman in a town or hideout to attach it.
";

const PREFIXED: &str = "Item Class: Viscera
Lesser Crustacean's Claw
--------
Upper Limb
--------
Item Level: 6
--------
{ Implicit Modifier }
Adds 3(2-4) to 7(5-7) Physical Damage to Attacks
--------
Take this to the Wickerman in a town or hideout to attach it.
";

#[test]
fn viscera_modifiers_are_parsed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let extra = std::env::temp_dir().join(format!("gold-viscera-test-{}", std::process::id()));
    std::fs::create_dir_all(extra.join("en")).unwrap();
    std::fs::write(
        extra.join("en").join(gold_core::data::EXTRA_ITEMS),
        r#"{"name":"Chaos Golem's Core","refName":"Chaos Golem's Core","namespace":"ITEM","icon":"","craftable":{"category":"Viscera"}}
{"name":"Crustacean's Claw","refName":"Crustacean's Claw","namespace":"ITEM","icon":"","craftable":{"category":"Viscera"}}"#,
    )
    .unwrap();
    let data = GameData::load_with_extra(&root.join("../../data"), Some(&extra), "en").unwrap();
    std::fs::remove_dir_all(&extra).unwrap();

    let item = parse_clipboard(&data, ITEM).unwrap();
    let value = serde_json::to_value(&item).unwrap();
    let stats = &value["statsByType"];
    assert_eq!(stats.as_array().map(Vec::len), Some(1), "{stats}");
    assert_eq!(stats[0]["type"], "implicit");
    assert_eq!(stats[0]["stat"]["ref"], "#% reduced Chaos Damage taken over time");

    let options = PresetOptions {
        league: "Covens".into(),
        merchant_only: false,
        currency: None,
        collapse_listings: "api",
        activate_stock_filter: false,
        search_stat_range: 10.0,
        use_en: false,
    };
    let presets = create_presets(&data, &item, &options).unwrap();
    let preset = &presets.presets[0];
    let request = create_trade_request(&data, &preset.filters, &preset.stats).unwrap();
    println!("{request}");
    let filters = &request["query"]["stats"][0]["filters"];
    assert_eq!(filters[0]["id"], "implicit.stat_3762784591", "{request}");
    assert_eq!(request["query"]["type"], "Chaos Golem's Core", "{request}");

    // the tier prefix is not part of the base type the trade API knows
    let prefixed = parse_clipboard(&data, PREFIXED).unwrap();
    let preset = &create_presets(&data, &prefixed, &options).unwrap().presets[0];
    let request = create_trade_request(&data, &preset.filters, &preset.stats).unwrap();
    assert_eq!(request["query"]["type"], "Crustacean's Claw", "{request}");
    assert!(
        !request["query"]["stats"][0]["filters"].as_array().unwrap().is_empty(),
        "{request}"
    );
}
