#![expect(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]
use gold_core::data::GameData;
use gold_core::parser::parse_clipboard;
use gold_core::price_check::filters::presets::{create_presets, PresetOptions};
use gold_core::price_check::trade::request::create_trade_request;
use std::path::Path;

const ITEM: &str = "Item Class: Enshrouded Items
Enshrouded Helmet
--------
Original Unique: Alpha's Howl
--------
Item Level: 85
--------
Enshrouded items can be transformed within the Domain of Timeless Conflict.
";

#[test]
fn enshrouded_probe() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let extra = std::env::temp_dir().join(format!("gold-ens-test-{}", std::process::id()));
    std::fs::create_dir_all(extra.join("en")).unwrap();
    std::fs::write(
        extra.join("en").join(gold_core::data::EXTRA_ITEMS),
        r#"{"name":"Enshrouded Helmet","refName":"Enshrouded Helmet","namespace":"ITEM","icon":"","craftable":{"category":"Helmet"}}"#,
    )
    .unwrap();
    let data = GameData::load_with_extra(&root.join("../../assets"), Some(&extra)).unwrap();
    std::fs::remove_dir_all(&extra).unwrap();
    let item = parse_clipboard(&data, ITEM).unwrap();
    println!("{}", serde_json::to_string(&item).unwrap());
    let options = PresetOptions {
        league: "Covens".into(),
        currency: None,
        collapse_listings: "api",
        activate_stock_filter: false,
    };
    let presets = create_presets(&data, &item, &options).unwrap();
    let p = &presets.presets[0];
    println!(
        "{}",
        serde_json::to_string(&create_trade_request(&data, &p.filters, &p.stats).unwrap()).unwrap()
    );
}
