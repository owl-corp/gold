//! What the overlay shows. The QML renders this JSON as it is and keeps no state.
use crate::session::{Row, SearchState, Session};
use gold_core::parser::item::{ItemRarity, ParsedItem};
use gold_core::price_check::details::{get_details_id, is_valuable_basetype};
use gold_core::price_check::filters::util::round_roll;
use gold_core::price_check::ninja::{auto_currency, delta_from_graph, display_rounding, Currency, NinjaDb};
use gold_core::price_check::related;
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
                // hidden filters that are switched on still shape the search, so they stay visible
                .filter(|(_, f)| s.show_hidden || f.hidden.is_none() || !f.disabled)
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

    let hidden_count = s
        .preset()
        .map(|p| p.stats.iter().filter(|f| f.hidden.is_some() && f.disabled).count())
        .unwrap_or(0);
    let trade_filters = s.preset().map(|p| {
        let t = &p.filters.trade;
        let magic_only = p.filters.rarity.as_ref().filter(|r| r.value == "magic").map(|r| !r.disabled);
        json!({ "offline": t.offline, "merchantOnly": t.merchant_only, "listed": t.listed, "currency": t.currency, "magicOnly": magic_only })
    });

    // the quality minimum the search uses (gems search below their own quality)
    let quality = s
        .preset()
        .and_then(|p| p.filters.quality.as_ref())
        .filter(|q| !q.disabled)
        .map(|q| q.value);

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
                "rows": grouped_rows(&g.rows),
                "more": g.fetch_id.is_some() && !g.pending.is_empty(),
                "fetchingMore": s.fetching_more,
            })).collect::<Vec<_>>(),
        }),
    };

    with_placement(
        &s.place,
        json!({
            "epoch": s.epoch,
            "title": title,
            "subtitle": subtitle,
            "rarity": rarity_name(item),
            "price": price,
            "presets": presets,
            "filters": filters,
            "hiddenCount": hidden_count,
            "showHidden": s.show_hidden,
            "tradeFilters": trade_filters,
            "searchQuality": quality,
            "trade": trade,
            "lines": s.note.iter().cloned().chain(item.unknown_modifiers.iter().map(|m| format!("? {}", m.text))).collect::<Vec<_>>(),
        }),
    )
}

/// Listings from the same seller at the same price are one row with a count.
fn grouped_rows(rows: &[Row]) -> Vec<Value> {
    let mut groups: Vec<(&Row, u32)> = Vec::new();
    for r in rows {
        match groups
            .iter_mut()
            .find(|(g, _)| g.who == r.who && g.price == r.price && g.status == r.status)
        {
            Some((_, n)) => *n += 1,
            None => groups.push((r, 1)),
        }
    }
    groups
        .into_iter()
        .map(|(r, count)| json!({ "price": r.price, "detail": r.detail, "status": r.status, "who": r.who, "mine": r.mine, "count": count }))
        .collect()
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

/// Most valuable first; entries without a price go last. Stable, so ties keep their order.
fn sort_by_price(rows: &mut [Value]) {
    let chaos = |r: &Value| r["chaos"].as_f64().unwrap_or(f64::NEG_INFINITY);
    rows.sort_by(|a, b| chaos(b).total_cmp(&chaos(a)));
}

/// Sections of poe.ninja with at most this many entries are shown whole.
const SMALL_CATEGORY: usize = 10;

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
    let fmt = |chaos: f64, is_divine: bool| {
        let v = auto_currency(chaos, if is_divine { None } else { rate });
        match v.currency {
            Currency::Chaos if is_divine => (display_rounding(chaos, false), "c"),
            // under 0.1c rounding shows 0 or a coarse 0.1: say how many a chaos buys
            Currency::Chaos if v.min > 0.0 && v.min < 0.1 => (display_rounding(1.0 / v.min, false), "per c"),
            Currency::Chaos => (display_rounding(v.min, false), "c"),
            Currency::Div => (display_rounding(v.min, false), "div"),
        }
    };
    let (amount, unit) = fmt(found.info.chaos, item.info.ref_name == "Divine Orb");
    let mut price = json!({ "amount": amount, "unit": unit, "url": found.url });
    if id.ns == "ITEM" && id.name.ends_with(" Thread") {
        if let Some(avg) = db.average_chaos_by_suffix("ITEM", " Thread") {
            let v = auto_currency(avg, rate);
            let (n, u) = match v.currency {
                Currency::Chaos => (display_rounding(v.min, false), "c"),
                Currency::Div => (display_rounding(v.min, false), "div"),
            };
            price["average"] = json!(format!("all threads avg {n} {u}"));
        }
    }
    // related items, or else all of a small category, not only this item
    let row = |name: String, p: Option<(f64, String)>, current: bool| match p {
        Some((chaos, url)) => {
            let (amount, unit) = fmt(chaos, name == "Divine Orb");
            json!({ "name": name, "chaos": chaos, "amount": amount, "unit": unit, "url": url, "current": current })
        }
        None => json!({ "name": name, "current": current }),
    };
    let curated = related::find_group(&id.ns, &id.name, id.variant.as_deref()).map(|g| {
        let rows = |ids: &[String], heading: &str| {
            let keys: Vec<_> = ids.iter().filter_map(|i| related::parse_key(i)).collect();
            let mut rows: Vec<Value> = keys
                .iter()
                .map(|k| {
                    // the variant tells apart entries that share a name
                    let dup = keys.iter().filter(|o| o.name == k.name).count() > 1;
                    let name = match k.variant {
                        Some(v) if dup => format!("{} ({v})", k.name),
                        _ => k.name.to_string(),
                    };
                    let p = db.find(k.ns, k.name, k.variant).map(|p| (p.info.chaos, p.url));
                    let current = k.ns == id.ns && k.name == id.name && k.variant == id.variant.as_deref();
                    row(name, p, current)
                })
                .collect();
            sort_by_price(&mut rows);
            (!rows.is_empty()).then(|| json!({ "heading": heading, "rows": rows }))
        };
        [rows(&g.query, "Related"), rows(&g.items, "Drops / used for")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
    });
    let groups = curated.filter(|g| !g.is_empty()).or_else(|| {
        let all = db.section_prices(&id.ns, &id.name, id.variant.as_deref())?;
        (all.len() > 1 && all.len() <= SMALL_CATEGORY).then(|| {
            let mut rows: Vec<Value> = all
                .iter()
                .map(|p| {
                    let name = match p.info.variant.as_deref() {
                        Some(v) if !v.is_empty() => format!("{}, {v}", p.info.name),
                        _ => p.info.name.clone(),
                    };
                    let current = p.info.name == found.info.name && p.info.variant == found.info.variant;
                    row(name, Some((p.info.chaos, p.url.clone())), current)
                })
                .collect();
            sort_by_price(&mut rows);
            vec![json!({ "rows": rows })]
        })
    });
    if let Some(groups) = groups {
        price["groups"] = json!(groups);
    }
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
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
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
            search_gen: 0,
            note: None,
            show_hidden: false,
            fetching_more: false,
        })
    }

    #[test]
    fn small_ninja_category_lists_every_entry() {
        let v = view_of("Item Class: Stackable Currency\nRarity: Currency\nDivine Orb\n--------\nStack Size: 3/10\n");
        let groups = v["price"]["groups"].as_array().unwrap();
        let all = groups[0]["rows"].as_array().unwrap();
        assert_eq!(all.len(), 2);
        // most valuable first
        assert_eq!(all[0]["name"], "Divine Orb");
        assert_eq!(all[0]["current"], true);
        assert_eq!(all[1]["name"], "Chaos Orb");
    }

    #[test]
    fn filter_rows_keep_their_index_and_hide_hidden_disabled_filters() {
        use gold_core::price_check::filters::presets::{create_presets, PresetOptions};
        let data = GameData::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            "en",
        )
        .unwrap();
        let text = "Item Class: Belts\nRarity: Rare\nGhoul Twine\nStudded Belt\n--------\nItem Level: 39\n--------\n\
                    { Prefix Modifier \"Rotund\" (Tier: 4) \u{2014} Life }\n+86(85-99) to maximum Life\n\
                    { Suffix Modifier \"of the Salamander\" (Tier: 7) \u{2014} Elemental, Fire, Resistance }\n+16(12-17)% to Fire Resistance\n";
        let item = parse_clipboard(&data, text).unwrap();
        let opts = PresetOptions {
            league: "Covens".into(),
            merchant_only: false,
            currency: None,
            collapse_listings: "api",
            activate_stock_filter: false,
        };
        let presets = create_presets(&data, &item, &opts).unwrap().presets;
        let stats = presets[0].stats.clone();
        let v = render(&Session {
            epoch: 7,
            item,
            place: Placement {
                screen: "DP-1".into(),
                x: 0,
                y: 0,
            },
            presets,
            active: 0,
            ninja: None,
            search: SearchState::Idle,
            search_gen: 0,
            note: None,
            show_hidden: false,
            fetching_more: false,
        });

        assert_eq!(v["epoch"], 7);
        let rows = v["filters"].as_array().unwrap();
        // every row's `index` points at the same filter in the full list the daemon edits
        for row in rows {
            let f = &stats[row["index"].as_u64().unwrap() as usize];
            assert!(
                f.hidden.is_none() || !f.disabled,
                "hidden and disabled filters must not be shown"
            );
        }
        let life = rows
            .iter()
            .find(|r| r["text"].as_str().unwrap().contains("total maximum Life"))
            .unwrap();
        assert_eq!(life["on"], true);
        assert_eq!(life["editable"], true);
    }

    #[test]
    fn same_seller_and_price_collapse_into_one_counted_row() {
        let row = |who: &str, price: &str| Row {
            price: price.into(),
            detail: String::new(),
            status: "online",
            who: who.into(),
            mine: false,
        };
        let rows = [
            row("a", "1 divine"),
            row("b", "1 divine"),
            row("a", "1 divine"),
            row("a", "2 divine"),
        ];
        let g = grouped_rows(&rows);
        let counts: Vec<_> = g
            .iter()
            .map(|r| (r["who"].as_str().unwrap(), r["count"].as_u64().unwrap()))
            .collect();
        assert_eq!(counts, [("a", 2), ("b", 1), ("a", 1)]);
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
