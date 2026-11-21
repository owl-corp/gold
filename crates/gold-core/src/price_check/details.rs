//! Which poe.ninja entry an item is priced by.
use super::floor_to_bracket;
use crate::parser::item::{ItemRarity, ParsedItem};
use crate::parser::meta::{is, is_armour, is_jewellery, is_weapon, known, ItemCategory};
use crate::parser::modifiers::ModifierType;

pub const SPECIAL_SUPPORT_GEM: [&str; 3] = ["Empower Support", "Enlighten Support", "Enhance Support"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailsId {
    pub ns: String,
    pub name: String,
    pub variant: Option<String>,
}

/// Normal, magic and rare equipment is priced by its base type.
#[must_use]
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
            0
        } else {
            item.map.tier.unwrap_or(0)
        };
        let kind = if item.rarity == Some(ItemRarity::Unique) {
            None
        } else if item.map.tier.is_some_and(|t| t != 0) {
            Some("Gen-24")
        } else {
            Some("Atlas")
        };
        return Some(DetailsId {
            ns: item.info.namespace.clone(),
            name: item.info.ref_name.clone(),
            variant: variant(&[Some(format!("T{tier}")), kind.map(String::from)]),
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
            .map_or_else(|| "undefined".into(), |c| num(c.value));
        return Some(DetailsId {
            ns: item.info.namespace.clone(),
            name: stat.stat.ref_.replacen('#', &value, 1),
            // without an item level no bracket applies
            variant: item
                .item_level
                .map(|ilvl| floor_to_bracket(ilvl, &[1, 70, 80, 84]).to_string()),
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
    } else if crate::js::eq(x, x.trunc()) && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

fn for_skill_gem(item: &ParsedItem) -> DetailsId {
    let name = item.info.ref_name.as_str();
    let special = SPECIAL_SUPPORT_GEM.contains(&name);
    let level = item.gem.level;

    let mut v = String::new();
    if special
        || name == "Portal"
        || name == "Brand Recall"
        || name == "Blood and Sand"
        || level.is_some_and(|l| l >= 20)
    {
        v += &level.map(|l| l.to_string()).unwrap_or_default();
    } else {
        v += "1";
    }
    if let Some(quality) = item.quality.filter(|q| *q != 0) {
        if !(special || (name == "Brand Recall" && item.status.is_corrupted)) {
            // Gem Q20 with up to 4xGCP
            let q = if (16..=20).contains(&quality) { 20 } else { quality };
            v.push('/');
            v += &q.to_string();
        }
    }
    if item.status.is_corrupted && name != "Portal" {
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
    let ilvl = item.item_level;
    Some(DetailsId {
        ns: item.info.namespace.clone(),
        name: item.info.ref_name.clone(),
        variant: variant(&[
            ilvl.map(|l| if l >= 86 { "86+".to_string() } else { l.to_string() }),
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
            item.sockets.as_ref().and_then(|s| s.linked).map(|l| format!("{l}L")),
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

/// Uniques whose variant is the first label with all of its stats on the item.
type VariantRule = (&'static str, &'static [&'static str]);

const VARIANT_RULES: &[(&str, &[VariantRule])] = &[
    (
        "Vessel of Vinktar",
        &[
            ("Added Attacks", &["Adds # to # Lightning Damage to Attacks during Flask effect"]),
            ("Added Spells", &["Adds # to # Lightning Damage to Spells during Flask effect"]),
            ("Penetration", &["Damage Penetrates #% Lightning Resistance during Flask effect"]),
            ("Conversion", &["#% of Physical Damage Converted to Lightning during Flask effect"]),
        ],
    ),
    (
        "Atziri's Splendour",
        &[
            ("Armour/Evasion/ES", &["#% increased Armour, Evasion and Energy Shield"]),
            (
                "Evasion/ES",
                &["#% increased Evasion and Energy Shield", "+# to maximum Energy Shield"],
            ),
            (
                "Evasion/ES/Life",
                &["#% increased Evasion and Energy Shield", "+# to maximum Life"],
            ),
            (
                "Armour/ES",
                &["#% increased Armour and Energy Shield", "+# to maximum Energy Shield"],
            ),
            (
                "Armour/ES/Life",
                &["#% increased Armour and Energy Shield", "+# to maximum Life"],
            ),
            ("Armour/Evasion", &["#% increased Armour and Evasion", "+# to maximum Life"]),
            ("ES", &["+# to maximum Energy Shield", "#% increased Energy Shield"]),
            ("Evasion", &["#% increased Evasion Rating", "+# to maximum Life"]),
            ("Armour", &["#% increased Armour", "+# to maximum Life"]),
        ],
    ),
    (
        "Volkuur's Guidance",
        &[
            ("Cold", &["Adds # to # Cold Damage to Spells and Attacks"]),
            ("Fire", &["Adds # to # Fire Damage to Spells and Attacks"]),
            ("Lightning", &["Adds # to # Lightning Damage to Spells and Attacks"]),
        ],
    ),
    (
        "Yriel's Fostering",
        &[
            (
                "Maim",
                &["Projectiles from Attacks have #% chance to Maim on Hit while\nyou have a Bestial Minion"],
            ),
            (
                "Poison",
                &["Projectiles from Attacks have #% chance to Poison on Hit while\nyou have a Bestial Minion"],
            ),
            (
                "Bleeding",
                &["Projectiles from Attacks have #% chance to inflict Bleeding on Hit while\nyou have a Bestial Minion"],
            ),
        ],
    ),
    (
        "Doryani's Invitation",
        &[
            ("Physical", &["#% increased Global Physical Damage"]),
            ("Fire", &["#% increased Fire Damage"]),
            ("Cold", &["#% increased Cold Damage"]),
            ("Lightning", &["#% increased Lightning Damage"]),
        ],
    ),
    (
        "Impresence",
        &[
            ("Cold", &["Adds # to # Cold Damage"]),
            ("Chaos", &["Adds # to # Chaos Damage"]),
            ("Fire", &["Adds # to # Fire Damage"]),
            ("Lightning", &["Adds # to # Lightning Damage"]),
            ("Physical", &["Adds # to # Physical Damage"]),
        ],
    ),
    (
        "The First Crest",
        &[
            (
                "Experience",
                &["Aureus Coins are converted to Experience upon defeating the Herald of the Scourge"],
            ),
            (
                "Relics",
                &["Aureus Coins are converted to Relics upon defeating the Herald of the Scourge"],
            ),
            (
                "Tainted Currency",
                &["Aureus Coins are converted to Tainted Currency upon defeating the Herald of the Scourge"],
            ),
        ],
    ),
];

fn unique_variant(item: &ParsedItem) -> Option<&'static str> {
    let name = item.info.ref_name.as_str();
    if let Some((_, rules)) = VARIANT_RULES.iter().find(|(n, _)| *n == name) {
        return rules
            .iter()
            .find(|(_, stats)| stats.iter().all(|s| has_stat(item, s)))
            .map(|(label, _)| *label);
    }
    match name {
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
        _ => None,
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::parser::parse_clipboard;
    use std::path::Path;

    fn data() -> GameData {
        GameData::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")).unwrap()
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
