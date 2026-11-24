//! Reads `data/en/client_strings.json` (the game's UI strings and regexes);
//! regexes are `{"re": "...", "flags": "i"}`.
use anyhow::{bail, Context, Result};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;

enum Entry {
    Str(String),
    Re(Regex),
    List(Vec<String>),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawEntry {
    Str(String),
    Re { re: String, flags: String },
    List(Vec<String>),
}

impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match RawEntry::deserialize(d)? {
            RawEntry::Str(s) => Self::Str(s),
            RawEntry::List(l) => Self::List(l),
            RawEntry::Re { re, flags } => Self::Re(
                RegexBuilder::new(&re)
                    .case_insensitive(flags.contains('i'))
                    .multi_line(flags.contains('m'))
                    .dot_matches_new_line(flags.contains('s'))
                    .build()
                    .map_err(serde::de::Error::custom)?,
            ),
        })
    }
}

macro_rules! client_strings {
    (str: [$($s:ident),* $(,)?], re: [$($r:ident),* $(,)?], list: [$($l:ident),* $(,)?] $(,)?) => {
        #[expect(non_snake_case, reason = "field names mirror the game constants")]
        pub struct ClientStrings {
            $(pub $s: String,)*
            $(pub $r: Regex,)*
            $(pub $l: Vec<String>,)*
        }

        impl ClientStrings {
            fn from_entries(mut map: HashMap<String, Entry>) -> Result<Self> {
                Ok(ClientStrings {
                    $($s: match map.remove(stringify!($s)) {
                        Some(Entry::Str(v)) => v,
                        _ => bail!("client_strings: {} missing or not a string", stringify!($s)),
                    },)*
                    $($r: match map.remove(stringify!($r)) {
                        Some(Entry::Re(v)) => v,
                        _ => bail!("client_strings: {} missing or not a regex", stringify!($r)),
                    },)*
                    $($l: match map.remove(stringify!($l)) {
                        Some(Entry::List(v)) => v,
                        _ => bail!("client_strings: {} missing or not a list", stringify!($l)),
                    },)*
                })
            }
        }
    };
}

client_strings! {
    str: [
        RARITY_NORMAL, RARITY_MAGIC, RARITY_RARE, RARITY_UNIQUE, RARITY_GEM, RARITY_CURRENCY,
        RARITY_DIVCARD, RARITY_QUEST, MAP_ITEM_QUANTITY, MAP_ITEM_RARITY, MAP_MONSTER_PACK_SIZE,
        MAP_MORE_MAPS, MAP_MORE_CURRENCY, MAP_MORE_DIVINATION_CARDS, RARITY,
        ITEM_CLASS, ITEM_LEVEL, CORPSE_LEVEL, TALISMAN_TIER, GEM_LEVEL, STACK_SIZE, SOCKETS,
        QUALITY, MEMORY_STRANDS, PHYSICAL_DAMAGE, ELEMENTAL_DAMAGE, CRIT_CHANCE, ATTACK_SPEED,
        ARMOUR, EVASION, ENERGY_SHIELD, TAG_WARD, BLOCK_CHANCE, CORRUPTED, UNIDENTIFIED,
        INFLUENCE_SHAPER, INFLUENCE_ELDER, INFLUENCE_CRUSADER, INFLUENCE_HUNTER,
        INFLUENCE_REDEEMER, INFLUENCE_WARLORD, SECTION_SYNTHESISED, VEILED_PREFIX, VEILED_SUFFIX,
        METAMORPH_HELP, BEAST_HELP, VOIDSTONE_HELP, CANNOT_USE_ITEM, AREA_LEVEL,
        HEIST_WINGS_REVEALED, HEIST_BLUEPRINT_TARGET, HEIST_BLUEPRINT_ENCHANTS,
        HEIST_BLUEPRINT_TRINKETS, HEIST_BLUEPRINT_GEMS, HEIST_BLUEPRINT_REPLICAS,
        HEIST_JOB_LOCKPICKING, HEIST_JOB_BRUTEFORCE, HEIST_JOB_PERCEPTION, HEIST_JOB_DEMOLITION,
        HEIST_JOB_COUNTERTHAUMATURGY, HEIST_JOB_TRAPDISARMAMENT, HEIST_JOB_AGILITY,
        HEIST_JOB_DECEPTION, HEIST_JOB_ENGINEERING, HEIST_TARGET_PRICELESS, MIRRORED, SPLIT,
        PREFIX_MODIFIER, SUFFIX_MODIFIER, CRAFTED_PREFIX, CRAFTED_SUFFIX, IMPLICIT_MODIFIER,
        FRACTURED_PREFIX, FRACTURED_SUFFIX, UNSCALABLE_VALUE, CORRUPTED_IMPLICIT, INCURSION_OPEN,
        INCURSION_OBSTRUCTED, ITEM_EATER, ITEM_EXARCH, ELDRITCH_MOD_R1, ELDRITCH_MOD_R2,
        ELDRITCH_MOD_R3, ELDRITCH_MOD_R4, ELDRITCH_MOD_R5, ELDRITCH_MOD_R6, SENTINEL_CHARGE,
        FOIL_UNIQUE, UNMODIFIABLE, TWICE_CORRUPTED, FOULBORN_MODIFIER, VESTIGIAL_IMPLICIT, MAP_AREA,
        CHART_SULPHUR, MERCENARY_LEVEL, MERCENARY_BUILD,
    ],
    re: [
        MAP_TIER, MAP_COMPLETION_REWARD, ITEM_SUPERIOR, MAP_BLIGHTED, MAP_BLIGHT_RAVAGED,
        ITEM_SYNTHESISED, FLASK_CHARGES, METAMORPH_BRAIN, METAMORPH_EYE, METAMORPH_LUNG,
        METAMORPH_HEART, METAMORPH_LIVER, QUALITY_ANOMALOUS, QUALITY_DIVERGENT,
        QUALITY_PHANTASMAL, HEIST_CONTRACT_JOB, HEIST_CONTRACT_TARGET, MODIFIER_LINE,
        MODIFIER_INCREASED, EATER_IMPLICIT, EXARCH_IMPLICIT, FOULBORN_NAME, VESTIGIAL_NAME,
        CHAT_SYSTEM, CHAT_TRADE, CHAT_GLOBAL, CHAT_PARTY, CHAT_GUILD, CHAT_WHISPER_TO,
        CHAT_WHISPER_FROM, CHAT_WEBTRADE_GEM,
    ],
    list: [
        SHAPER_MODS, ELDER_MODS, CRUSADER_MODS, HUNTER_MODS, REDEEMER_MODS, WARLORD_MODS,
        DELVE_MODS, VEILED_MODS, INCURSION_MODS, ESSENCE_MODS, INFAMOUS_MODS,
    ],
}

impl ClientStrings {
    pub fn parse(source: &str) -> Result<Self> {
        Self::from_entries(serde_json::from_str(source).context("client_strings json")?)
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn parses_english_client_strings() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/en/client_strings.json"
        ))
        .unwrap();
        let cs = ClientStrings::parse(&src).unwrap();
        assert_eq!(cs.ITEM_CLASS, "Item Class: ");
        assert_eq!(cs.HEIST_BLUEPRINT_TRINKETS, "Thieves' Trinkets or Currency");
        assert_eq!(cs.UNSCALABLE_VALUE, " \u{2014} Unscalable Value");
        assert_eq!(&cs.MAP_TIER.captures(" (Tier 14)").unwrap()[1], "14");
        let m = cs
            .HEIST_CONTRACT_JOB
            .captures("Requires Lockpicking (Level 3)")
            .unwrap();
        assert_eq!(&m["job"], "Lockpicking");
        assert_eq!(cs.SHAPER_MODS.len(), 4);
    }
}
