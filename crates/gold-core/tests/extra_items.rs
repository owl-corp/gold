//! Items from `gold data update` (generated from trade API data) go through the parser
//! and the request builder like shipped ones. Not covered by the golden corpus.
#![expect(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]
use gold_core::data::GameData;
use gold_core::data_update::extra_items;
use gold_core::parser::parse_clipboard;
use gold_core::price_check::filters::presets::{create_presets, PresetOptions};
use gold_core::price_check::trade::request::create_trade_request;
use serde_json::Value;
use std::path::Path;

const TRADE: &str = r#"{"result":[
    {"id":"currency","entries":[{"type":"Omen of Resurgence"}]},
    {"id":"card","entries":[{"type":"The Cheater"}]},
    {"id":"monster","entries":[{"type":"Junglemare"}]}
]}"#;
const STATIC: &str = r#"{"result":[
    {"id":"Currency","entries":[{"id":"omen-of-resurgence","text":"Omen of Resurgence","image":"/gen/image/x/Omen.png"}]},
    {"id":"Cards","entries":[{"id":"the-cheater","text":"The Cheater","image":"/gen/image/x/Card.png"}]}
]}"#;

const OMEN: &str = "Item Class: Stackable Currency
Rarity: Currency
Omen of Resurgence
--------
Stack Size: 1/10
--------
Whatever it does.
";

const CARD: &str = "Item Class: Divination Cards
Rarity: Divination Card
The Cheater
--------
Stack Size: 1/5
--------
Something rewarding.
";

fn request(data: &GameData, text: &str) -> Value {
    let options = PresetOptions {
        league: "Covens".into(),
        currency: None,
        collapse_listings: "api",
        activate_stock_filter: false,
    };
    let item = parse_clipboard(data, text).unwrap();
    let presets = create_presets(data, &item, &options).unwrap();
    let preset = &presets.presets[0];
    serde_json::to_value(create_trade_request(data, &preset.filters, &preset.stats).unwrap()).unwrap()
}

#[test]
fn generated_items_are_priced() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let shipped = std::fs::read_to_string(root.join("../../assets/en/items.ndjson")).unwrap();
    let extra = extra_items(TRADE, STATIC, &shipped).unwrap();
    assert_eq!(extra.bases, 3);

    let dir = std::env::temp_dir().join(format!("gold-extra-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("en")).unwrap();
    std::fs::write(
        dir.join("en").join(gold_core::data::EXTRA_ITEMS),
        extra.lines.join("\n"),
    )
    .unwrap();
    let data = GameData::load_with_extra(&root.join("../../assets"), Some(&dir)).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();

    let omen = request(&data, OMEN);
    assert_eq!(omen["query"]["type"], "Omen of Resurgence", "{omen}");

    let card = request(&data, CARD);
    assert_eq!(card["query"]["type"], "The Cheater", "{card}");
    assert!(data.items.by_translated("CAPTURED_BEAST", "Junglemare").is_some());
}
