//! The parsed item. Serialized field names and
//! shapes are camelCase JSON so results can be compared with the golden files.
use super::advanced::ParsedModifier;
use super::meta::Category;
use super::modifiers::{ModifierType, StatCalculated};
use super::stat_translations::ParsedStat;
use crate::data::BaseType;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ItemRarity {
    Normal,
    Magic,
    Rare,
    Unique,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ItemInfluence {
    Crusader,
    Elder,
    Hunter,
    Redeemer,
    Shaper,
    Warlord,
}

/// Matches the trade site's option values.
pub const INCURSION_OPEN: f64 = 1.0;
pub const INCURSION_OBSTRUCTED: f64 = 2.0;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeistBlueprint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wings_revealed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wings_total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<&'static str>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeistContract {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_job: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_value: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Sockets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked: Option<f64>,
    pub white: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct StackSize {
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnknownModifier {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ModifierType,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity: Option<ItemRarity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_level: Option<f64>,

    #[serde(rename = "armourAR", skip_serializing_if = "Option::is_none")]
    pub armour_ar: Option<f64>,
    #[serde(rename = "armourEV", skip_serializing_if = "Option::is_none")]
    pub armour_ev: Option<f64>,
    #[serde(rename = "armourES", skip_serializing_if = "Option::is_none")]
    pub armour_es: Option<f64>,
    #[serde(rename = "armourWARD", skip_serializing_if = "Option::is_none")]
    pub armour_ward: Option<f64>,
    #[serde(rename = "armourBLOCK", skip_serializing_if = "Option::is_none")]
    pub armour_block: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_percentile: Option<f64>,
    #[serde(rename = "weaponCRIT", skip_serializing_if = "Option::is_none")]
    pub weapon_crit: Option<f64>,
    #[serde(rename = "weaponAS", skip_serializing_if = "Option::is_none")]
    pub weapon_as: Option<f64>,
    #[serde(rename = "weaponPHYSICAL", skip_serializing_if = "Option::is_none")]
    pub weapon_physical: Option<f64>,
    #[serde(rename = "weaponELEMENTAL", skip_serializing_if = "Option::is_none")]
    pub weapon_elemental: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_area: Option<BaseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_item_quantity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_item_rarity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_pack_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_completion_reward: Option<BaseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_tier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_more_maps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_more_scarabs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_more_currency: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_more_div_cards: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_blueprint: Option<HeistBlueprint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_contract: Option<HeistContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logbook_area_mods: Option<Vec<Vec<ParsedModifier>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_sulphur: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem_level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vaal_gem: Option<BaseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imbued_gem: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mercenary_build: Option<BaseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mercenary_skills: Option<Vec<Vec<ParsedStat>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub talisman_tier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_strands: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<Sockets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_size: Option<StackSize>,
    pub is_unidentified: bool,
    pub is_corrupted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_unmodifiable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_mirrored: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_split: Option<bool>,
    pub influences: Vec<ItemInfluence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentinel_charge: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_synthesised: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_fractured: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_veiled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_foil: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_foulborn: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_vestigial: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dust_equivalent: Option<f64>,
    pub stats_by_type: Vec<StatCalculated>,
    pub new_mods: Vec<ParsedModifier>,
    pub unknown_modifiers: Vec<UnknownModifier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Category>,
    pub info: BaseType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_base: Option<BaseType>,
    pub raw_text: String,

    /// Parser scratch state: not part of the result.
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    pub base_type: Option<String>,
    #[serde(skip)]
    pub info_variants: Vec<BaseType>,
}
