//! Which poe.ninja entry an item is priced by.
use super::floor_to_bracket;
use crate::parser::item::{ItemRarity, ParsedItem};
use crate::parser::meta::{is, is_armour, is_jewellery, is_weapon, known, ItemCategory};
use crate::parser::modifiers::ModifierType;

pub const SPECIAL_SUPPORT_GEM: [&str; 3] = ["Empower Support", "Enlighten Support", "Enhance Support"];

#[derive(Clone, Debug, PartialEq)]
pub struct DetailsId {
    pub ns: String,
    pub name: String,
    pub variant: Option<String>,
}

/// Normal, magic and rare equipment is priced by its base type.
pub fn is_valuable_basetype(item: &ParsedItem) -> bool {
    if !matches!(
        item.rarity,
        Some(ItemRarity::Normal | ItemRarity::Magic | ItemRarity::Rare)
    ) || item.category.is_none()
    {
        return false;
    }
    known(&item.category).is_some_and(|c| is_jewellery(c) || is_armour(c) || is_weapon(c) || c == ItemCategory::Quiver)
}

pub fn get_details_id(item: &ParsedItem) -> Option<DetailsId> {
    if is(&item.category, ItemCategory::Gem) {
        return Some(for_skill_gem(item));
    }
    if is(&item.category, ItemCategory::Map) {
        let tier = if item.rarity == Some(ItemRarity::Unique) {
            0.0
        } else {
            item.map_tier.unwrap_or(0.0)
        };
        let kind = if item.rarity == Some(ItemRarity::Unique) {
            None
        } else if item.map_tier.is_some_and(|t| t != 0.0) {
            Some("Gen-24")
        } else {
            Some("Atlas")
        };
        return Some(DetailsId {
            ns: item.info.namespace.clone(),
            name: item.info.ref_name.clone(),
            variant: variant(&[Some(format!("T{}", num(tier))), kind.map(String::from)]),
        });
    }
    if item.rarity == Some(ItemRarity::Unique) {
        return for_unique_item(item);
    }
    if is_valuable_basetype(item) {
        return for_basetype(item);
    }
    if item.info.ref_name == "Filled Coffin" {
        let stat = item.stats_by_type.first()?;
        let value = stat
            .sources
            .first()?
            .contributes
            .map(|c| num(c.value))
            .unwrap_or_else(|| "undefined".into());
        return Some(DetailsId {
            ns: item.info.namespace.clone(),
            name: stat.stat.ref_.replacen('#', &value, 1),
            variant: Some(num(floor_to_bracket(
                item.item_level.unwrap_or(f64::NAN),
                &[1.0, 70.0, 80.0, 84.0],
            ))),
        });
    }
    Some(DetailsId {
        ns: item.info.namespace.clone(),
        name: item.info.ref_name.clone(),
        variant: None,
    })
}

/// A number the way JavaScript prints it in a template string.
fn num(x: f64) -> String {
    if x.is_nan() {
        "NaN".into()
    } else if x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

fn for_skill_gem(item: &ParsedItem) -> DetailsId {
    let name = item.info.ref_name.as_str();
    let special = SPECIAL_SUPPORT_GEM.contains(&name);
    let level = item.gem_level.unwrap_or(f64::NAN);

    let mut v = String::new();
    if special || name == "Portal" || name == "Brand Recall" || name == "Blood and Sand" || level >= 20.0 {
        v += &num(level);
    } else {
        v += "1";
    }
    if let Some(quality) = item.quality.filter(|q| *q != 0.0 && !q.is_nan()) {
        if !special && !(name == "Brand Recall" && item.is_corrupted) {
            // Gem Q20 with up to 4xGCP
            let q = if (16.0..=20.0).contains(&quality) {
                20.0
            } else {
                quality
            };
            v += &format!("/{}", num(q));
        }
    }
    if item.is_corrupted && name != "Portal" {
        v += "c";
    }

    DetailsId {
        ns: item.info.namespace.clone(),
        name: name.to_string(),
        variant: Some(v),
    }
}

fn for_basetype(item: &ParsedItem) -> Option<DetailsId> {
    if item.influences.len() > 1 {
        return None;
    }
    let ilvl = item.item_level.unwrap_or(f64::NAN);
    Some(DetailsId {
        ns: item.info.namespace.clone(),
        name: item.info.ref_name.clone(),
        variant: variant(&[
            Some(if ilvl >= 86.0 { "86+".to_string() } else { num(ilvl) }),
            item.influences.first().map(|i| format!("{i:?}")),
        ]),
    })
}

fn for_unique_item(item: &ParsedItem) -> Option<DetailsId> {
    let base = item.unique_base.as_ref()?;
    let kind = if is(&item.category, ItemCategory::Flask) {
        None
    } else if is(&item.category, ItemCategory::SanctumRelic) {
        Some("Relic".to_string())
    } else {
        Some(base.ref_name.clone())
    };
    Some(DetailsId {
        ns: item.info.namespace.clone(),
        name: item.info.ref_name.clone(),
        variant: variant(&[
            unique_variant(item).map(String::from),
            kind,
            item.sockets
                .as_ref()
                .and_then(|s| s.linked)
                .map(|l| format!("{}L", num(l))),
        ]),
    })
}

fn variant(props: &[Option<String>]) -> Option<String> {
    let props: Vec<&str> = props.iter().flatten().map(String::as_str).collect();
    (!props.is_empty()).then(|| props.join(", "))
}

fn has_stat(item: &ParsedItem, stat: &str) -> bool {
    item.stats_by_type.iter().any(|m| m.stat.ref_ == stat)
}

fn unique_variant(item: &ParsedItem) -> Option<&'static str> {
    let has = |s: &str| has_stat(item, s);
    match item.info.ref_name.as_str() {
        "Vessel of Vinktar" => {
            if has("Adds # to # Lightning Damage to Attacks during Flask effect") {
                Some("Added Attacks")
            } else if has("Adds # to # Lightning Damage to Spells during Flask effect") {
                Some("Added Spells")
            } else if has("Damage Penetrates #% Lightning Resistance during Flask effect") {
                Some("Penetration")
            } else if has("#% of Physical Damage Converted to Lightning during Flask effect") {
                Some("Conversion")
            } else {
                None
            }
        }
        "Atziri's Splendour" => {
            if has("#% increased Armour, Evasion and Energy Shield") {
                Some("Armour/Evasion/ES")
            } else if has("#% increased Evasion and Energy Shield") && has("+# to maximum Energy Shield") {
                Some("Evasion/ES")
            } else if has("#% increased Evasion and Energy Shield") && has("+# to maximum Life") {
                Some("Evasion/ES/Life")
            } else if has("#% increased Armour and Energy Shield") && has("+# to maximum Energy Shield") {
                Some("Armour/ES")
            } else if has("#% increased Armour and Energy Shield") && has("+# to maximum Life") {
                Some("Armour/ES/Life")
            } else if has("#% increased Armour and Evasion") && has("+# to maximum Life") {
                Some("Armour/Evasion")
            } else if has("+# to maximum Energy Shield") && has("#% increased Energy Shield") {
                Some("ES")
            } else if has("#% increased Evasion Rating") && has("+# to maximum Life") {
                Some("Evasion")
            } else if has("#% increased Armour") && has("+# to maximum Life") {
                Some("Armour")
            } else {
                None
            }
        }
        "Bubonic Trail" | "Lightpoacher" | "Shroud of the Lightless" | "Tombfist" => {
            let sockets = item
                .stats_by_type
                .iter()
                .find(|m| m.type_ == ModifierType::Explicit && m.stat.ref_ == "Has # Abyssal Sockets")?;
            match sockets.sources.first()?.contributes?.value {
                2.0 => Some("2 Jewels"),
                1.0 => Some("1 Jewel"),
                _ => None,
            }
        }
        "Volkuur's Guidance" => {
            if has("Adds # to # Cold Damage to Spells and Attacks") {
                Some("Cold")
            } else if has("Adds # to # Fire Damage to Spells and Attacks") {
                Some("Fire")
            } else if has("Adds # to # Lightning Damage to Spells and Attacks") {
                Some("Lightning")
            } else {
                None
            }
        }
        "Yriel's Fostering" => {
            if has("Projectiles from Attacks have #% chance to Maim on Hit while\nyou have a Bestial Minion") {
                Some("Maim")
            } else if has("Projectiles from Attacks have #% chance to Poison on Hit while\nyou have a Bestial Minion") {
                Some("Poison")
            } else if has(
                "Projectiles from Attacks have #% chance to inflict Bleeding on Hit while\nyou have a Bestial Minion",
            ) {
                Some("Bleeding")
            } else {
                None
            }
        }
        "Doryani's Invitation" => {
            if has("#% increased Global Physical Damage") {
                Some("Physical")
            } else if has("#% increased Fire Damage") {
                Some("Fire")
            } else if has("#% increased Cold Damage") {
                Some("Cold")
            } else if has("#% increased Lightning Damage") {
                Some("Lightning")
            } else {
                None
            }
        }
        "Impresence" => {
            if has("Adds # to # Cold Damage") {
                Some("Cold")
            } else if has("Adds # to # Chaos Damage") {
                Some("Chaos")
            } else if has("Adds # to # Fire Damage") {
                Some("Fire")
            } else if has("Adds # to # Lightning Damage") {
                Some("Lightning")
            } else if has("Adds # to # Physical Damage") {
                Some("Physical")
            } else {
                None
            }
        }
        "Voices" => {
            let passives = item
                .stats_by_type
                .iter()
                .find(|m| m.stat.ref_ == "Adds # Small Passive Skills which grant nothing")?;
            match passives.sources.first()?.contributes?.value {
                7.0 => Some("7 passives"),
                5.0 => Some("5 passives"),
                3.0 => Some("3 passives"),
                _ => None,
            }
        }
        "The First Crest" => {
            if has("Aureus Coins are converted to Experience upon defeating the Herald of the Scourge") {
                Some("Experience")
            } else if has("Aureus Coins are converted to Relics upon defeating the Herald of the Scourge") {
                Some("Relics")
            } else if has("Aureus Coins are converted to Tainted Currency upon defeating the Herald of the Scourge") {
                Some("Tainted Currency")
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::parser::parse_clipboard;
    use std::path::Path;

    fn data() -> GameData {
        GameData::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"),
            "en",
        )
        .unwrap()
    }

    fn id(text: &str) -> Option<DetailsId> {
        get_details_id(&parse_clipboard(&data(), text).unwrap())
    }

    #[test]
    fn currency_and_gems_and_uniques() {
        let divine =
            id("Item Class: Stackable Currency\nRarity: Currency\nDivine Orb\n--------\nStack Size: 3/10\n").unwrap();
        assert_eq!(
            (divine.ns.as_str(), divine.name.as_str(), divine.variant),
            ("ITEM", "Divine Orb", None)
        );

        let gem = id("Item Class: Active Skill Gems\nRarity: Gem\nGrace\n--------\nAura, Spell\nLevel: 20 (Max)\nQuality: +20% (augmented)\n--------\nCorrupted\n").unwrap();
        assert_eq!(gem.variant.as_deref(), Some("20/20c"));

        let unique =
            id("Item Class: Belts\nRarity: Unique\nHeadhunter\nLeather Belt\n--------\nItem Level: 84\n").unwrap();
        assert_eq!(unique.variant.as_deref(), Some("Leather Belt"));
    }
}
