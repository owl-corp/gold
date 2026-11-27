//! The game prints a map's citadel implicit cut short as `...Citadel(...)`.
#![expect(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]
use gold_core::data::GameData;
use gold_core::parser::parse_clipboard;
use std::path::Path;

const ITEM: &str = "Item Class: Maps
Rarity: Rare
Desolate Portent
Map (Tier 16)
--------
Item Level: 83
--------
Monster Level: 83
--------
{ Implicit Modifier }
Map contains Drox's Citadel(...) \u{2014} Unscalable Value
--------
{ Suffix Modifier \"of Blinding\" }
Monsters Blind on Hit
--------
Travel to a Map of this tier or lower by using this in a personal Map Device. Maps can only be used once.
";

#[test]
fn truncated_citadel_implicit_is_parsed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let data = GameData::load_with_extra(&root.join("../../assets"), None).unwrap();
    let value = serde_json::to_value(parse_clipboard(&data, ITEM).unwrap()).unwrap();
    let found = value["statsByType"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["type"] == "implicit" && s["stat"]["trade"]["ids"]["implicit"][0] == "implicit.stat_2563183002|4");
    assert!(found, "{value}");
}
