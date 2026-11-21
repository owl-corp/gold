//! Items and stats the shipped data lacks, taken from the trade API (`/api/trade/data/items`
//! and `/api/trade/data/static`).
//!
//! The shipped data files lag behind the game, so a new item stays unknown until
//! the data is refreshed. The trade API lists every tradable item, and each trade group
//! maps onto one shape of shipped record:
//! - `card` -> `DIVINATION_CARD`, `monster` -> `CAPTURED_BEAST`, `gem` -> `GEM`;
//! - `viscera`, `jewel`, `enshrouded` -> `ITEM` with a craftable category;
//! - `currency`, `map` (fragments...), `leaguestone`, `graft`, `wombgift`,
//!   `sanctum` -> plain `ITEM`;
//! - uniques, when their base type is already known.
//!
//! The static list adds the bulk-exchange tag (`tradeTag`, `exchangeable`) and the icon
//! to the items it knows, and a few items the items list lacks. Groups of other
//! kinds (maps, flasks, armour...) are left to the shipped data.
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

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

#[derive(Debug, Default, PartialEq, Eq)]
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

/// Sets a field on a JSON object record (appended last when new, like indexing would).
fn set(record: &mut Value, key: &str, value: Value) {
    if let Some(o) = record.as_object_mut() {
        o.insert(key.to_string(), value);
    }
}

/// The record a trade entry of the given group shape becomes, with its namespace.
fn shape_record(shape: &Shape, name: &str, icon: &str) -> (&'static str, Value) {
    match shape {
        Shape::Base(category) => {
            let mut r = json!({"name": name, "refName": name, "namespace": "ITEM", "icon": icon});
            if let Some(c) = category {
                set(&mut r, "craftable", json!({ "category": c }));
            }
            ("ITEM", r)
        }
        Shape::Enshrouded => {
            let mut r = json!({"name": name, "refName": name, "namespace": "ITEM", "icon": icon});
            if let Some(c) = enshrouded_category(name) {
                set(&mut r, "craftable", json!({ "category": c }));
            }
            ("ITEM", r)
        }
        Shape::Card => (
            "DIVINATION_CARD",
            json!({"name": name, "refName": name, "namespace": "DIVINATION_CARD", "icon": icon}),
        ),
        Shape::Beast => (
            "CAPTURED_BEAST",
            json!({"name": name, "refName": name, "namespace": "CAPTURED_BEAST", "icon": icon}),
        ),
        Shape::Gem => (
            "GEM",
            json!({"name": name, "refName": name, "namespace": "GEM", "icon": icon, "gem": {"maxLevel": 20}}),
        ),
    }
}

/// `(namespace, name)` pairs and base type names.
type KnownItems = (HashSet<(String, String)>, HashSet<String>);

/// Like the shipped data, but the base type names also include the ones this update adds.
fn known_and_bases(response: &Response, shipped: &str) -> Result<KnownItems> {
    let mut known: HashSet<(String, String)> = HashSet::new();
    let mut bases = HashSet::new();
    for (n, line) in shipped.lines().enumerate() {
        let k: Known = serde_json::from_str(line).with_context(|| format!("items.ndjson line {}", n + 1))?;
        if k.namespace == "ITEM" {
            bases.insert(k.name.clone());
        }
        known.insert((k.namespace, k.name));
    }

    // bases this update adds count too, or a unique on a brand-new base would be skipped
    for g in response
        .result
        .iter()
        .filter(|g| GROUPS.iter().any(|(id, _)| *id == g.id))
    {
        for e in g.entries.iter().filter(|e| e.name.is_none() && e.disc.is_none()) {
            bases.insert(e.base.clone());
        }
    }
    Ok((known, bases))
}

/// name -> (group, id, image); the first group wins, as in the shipped data
fn static_by_text(statics: &StaticResponse) -> HashMap<&str, (&str, &str, Option<&str>)> {
    let mut by_text = HashMap::new();
    for g in &statics.result {
        for e in &g.entries {
            by_text
                .entry(e.text.as_str())
                .or_insert((g.id.as_str(), e.id.as_str(), e.image.as_deref()));
        }
    }
    by_text
}

/// Cards and currency-like items the static list knows trade in bulk.
fn mark_exchangeable(namespace: &str, record: &mut Value, static_id: Option<&str>) {
    if namespace == "DIVINATION_CARD" && static_id.is_some() {
        set(record, "exchangeable", json!(true));
    }
    // currency-like items trade in bulk under the static list's id
    if namespace == "ITEM" {
        if let Some(id) = static_id {
            set(record, "tradeTag", json!(id));
            set(record, "exchangeable", json!(true));
        }
    }
}

/// `trade_items` and `static_data` are the API responses, `shipped` the shipped `items.ndjson`.
pub fn extra_items(trade_items: &str, static_data: &str, shipped: &str) -> Result<Extra> {
    let response: Response = serde_json::from_str(trade_items).context("trade items response")?;
    let statics: StaticResponse = serde_json::from_str(static_data).context("trade static response")?;

    let (known, bases) = known_and_bases(&response, shipped)?;

    let by_text = static_by_text(&statics);
    let icon = |name: &str| match by_text.get(name) {
        Some((_, _, Some(image))) => format!("{POE_CDN}{image}"),
        _ => String::new(),
    };

    let mut out = Extra::default();
    let mut added: HashSet<(&'static str, String)> = HashSet::new();
    let mut push = |out: &mut Extra, namespace: &'static str, record: Value| {
        let name = record
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
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
            let Some(shape) = shape else {
                if !bases.contains(name) {
                    out.unmapped += 1;
                }
                continue;
            };
            let (namespace, mut record) = shape_record(shape, name, &icon(name));
            mark_exchangeable(namespace, &mut record, stat.map(|(_, id, _)| *id));
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

// ---- stats ----

#[derive(Deserialize)]
struct StatsResponse {
    result: Vec<StatGroup>,
}

#[derive(Deserialize)]
struct StatGroup {
    id: String,
    entries: Vec<StatEntry>,
}

#[derive(Deserialize)]
struct StatEntry {
    id: String,
    text: String,
    option: Option<Value>,
}

#[derive(Deserialize)]
struct ShippedStat {
    #[serde(rename = "ref")]
    ref_: String,
    #[serde(default)]
    matchers: Vec<ShippedMatcher>,
    #[serde(default)]
    trade: ShippedTrade,
}

#[derive(Deserialize)]
struct ShippedMatcher {
    string: String,
    advanced: Option<String>,
}

#[derive(Default, Deserialize)]
struct ShippedTrade {
    #[serde(default)]
    ids: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ShippedLine {
    Group { stats: Vec<ShippedStat> },
    Stat(ShippedStat),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExtraStats {
    /// `stats.extra.ndjson` lines: whole stats and `{"patch": ...}` lines.
    pub lines: Vec<String>,
    /// New stats.
    pub added: usize,
    /// Shipped stats that got trade ids for a modifier type they lacked.
    pub patched: usize,
    /// Stats left out: the text matches a stat of a resolver group, several ids share
    /// one text, or a matcher collides with a shipped one.
    pub skipped: usize,
}

/// Trade stat types that are not read from item text (`pseudo` is built by hand, the
/// crucible tree is retired).
const STAT_SKIP: [&str; 2] = ["pseudo", "crucible"];

/// ref -> (stats with that ref, whether it is one plain stat) and its trade id types
type RefInfo = (usize, bool, BTreeMap<String, Vec<String>>);

/// The line a stat has on an item: no "(Local)" suffix.
fn stat_text(text: &str) -> &str {
    text.strip_suffix(" (Local)").unwrap_or(text)
}

/// Replaces `+#` and `-#` by `#`: rolls take their sign into the number, which is
/// why the matchers of shipped stats have none.
fn matcher_text(text: &str) -> String {
    text.replace("+#", "#").replace("-#", "#")
}

/// Wordings of the same stat with a negative roll, as in the shipped matchers.
const NEGATED: [(&str, &str); 3] = [
    (" increased ", " reduced "),
    (" more ", " less "),
    (" faster", " slower"),
];

/// `trade_stats` is the `/api/trade/data/stats` response, `shipped` the shipped `stats.ndjson`.
pub fn extra_stats(trade_stats: &str, shipped: &str) -> Result<ExtraStats> {
    let response: StatsResponse = serde_json::from_str(trade_stats).context("trade stats response")?;

    let mut have_ids: HashSet<String> = HashSet::new();
    let mut matchers: HashSet<String> = HashSet::new();
    let mut refs: HashMap<String, RefInfo> = HashMap::new();
    for (n, line) in shipped.lines().enumerate() {
        let line: ShippedLine = serde_json::from_str(line).with_context(|| format!("stats.ndjson line {}", n + 1))?;
        let (stats, plain) = match line {
            ShippedLine::Group { stats } => (stats, false),
            ShippedLine::Stat(s) => (vec![s], true),
        };
        for s in stats {
            for m in &s.matchers {
                matchers.insert(m.string.clone());
                matchers.extend(m.advanced.clone());
            }
            have_ids.extend(s.trade.ids.values().flatten().cloned());
            let entry = refs.entry(s.ref_).or_insert_with(|| (0, plain, s.trade.ids.clone()));
            entry.0 += 1;
        }
    }

    // text -> modifier type -> missing ids; texts in first-seen order
    let mut order: Vec<&str> = Vec::new();
    let mut missing: HashMap<&str, BTreeMap<&str, Vec<&str>>> = HashMap::new();
    for g in response.result.iter().filter(|g| !STAT_SKIP.contains(&g.id.as_str())) {
        for e in g
            .entries
            .iter()
            .filter(|e| e.option.is_none() && !have_ids.contains(&e.id))
        {
            let text = stat_text(&e.text);
            if !missing.contains_key(text) {
                order.push(text);
            }
            missing.entry(text).or_default().entry(&g.id).or_default().push(&e.id);
        }
    }

    let mut out = ExtraStats::default();
    for text in order {
        let Some(by_type) = missing.get(text) else { continue };
        if by_type.values().any(|ids| ids.len() > 1) || text == "#" {
            out.skipped += 1;
            continue;
        }
        let ids: BTreeMap<&str, Vec<&str>> = by_type.clone();
        if let Some((count, plain, shipped_ids)) = refs.get(text) {
            let ids: BTreeMap<&str, &Vec<&str>> = ids
                .iter()
                .filter(|(k, _)| !shipped_ids.contains_key(**k))
                .map(|(k, v)| (*k, v))
                .collect();
            if *count == 1 && *plain && !ids.is_empty() {
                out.lines.push(json!({"patch": text, "ids": ids}).to_string());
                out.patched += 1;
            } else {
                out.skipped += 1;
            }
            continue;
        }
        let plain = matcher_text(text);
        if matchers.contains(&plain) {
            out.skipped += 1;
            continue;
        }
        let mut list = vec![json!({"string": plain})];
        if let Some((from, to)) = NEGATED.iter().find(|(from, _)| plain.contains(from)) {
            let negated = plain.replacen(from, to, 1);
            if matchers.insert(negated.clone()) {
                list.push(json!({"string": negated, "negate": true}));
            }
        }
        matchers.insert(plain);
        let better = i32::from(text.starts_with("+#") || text.contains(" increased ") || text.contains(" more "));
        out.lines
            .push(json!({"ref": text, "better": better, "matchers": list, "trade": {"ids": ids}}).to_string());
        out.added += 1;
    }
    Ok(out)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, reason = "tests")]
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
        assert_eq!(
            extra(r#"{"type":"Coral Ring"}"#, "viscera").lines,
            [] as [std::string::String; 0]
        );
        let other = extra(r#"{"type":"Mystery"}"#, "newgroup");
        assert_eq!(other.lines, [] as [std::string::String; 0]);
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
    fn adds_uniques_on_a_base_added_by_the_same_update() {
        let json = r#"{"type":"New Jewel"},{"type":"New Jewel","name":"Fresh Unique"}"#;
        let extra = extra(json, "jewel");
        assert_eq!((extra.uniques, extra.skipped_uniques), (1, 0));
    }

    #[test]
    fn extra_lines_load_into_the_item_db() {
        let extra = extra(r#"{"type":"Primate Foot"}"#, "viscera");
        let db = crate::data::ItemDb::parse(&format!("{SHIPPED}{}\n", extra.lines.join("\n"))).unwrap();
        let found = db.by_translated("ITEM", "Primate Foot").unwrap();
        assert_eq!(
            found[0].craftable.as_ref().unwrap().category.known(),
            Some(crate::parser::meta::ItemCategory::Viscera)
        );
    }

    const SHIPPED_STATS: &str = concat!(
        r##"{"ref":"+# to Strength","better":1,"matchers":[{"string":"# to Strength"}],"trade":{"ids":{"explicit":["explicit.stat_1"]}}}"##,
        "\n",
        r##"{"resolve":{"strat":"trivial-merge"},"stats":[{"ref":"Grouped","better":1,"matchers":[{"string":"Grouped"}],"trade":{"ids":{"explicit":["explicit.stat_2"]}}}]}"##,
        "\n",
    );

    fn stats_api(entries: &str) -> String {
        format!(
            r#"{{"result":[{{"id":"implicit","entries":[{entries}]}},{{"id":"crucible","entries":[{{"id":"crucible.stat_9","text":"Tree","type":"crucible"}}]}}]}}"#
        )
    }

    fn stat_lines(entries: &str) -> (ExtraStats, Vec<Value>) {
        let e = extra_stats(&stats_api(entries), SHIPPED_STATS).unwrap();
        let lines = e.lines.iter().map(|l| serde_json::from_str(l).unwrap()).collect();
        (e, lines)
    }

    #[test]
    fn new_stats_get_sign_free_and_negated_matchers() {
        let (e, lines) = stat_lines(
            r##"{"id":"implicit.stat_5","text":"+#% to Foo","type":"implicit"},{"id":"implicit.stat_6","text":"#% increased Bar","type":"implicit"}"##,
        );
        assert_eq!(e.added, 2);
        assert_eq!(lines[0]["matchers"][0]["string"], "#% to Foo");
        assert_eq!(lines[0]["better"], 1);
        assert_eq!(lines[1]["matchers"][1]["string"], "#% reduced Bar");
        assert_eq!(lines[1]["matchers"][1]["negate"], true);
        assert_eq!(lines[1]["trade"]["ids"]["implicit"][0], "implicit.stat_6");
    }

    #[test]
    fn known_stats_get_patched_with_missing_types_only() {
        let (e, lines) = stat_lines(
            r#"{"id":"implicit.stat_7","text":"+# to Strength","type":"implicit"},{"id":"implicit.stat_8","text":"Grouped","type":"implicit"}"#,
        );
        assert_eq!((e.added, e.patched, e.skipped), (0, 1, 1));
        assert_eq!(lines[0]["patch"], "+# to Strength");
        assert_eq!(lines[0]["ids"]["implicit"][0], "implicit.stat_7");
    }

    #[test]
    fn ambiguous_and_known_ids_are_skipped() {
        let (e, _) = stat_lines(
            r#"{"id":"explicit.stat_1","text":"+# to Strength","type":"implicit"},{"id":"implicit.stat_3","text":"Twin","type":"implicit"},{"id":"implicit.stat_4","text":"Twin (Local)","type":"implicit"}"#,
        );
        assert_eq!((e.added, e.patched, e.skipped), (0, 0, 1));
    }

    #[test]
    fn extra_stats_load_into_the_stat_db() {
        let (_, lines) = stat_lines(
            r#"{"id":"implicit.stat_5","text":"+#% to Foo","type":"implicit"},{"id":"implicit.stat_7","text":"+# to Strength","type":"implicit"}"#,
        );
        let db = crate::data::StatDb::parse_with_extra(
            SHIPPED_STATS,
            &lines
                .iter()
                .flat_map(|l| [l.to_string(), "\n".to_string()])
                .collect::<String>(),
        )
        .unwrap();
        assert!(db.by_match_str("#% to Foo").is_some());
        let crate::data::StatOrGroup::Stat(strength) = db.by_ref("+# to Strength").unwrap() else {
            panic!()
        };
        assert_eq!(strength.trade.ids["implicit"][0], "implicit.stat_7");
        assert_eq!(strength.trade.ids["explicit"][0], "explicit.stat_1");
    }
}
