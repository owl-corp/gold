//! What the overlay shows. The QML renders this JSON as it is and keeps no state.
use gold_core::parser::item::{ItemRarity, ParsedItem};
use gold_core::price_check::details::{get_details_id, is_valuable_basetype};
use gold_core::price_check::ninja::{auto_currency, delta_from_graph, display_rounding, Currency, NinjaDb};
use serde_json::{json, Value};

pub struct Placement {
    pub screen: String,
    pub x: i32,
    pub y: i32,
}

fn base(p: &Placement, mut view: Value) -> Value {
    view["screen"] = json!(p.screen);
    view["x"] = json!(p.x);
    view["y"] = json!(p.y);
    view
}

pub fn error_view(p: &Placement, title: &str, lines: Vec<String>) -> Value {
    base(p, json!({ "title": title, "lines": lines }))
}

fn rarity_name(item: &ParsedItem) -> &'static str {
    match item.rarity {
        Some(ItemRarity::Normal) => "normal",
        Some(ItemRarity::Magic) => "magic",
        Some(ItemRarity::Rare) => "rare",
        Some(ItemRarity::Unique) => "unique",
        None => "currency",
    }
}

/// The item header. `price` is None while prices are still loading.
pub fn item_view(p: &Placement, item: &ParsedItem, ninja: Option<&Result<std::sync::Arc<NinjaDb>, String>>) -> Value {
    let mut lines = Vec::new();
    for m in &item.unknown_modifiers {
        lines.push(format!("? {}", m.text));
    }

    let (title, subtitle) = if item.info.ref_name == item.info.name && item.rarity != Some(ItemRarity::Unique) {
        (item.info.name.clone(), String::new())
    } else {
        let base = item.unique_base.as_ref().map(|b| b.name.clone()).unwrap_or_default();
        (item.info.name.clone(), base)
    };

    let price = match ninja {
        None => json!({ "status": "Loading prices…" }),
        Some(Err(e)) => json!({ "status": format!("Prices unavailable: {e}") }),
        Some(Ok(db)) => ninja_price(item, db),
    };

    base_view(
        p,
        json!({ "title": title, "subtitle": subtitle, "rarity": rarity_name(item), "price": price, "lines": lines }),
    )
}

fn base_view(p: &Placement, view: Value) -> Value {
    base(p, view)
}

fn ninja_price(item: &ParsedItem, db: &NinjaDb) -> Value {
    if is_valuable_basetype(item) && item.rarity != Some(ItemRarity::Unique) {
        // Equipment is priced by its mods on the trade site, not by poe.ninja.
        return json!({ "status": "Rare and magic items need the trade search" });
    }
    let Some(id) = get_details_id(item) else {
        return json!({ "status": "No price data for this item" });
    };
    let Some(found) = db.find(&id.ns, &id.name, id.variant.as_deref()) else {
        return json!({ "status": "Not listed on poe.ninja" });
    };

    let rate = db.divine_rate();
    let value = if item.info.ref_name == "Divine Orb" {
        auto_currency(found.info.chaos, None)
    } else {
        auto_currency(found.info.chaos, rate)
    };
    let (amount, unit) = match value.currency {
        Currency::Chaos if item.info.ref_name == "Divine Orb" => (display_rounding(found.info.chaos, false), "c"),
        Currency::Chaos => (display_rounding(value.min, false), "c"),
        Currency::Div => (display_rounding(value.min, false), "div"),
    };
    let mut price = json!({ "amount": amount, "unit": unit, "url": found.url });
    if let Some(trend) = delta_from_graph(&found.info.graph) {
        price["trend"] = json!(trend.forecast);
        price["trendText"] = json!(trend.text);
    }
    price
}

#[cfg(test)]
mod tests {
    use super::*;
    use gold_core::data::GameData;
    use gold_core::parser::parse_clipboard;
    use std::sync::Arc;

    const NINJA: &str = r#"{"currencyOverviews":[{"type":"Currency","lines":[
        {"name":"Chaos Orb","chaos":1,"graph":[null,null,null,null,null,null,-37.23]},
        {"name":"Divine Orb","chaos":150.5,"graph":[null,null,null,null,null,null,3.2]}]}],"itemOverviews":[]}"#;

    fn view_of(text: &str) -> Value {
        let data = GameData::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"),
            "en",
        )
        .unwrap();
        let item = parse_clipboard(&data, text).unwrap();
        let db = Ok(Arc::new(NinjaDb::from_json("Covens", NINJA.as_bytes()).unwrap()));
        item_view(
            &Placement {
                screen: "DP-1".into(),
                x: 10,
                y: 20,
            },
            &item,
            Some(&db),
        )
    }

    #[test]
    fn chaos_orb_is_one_chaos() {
        let v = view_of("Item Class: Stackable Currency\nRarity: Currency\nChaos Orb\n--------\nStack Size: 5/20\n");
        assert_eq!(v["price"]["amount"], "1");
        assert_eq!(v["price"]["unit"], "c");
    }

    #[test]
    fn divine_orb_shows_its_chaos_value() {
        let v = view_of("Item Class: Stackable Currency\nRarity: Currency\nDivine Orb\n--------\nStack Size: 1/10\n");
        assert_eq!(v["price"]["unit"], "c");
        assert_eq!(v["price"]["amount"], "151");
    }
}
