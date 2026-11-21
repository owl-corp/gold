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
pub const INCURSION_OPEN: i64 = 1;
pub const INCURSION_OBSTRUCTED: i64 = 2;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeistBlueprint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wings_revealed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wings_total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<&'static str>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeistContract {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_job: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_level: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_value: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Sockets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked: Option<i64>,
    pub white: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct StackSize {
    pub value: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnknownModifier {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ModifierType,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ArmourInfo {
    #[serde(rename = "armourAR", skip_serializing_if = "Option::is_none")]
    pub ar: Option<i64>,
    #[serde(rename = "armourEV", skip_serializing_if = "Option::is_none")]
    pub ev: Option<i64>,
    #[serde(rename = "armourES", skip_serializing_if = "Option::is_none")]
    pub es: Option<i64>,
    #[serde(rename = "armourWARD", skip_serializing_if = "Option::is_none")]
    pub ward: Option<i64>,
    #[serde(rename = "armourBLOCK", skip_serializing_if = "Option::is_none")]
    pub block: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct WeaponInfo {
    #[serde(rename = "weaponCRIT", skip_serializing_if = "Option::is_none")]
    pub crit: Option<f64>,
    #[serde(rename = "weaponAS", skip_serializing_if = "Option::is_none")]
    pub attack_speed: Option<f64>,
    #[serde(rename = "weaponPHYSICAL", skip_serializing_if = "Option::is_none")]
    pub physical: Option<f64>,
    #[serde(rename = "weaponELEMENTAL", skip_serializing_if = "Option::is_none")]
    pub elemental: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct MapInfo {
    #[serde(rename = "mapArea", skip_serializing_if = "Option::is_none")]
    pub area: Option<BaseType>,
    #[serde(rename = "areaLevel", skip_serializing_if = "Option::is_none")]
    pub area_level: Option<i64>,
    #[serde(rename = "areaItemQuantity", skip_serializing_if = "Option::is_none")]
    pub item_quantity: Option<i64>,
    #[serde(rename = "areaItemRarity", skip_serializing_if = "Option::is_none")]
    pub item_rarity: Option<i64>,
    #[serde(rename = "areaPackSize", skip_serializing_if = "Option::is_none")]
    pub pack_size: Option<i64>,
    #[serde(rename = "mapCompletionReward", skip_serializing_if = "Option::is_none")]
    pub completion_reward: Option<BaseType>,
    #[serde(rename = "mapTier", skip_serializing_if = "Option::is_none")]
    pub tier: Option<i64>,
    #[serde(rename = "mapMoreMaps", skip_serializing_if = "Option::is_none")]
    pub more_maps: Option<i64>,
    #[serde(rename = "mapMoreCurrency", skip_serializing_if = "Option::is_none")]
    pub more_currency: Option<i64>,
    #[serde(rename = "mapMoreDivCards", skip_serializing_if = "Option::is_none")]
    pub more_div_cards: Option<i64>,
    #[serde(rename = "chartSulphur", skip_serializing_if = "Option::is_none")]
    pub chart_sulphur: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct HeistInfo {
    #[serde(rename = "heistBlueprint", skip_serializing_if = "Option::is_none")]
    pub blueprint: Option<HeistBlueprint>,
    #[serde(rename = "heistContract", skip_serializing_if = "Option::is_none")]
    pub contract: Option<HeistContract>,
    #[serde(rename = "logbookAreaMods", skip_serializing_if = "Option::is_none")]
    pub logbook_area_mods: Option<Vec<Vec<ParsedModifier>>>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct GemInfo {
    #[serde(rename = "gemLevel", skip_serializing_if = "Option::is_none")]
    pub level: Option<i64>,
    #[serde(rename = "vaalGem", skip_serializing_if = "Option::is_none")]
    pub vaal: Option<BaseType>,
    #[serde(rename = "imbuedGem", skip_serializing_if = "std::ops::Not::not")]
    pub imbued: bool,
    #[serde(rename = "mercenaryBuild", skip_serializing_if = "Option::is_none")]
    pub mercenary_build: Option<BaseType>,
    #[serde(rename = "mercenarySkills", skip_serializing_if = "Option::is_none")]
    pub mercenary_skills: Option<Vec<Vec<ParsedStat>>>,
}

/// Whether the item is identified, corrupted or locked.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStatus {
    pub is_unidentified: bool,
    pub is_corrupted: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_unmodifiable: bool,
}

/// How the item came to be.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemOrigin {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_mirrored: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_split: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_synthesised: bool,
}

/// Special modifiers on the item.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemModMarks {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_fractured: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_veiled: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_foulborn: bool,
}

/// Cosmetic or legacy markers.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemLooks {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_foil: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_vestigial: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedItem {
    #[serde(flatten)]
    pub status: ItemStatus,
    #[serde(flatten)]
    pub origin: ItemOrigin,
    #[serde(flatten)]
    pub mod_marks: ItemModMarks,
    #[serde(flatten)]
    pub looks: ItemLooks,
    #[serde(flatten)]
    pub armour: ArmourInfo,
    #[serde(flatten)]
    pub weapon: WeaponInfo,
    #[serde(flatten)]
    pub map: MapInfo,
    #[serde(flatten)]
    pub heist: HeistInfo,
    #[serde(flatten)]
    pub gem: GemInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity: Option<ItemRarity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_level: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_percentile: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub talisman_tier: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_strands: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<Sockets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_size: Option<StackSize>,
    pub influences: Vec<ItemInfluence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentinel_charge: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dust_equivalent: Option<i64>,
    pub stats_by_type: Vec<StatCalculated>,
    pub new_mods: Vec<ParsedModifier>,
    pub unknown_modifiers: Vec<UnknownModifier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Category>,
    pub info: BaseType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_base: Option<BaseType>,
    pub raw_text: String,
}
