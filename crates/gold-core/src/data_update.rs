//! Items the shipped data lacks, taken from the trade API (`/api/trade/data/items`
//! and `/api/trade/data/static`).
//!
//! The shipped data files lag behind the game, so a new item stays unknown until
//! the data is refreshed. The trade API lists every tradable item, and each trade group
//! maps onto one shape of shipped record:
//! - `card` -> `DIVINATION_CARD`, `monster` -> `CAPTURED_BEAST`, `gem` -> `GEM`;
//! - `viscera`, `jewel`, `enshrouded` -> `ITEM` with a craftable category;
//! - `currency`, `map` (fragments, scarabs...), `leaguestone`, `graft`, `wombgift`,
//!   `sanctum` -> plain `ITEM`;
//! - uniques, when their base type is already known.
//!
//! The static list adds the bulk-exchange tag (`tradeTag`, `exchangeable`) and the icon
//! to the items it knows, and a few items the items list lacks. Groups of other
//! kinds (maps, flasks, armour...) are left to the shipped data.
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

enum Shape {
    /// `ITEM` with the given craftable category, if any.
    Base(Option<&'static str>),
    /// `ITEM` whose category follows from the last word of the name.
    Enshrouded,
    Card,
    Beast,
    Gem,
}

/// Trade API group id -> how its unnamed entries are represented.
const GROUPS: [(&str, Shape); 12] = [
    ("viscera", Shape::Base(Some("Viscera"))),
    ("jewel", Shape::Base(Some("Jewel"))),
    ("currency", Shape::Base(None)),
    ("map", Shape::Base(None)),
    ("leaguestone", Shape::Base(None)),
    ("graft", Shape::Base(None)),
    ("wombgift", Shape::Base(None)),
    ("sanctum", Shape::Base(None)),
    ("enshrouded", Shape::Enshrouded),
    ("card", Shape::Card),
    ("monster", Shape::Beast),
    ("gem", Shape::Gem),
];

/// Static groups whose entries are not items one can hold (bulk-only stacks of tiers).
const STATIC_SKIP: [&str; 8] = [
    "Heist",
    "Sanctum",
    "MapKey",
    "MapsSpecial",
    "MapsUnique",
    "Misc",
    "Legacy",
    "Beasts",
];

const POE_CDN: &str = "https://web.poecdn.com";

#[derive(Deserialize)]
struct Response {
    result: Vec<Group>,
}

#[derive(Deserialize)]
struct Group {
    id: String,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    base: String,
    name: Option<String>,
    /// Variants of a unique or gem (legacy versions, alternate art); not supported.
    disc: Option<String>,
}

#[derive(Deserialize)]
struct StaticResponse {
    result: Vec<StaticGroup>,
}

#[derive(Deserialize)]
struct StaticGroup {
    id: String,
    entries: Vec<StaticEntry>,
}

#[derive(Deserialize)]
struct StaticEntry {
    id: String,
    text: String,
    image: Option<String>,
}

#[derive(Deserialize)]
struct Known {
    name: String,
    namespace: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct Extra {
    /// `items.ndjson` lines for the missing items.
    pub lines: Vec<String>,
    /// Base items, currency, cards, beasts and gems (everything but uniques).
    pub bases: usize,
    pub uniques: usize,
    /// Uniques whose base type is not in the data.
    pub skipped_uniques: usize,
    /// Entries of trade groups this module has no mapping for.
    pub unmapped: usize,
}

fn enshrouded_category(name: &str) -> Option<&'static str> {
    ["Body Armour", "Boots", "Gloves", "Helmet"]
        .into_iter()
        .find(|c| name.ends_with(c))
}

/// `trade_items` and `static_data` are the API responses, `shipped` the shipped `items.ndjson`.
pub fn extra_items(trade_items: &str, static_data: &str, shipped: &str) -> Result<Extra> {
    let response: Response = serde_json::from_str(trade_items).context("trade items response")?;
    let statics: StaticResponse = serde_json::from_str(static_data).context("trade static response")?;

    let mut known: HashSet<(String, String)> = HashSet::new();
    let mut bases = HashSet::new();
    for (n, line) in shipped.lines().enumerate() {
        let k: Known = serde_json::from_str(line).with_context(|| format!("items.ndjson line {}", n + 1))?;
        if k.namespace == "ITEM" {
            bases.insert(k.name.clone());
        }
        known.insert((k.namespace, k.name));
    }

    // name -> (group, id, image); the first group wins, as in the shipped data
    let mut by_text: HashMap<&str, (&str, &str, Option<&str>)> = HashMap::new();
    for g in &statics.result {
        for e in &g.entries {
            by_text.entry(&e.text).or_insert((&g.id, &e.id, e.image.as_deref()));
        }
    }
    let icon = |name: &str| match by_text.get(name) {
        Some((_, _, Some(image))) => format!("{POE_CDN}{image}"),
        _ => String::new(),
    };

    let mut out = Extra::default();
    let mut added: HashSet<(&'static str, String)> = HashSet::new();
    let mut push = |out: &mut Extra, namespace: &'static str, record: Value| {
        let name = record["name"].as_str().unwrap_or_default().to_string();
        if known.contains(&(namespace.to_string(), name.clone())) || !added.insert((namespace, name)) {
            return false;
        }
        out.lines.push(record.to_string());
        true
    };

    for group in &response.result {
        let shape = GROUPS.iter().find(|(id, _)| *id == group.id).map(|(_, s)| s);
        for e in &group.entries {
            if e.disc.is_some() {
                continue;
            }
            if let Some(name) = &e.name {
                if known.contains(&("UNIQUE".to_string(), name.clone())) {
                    continue;
                }
                if !bases.contains(&e.base) {
                    out.skipped_uniques += 1;
                    continue;
                }
                let record = json!({
                    "name": name,
                    "refName": name,
                    "namespace": "UNIQUE",
                    "unique": { "base": e.base },
                    "icon": "",
                });
                if push(&mut out, "UNIQUE", record) {
                    out.uniques += 1;
                }
                continue;
            }
            let name = &e.base;
            let stat = by_text.get(name.as_str());
            let record = match shape {
                None => {
                    if !bases.contains(name) {
                        out.unmapped += 1;
                    }
                    continue;
                }
                Some(Shape::Base(category)) => {
                    let mut r = json!({"name": name, "refName": name, "namespace": "ITEM", "icon": icon(name)});
                    if let Some(c) = category {
                        r["craftable"] = json!({ "category": c });
                    }
                    ("ITEM", r)
                }
                Some(Shape::Enshrouded) => {
                    let mut r = json!({"name": name, "refName": name, "namespace": "ITEM", "icon": icon(name)});
                    if let Some(c) = enshrouded_category(name) {
                        r["craftable"] = json!({ "category": c });
                    }
                    ("ITEM", r)
                }
                Some(Shape::Card) => (
                    "DIVINATION_CARD",
                    json!({"name": name, "refName": name, "namespace": "DIVINATION_CARD", "icon": icon(name)}),
                ),
                Some(Shape::Beast) => (
                    "CAPTURED_BEAST",
                    json!({"name": name, "refName": name, "namespace": "CAPTURED_BEAST", "icon": icon(name)}),
                ),
                Some(Shape::Gem) => (
                    "GEM",
                    json!({"name": name, "refName": name, "namespace": "GEM", "icon": icon(name), "gem": {"maxLevel": 20}}),
                ),
            };
            let (namespace, mut record) = record;
            if namespace == "DIVINATION_CARD" && stat.is_some() {
                record["exchangeable"] = json!(true);
            }
            // currency-like items trade in bulk under the static list's id
            if namespace == "ITEM" {
                if let Some((_, id, _)) = stat {
                    record["tradeTag"] = json!(id);
                    record["exchangeable"] = json!(true);
                }
            }
            if push(&mut out, namespace, record) {
                out.bases += 1;
            }
        }
    }

    // items only the static list knows (new fragments, cards)
    let listed: HashSet<&str> = response
        .result
        .iter()
        .flat_map(|g| &g.entries)
        .map(|e| e.base.as_str())
        .collect();
    for g in &statics.result {
        if STATIC_SKIP.contains(&g.id.as_str()) {
            continue;
        }
        for e in &g.entries {
            if e.text.is_empty() || listed.contains(e.text.as_str()) {
                continue;
            }
            let image = e.image.as_deref().map(|i| format!("{POE_CDN}{i}")).unwrap_or_default();
            let (namespace, record) = if g.id == "Cards" {
                (
                    "DIVINATION_CARD",
                    json!({"name": e.text, "refName": e.text, "namespace": "DIVINATION_CARD", "icon": image, "exchangeable": true}),
                )
            } else {
                (
                    "ITEM",
                    json!({"name": e.text, "refName": e.text, "namespace": "ITEM", "icon": image, "tradeTag": e.id, "exchangeable": true}),
                )
            };
            if push(&mut out, namespace, record) {
                out.bases += 1;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: &str = concat!(
        r#"{"name":"Coral Ring","refName":"Coral Ring","namespace":"ITEM","icon":""}"#,
        "\n",
        r#"{"name":"Bloodboil","refName":"Bloodboil","namespace":"UNIQUE","unique":{"base":"Coral Ring"},"icon":""}"#,
        "\n",
    );
    const STATIC: &str = r#"{"result":[
        {"id":"Currency","entries":[{"id":"omen-res","text":"Omen of Resurgence","image":"/gen/image/x/Omen.png"}]},
        {"id":"Cards","entries":[{"id":"the-cheater","text":"The Cheater","image":"/gen/image/x/Card.png"},{"id":"new-card","text":"New Card","image":"/gen/image/x/Card.png"}]},
        {"id":"Heist","entries":[{"id":"contract","text":"Lockpicking Contract (Level 68-72)"}]}
    ]}"#;

    fn api(entries: &str, group: &str) -> String {
        format!(r#"{{"result":[{{"id":"{group}","label":"x","entries":[{entries}]}}]}}"#)
    }

    fn extra(entries: &str, group: &str) -> Extra {
        extra_items(&api(entries, group), r#"{"result":[]}"#, SHIPPED).unwrap()
    }

    fn records(extra: &Extra) -> Vec<Value> {
        extra.lines.iter().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    #[test]
    fn adds_missing_viscera_bases_once() {
        let extra = extra(r#"{"type":"Primate Foot"},{"type":"Primate Foot"}"#, "viscera");
        assert_eq!(extra.bases, 1);
        let v = &records(&extra)[0];
        assert_eq!(v["name"], "Primate Foot");
        assert_eq!(v["namespace"], "ITEM");
        assert_eq!(v["craftable"]["category"], "Viscera");
    }

    #[test]
    fn skips_known_items_and_unmapped_groups() {
        assert!(extra(r#"{"type":"Coral Ring"}"#, "viscera").lines.is_empty());
        let other = extra(r#"{"type":"Mystery"}"#, "newgroup");
        assert!(other.lines.is_empty());
        assert_eq!(other.unmapped, 1);
    }

    #[test]
    fn currency_takes_tag_and_icon_from_the_static_list() {
        let json = api(r#"{"type":"Omen of Resurgence"},{"type":"The Cheater"}"#, "currency");
        let e = extra_items(&json, STATIC, SHIPPED).unwrap();
        let r = records(&e);
        assert_eq!(r[0]["tradeTag"], "omen-res");
        assert_eq!(r[0]["exchangeable"], true);
        assert_eq!(r[0]["icon"], "https://web.poecdn.com/gen/image/x/Omen.png");
        assert!(r[0].get("craftable").is_none());
    }

    #[test]
    fn cards_beasts_and_gems_use_their_namespace() {
        let json = r#"{"result":[
            {"id":"card","entries":[{"type":"The Cheater"},{"type":"Unlisted Card"}]},
            {"id":"monster","entries":[{"type":"Junglemare"}]},
            {"id":"gem","entries":[{"type":"Quickstep"},{"type":"Quickstep of X","disc":"alt_x"}]}
        ]}"#;
        let e = extra_items(json, STATIC, SHIPPED).unwrap();
        let r = records(&e);
        let find = |n: &str| r.iter().find(|v| v["name"] == n).unwrap();
        assert_eq!(find("The Cheater")["namespace"], "DIVINATION_CARD");
        assert_eq!(find("The Cheater")["exchangeable"], true);
        assert!(find("Unlisted Card").get("exchangeable").is_none());
        assert_eq!(find("Junglemare")["namespace"], "CAPTURED_BEAST");
        assert_eq!(find("Quickstep")["gem"]["maxLevel"], 20);
        assert!(r.iter().all(|v| v["name"] != "Quickstep of X"));
    }

    #[test]
    fn static_only_items_are_added_but_not_bulk_tiers() {
        let e = extra_items(r#"{"result":[]}"#, STATIC, SHIPPED).unwrap();
        let names: Vec<String> = records(&e)
            .iter()
            .map(|v| v["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, ["Omen of Resurgence", "The Cheater", "New Card"]);
    }

    #[test]
    fn enshrouded_gear_gets_a_category() {
        let e = extra(r#"{"type":"Enshrouded Boots"}"#, "enshrouded");
        assert_eq!(records(&e)[0]["craftable"]["category"], "Boots");
    }

    #[test]
    fn adds_uniques_with_a_known_base() {
        let json = concat!(
            r#"{"type":"Coral Ring","name":"Angler's Plait"},"#,
            r#"{"type":"Coral Ring","name":"Bloodboil"},"#,
            r#"{"type":"Coral Ring","name":"Old Thing","disc":"legacy"},"#,
            r#"{"type":"Unheard Ring","name":"Lost"}"#,
        );
        let extra = extra(json, "accessory");
        assert_eq!((extra.uniques, extra.skipped_uniques), (1, 1));
        let v = &records(&extra)[0];
        assert_eq!(v["name"], "Angler's Plait");
        assert_eq!(v["unique"]["base"], "Coral Ring");
    }

    #[test]
    fn extra_lines_load_into_the_item_db() {
        let extra = extra(r#"{"type":"Primate Foot"}"#, "viscera");
        let db = crate::data::ItemDb::parse(&format!("{SHIPPED}{}\n", extra.lines.join("\n"))).unwrap();
        let found = db.by_translated("ITEM", "Primate Foot").unwrap();
        assert_eq!(found[0].craftable.as_ref().unwrap().category.known(), None);
    }
}
