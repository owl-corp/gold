//! Related-item groups: what an item is sold alongside, and what it drops or is used for.
//!
//! `item_drop.json` is Awakened `PoE` Trade's `data/item-drop.json` (MIT, SnosMe/awakened-poe-trade).
//! Entries read `NS::name` or `NS::name // variant`, the same ids `get_details_id` produces.
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
pub struct Group {
    /// The related set; the checked item is one of these.
    pub query: Vec<String>,
    /// What it drops, or is used for.
    pub items: Vec<String>,
}

/// One entry id split into its parts.
#[derive(Debug, PartialEq, Eq)]
pub struct Key<'a> {
    pub ns: &'a str,
    pub name: &'a str,
    pub variant: Option<&'a str>,
}

#[must_use]
pub fn parse_key(id: &str) -> Option<Key<'_>> {
    let (ns, rest) = id.split_once("::")?;
    let (name, variant) = match rest.split_once(" // ") {
        Some((n, v)) => (n, Some(v)),
        None => (rest, None),
    };
    Some(Key { ns, name, variant })
}

fn groups() -> &'static [Group] {
    static GROUPS: OnceLock<Vec<Group>> = OnceLock::new();
    GROUPS.get_or_init(|| serde_json::from_str(include_str!("item_drop.json")).expect("item_drop.json is valid"))
}

/// The first group whose related set contains this item.
#[must_use]
pub fn find_group(ns: &str, name: &str, variant: Option<&str>) -> Option<&'static Group> {
    let id = variant
        .filter(|v| !v.is_empty())
        .map_or_else(|| format!("{ns}::{name}"), |v| format!("{ns}::{name} // {v}"));
    groups().iter().find(|g| g.query.contains(&id))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn every_entry_parses_and_groups_are_found_by_id() {
        for g in groups() {
            for id in g.query.iter().chain(&g.items) {
                assert!(parse_key(id).is_some(), "{id}");
            }
        }
        let g = find_group("ITEM", "Deafening Essence of Greed", None).unwrap();
        assert_eq!(g.query.len(), 3);
        assert!(find_group("ITEM", "Vaal Temple Map", Some("T0, Atlas")).is_some());
        assert_eq!(
            parse_key("UNIQUE::Headhunter // Leather Belt"),
            Some(Key {
                ns: "UNIQUE",
                name: "Headhunter",
                variant: Some("Leather Belt")
            })
        );
    }
}
