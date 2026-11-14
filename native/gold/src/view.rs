//! What the overlay shows. The QML renders this JSON as it is and keeps no state.
use crate::session::{SearchState, Session};
use gold_core::parser::item::{ItemRarity, ParsedItem};
use gold_core::price_check::details::{get_details_id, is_valuable_basetype};
use gold_core::price_check::filters::util::round_roll;
use gold_core::price_check::ninja::{auto_currency, delta_from_graph, display_rounding, Currency, NinjaDb};
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct Placement {
    pub screen: String,
    pub x: i32,
    pub y: i32,
}

fn with_placement(p: &Placement, mut view: Value) -> Value {
    view["screen"] = json!(p.screen);
    view["x"] = json!(p.x);
    view["y"] = json!(p.y);
    view
}

pub fn error_view(p: &Placement, title: &str, lines: Vec<String>) -> Value {
    with_placement(p, json!({ "title": title, "lines": lines }))
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

/// The whole overlay for a session.
pub fn render(s: &Session) -> Value {
    let item = &s.item;

    let (title, subtitle) = if item.info.ref_name == item.info.name && item.rarity != Some(ItemRarity::Unique) {
        (item.info.name.clone(), String::new())
    } else {
        (
            item.info.name.clone(),
            item.unique_base.as_ref().map(|b| b.name.clone()).unwrap_or_default(),
        )
    };

    let price = match &s.ninja {
        None => json!({ "status": "Loading prices…" }),
        Some(Err(e)) => json!({ "status": format!("Prices unavailable: {e}") }),
        Some(Ok(db)) => ninja_price(item, db),
    };

    let filters: Vec<Value> = s
        .preset()
        .map(|p| {
            p.stats
                .iter()
                .enumerate()
                .filter(|(_, f)| f.hidden.is_none())
                .map(|(i, f)| {
                    let value = f.roll.as_ref().map(|r| r.value);
                    let text = match value {
                        Some(v) => f.text.replace(
                            '#',
                            &display_rounding(round_roll(v, f.roll.as_ref().is_some_and(|r| r.dp)), false),
                        ),
                        None => f.text.clone(),
                    };
                    json!({
                        "index": i,
                        "text": text,
                        "tag": f.tag,
                        "on": !f.disabled,
                        "not": f.not.unwrap_or(false),
                        "min": f.roll.as_ref().and_then(|r| r.min),
                        "max": f.roll.as_ref().and_then(|r| r.max),
                        "editable": f.roll.is_some(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let presets: Vec<Value> = s
        .presets
        .iter()
        .enumerate()
        .map(|(i, p)| json!({ "index": i, "name": preset_name(p.id), "active": i == s.active }))
        .collect();

    let trade = match &s.search {
        SearchState::Idle => json!({ "state": "idle" }),
        SearchState::Loading => json!({ "state": "loading" }),
        SearchState::Failed(e) => json!({ "state": "failed", "message": e }),
        SearchState::Done(groups) => json!({
            "state": "done",
            "groups": groups.iter().map(|g| json!({
                "heading": g.heading,
                "total": g.total,
                "url": g.url,
                "rows": g.rows.iter().map(|r| json!({
                    "price": r.price, "detail": r.detail, "status": r.status, "who": r.who,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }),
    };

    with_placement(
        &s.place,
        json!({
            "title": title,
            "subtitle": subtitle,
            "rarity": rarity_name(item),
            "price": price,
            "presets": presets,
            "filters": filters,
            "trade": trade,
            "lines": s.note.iter().cloned().chain(item.unknown_modifiers.iter().map(|m| format!("? {}", m.text))).collect::<Vec<_>>(),
        }),
    )
}

fn preset_name(id: &str) -> &'static str {
    match id {
        "filters.preset_pseudo" => "Similar",
        "filters.preset_exact" => "Exact",
        "filters.preset_bulk" => "Bulk",
        "filters.preset_base_item" => "Base item",
        _ => "Search",
    }
}

fn ninja_price(item: &ParsedItem, db: &NinjaDb) -> Value {
    if is_valuable_basetype(item) && item.rarity != Some(ItemRarity::Unique) {
        // Equipment is priced by its mods on the trade site, not by poe.ninja.
        return json!({ "status": "Priced by its mods on the trade site" });
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
    use crate::session::Session;
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
        render(&Session {
            epoch: 1,
            item,
            place: Placement {
                screen: "DP-1".into(),
                x: 10,
                y: 20,
            },
            presets: vec![],
            active: 0,
            ninja: Some(db),
            search: SearchState::Idle,
            note: None,
        })
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
