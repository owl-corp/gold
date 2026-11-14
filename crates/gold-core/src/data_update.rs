//! Items the shipped data lacks, taken from the trade API (`/api/trade/data/items`).
//!
//! The shipped data files lag behind the game, so a new item type stays unknown
//! until the data is refreshed. The trade API
//! lists every tradable item at once, which is enough for two cases:
//! - groups whose whole content is one item category (`viscera`);
//! - uniques, when their base type is already known.
//!
//! The entries get no icon and no game-file data. Everything else in the API list
//! (maps, currency, monsters...) is represented differently in the shipped data, so it
//! is left alone.
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;

/// Trade API group id -> item category of every base type in the group.
const GROUP_CATEGORY: [(&str, &str); 1] = [("viscera", "Viscera")];

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
    /// Variants of a unique (legacy versions, alternate art); not supported.
    disc: Option<String>,
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
    pub bases: usize,
    pub uniques: usize,
    /// Uniques whose base type is not in the data.
    pub skipped_uniques: usize,
}

/// `trade_items` is the API response, `shipped` the shipped `items.ndjson`.
pub fn extra_items(trade_items: &str, shipped: &str) -> Result<Extra> {
    let response: Response = serde_json::from_str(trade_items).context("trade items response")?;

    let mut bases = HashSet::new();
    let mut uniques = HashSet::new();
    for (n, line) in shipped.lines().enumerate() {
        let k: Known = serde_json::from_str(line).with_context(|| format!("items.ndjson line {}", n + 1))?;
        match k.namespace.as_str() {
            "ITEM" => bases.insert(k.name),
            "UNIQUE" => uniques.insert(k.name),
            _ => false,
        };
    }

    let mut out = Extra::default();
    let mut added = HashSet::new();
    for group in &response.result {
        let category = GROUP_CATEGORY.iter().find(|(id, _)| *id == group.id).map(|(_, c)| *c);
        for e in &group.entries {
            match (&e.name, category) {
                (Some(_), _) if e.disc.is_some() => {}
                (Some(name), _) => {
                    if uniques.contains(name) || !added.insert(format!("U::{name}::{}", e.base)) {
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
                    out.lines.push(record.to_string());
                    out.uniques += 1;
                }
                (None, Some(category)) => {
                    if bases.contains(&e.base) || !added.insert(format!("B::{}", e.base)) {
                        continue;
                    }
                    let record = json!({
                        "name": e.base,
                        "refName": e.base,
                        "namespace": "ITEM",
                        "icon": "",
                        "craftable": { "category": category },
                    });
                    out.lines.push(record.to_string());
                    out.bases += 1;
                }
                (None, None) => {}
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

    fn api(entries: &str, group: &str) -> String {
        format!(r#"{{"result":[{{"id":"{group}","label":"x","entries":[{entries}]}}]}}"#)
    }

    #[test]
    fn adds_missing_viscera_bases_once() {
        let json = api(r#"{"type":"Primate Foot"},{"type":"Primate Foot"}"#, "viscera");
        let extra = extra_items(&json, SHIPPED).unwrap();
        assert_eq!(extra.bases, 1);
        let v: serde_json::Value = serde_json::from_str(&extra.lines[0]).unwrap();
        assert_eq!(v["name"], "Primate Foot");
        assert_eq!(v["namespace"], "ITEM");
        assert_eq!(v["craftable"]["category"], "Viscera");
    }

    #[test]
    fn ignores_bases_of_other_groups_and_known_items() {
        let other = api(r#"{"type":"Omen of Resurgence"}"#, "currency");
        assert!(extra_items(&other, SHIPPED).unwrap().lines.is_empty());
        let known = api(r#"{"type":"Coral Ring"}"#, "viscera");
        assert!(extra_items(&known, SHIPPED).unwrap().lines.is_empty());
    }

    #[test]
    fn adds_uniques_with_a_known_base() {
        let json = api(
            concat!(
                r#"{"type":"Coral Ring","name":"Angler's Plait"},"#,
                r#"{"type":"Coral Ring","name":"Bloodboil"},"#,
                r#"{"type":"Coral Ring","name":"Old Thing","disc":"legacy"},"#,
                r#"{"type":"Unheard Ring","name":"Lost"}"#,
            ),
            "accessory",
        );
        let extra = extra_items(&json, SHIPPED).unwrap();
        assert_eq!((extra.uniques, extra.skipped_uniques), (1, 1));
        let v: serde_json::Value = serde_json::from_str(&extra.lines[0]).unwrap();
        assert_eq!(v["name"], "Angler's Plait");
        assert_eq!(v["unique"]["base"], "Coral Ring");
    }

    #[test]
    fn extra_lines_load_into_the_item_db() {
        let json = api(r#"{"type":"Primate Foot"}"#, "viscera");
        let extra = extra_items(&json, SHIPPED).unwrap();
        let db = crate::data::ItemDb::parse(&format!("{SHIPPED}{}\n", extra.lines.join("\n"))).unwrap();
        let found = db.by_translated("ITEM", "Primate Foot").unwrap();
        assert_eq!(found[0].craftable.as_ref().unwrap().category.known(), None);
    }
}
