//! Search filters built from a parsed item.
//! The overlay shows these and lets the user change them; the trade request is built from them.
pub mod item_filters;
pub mod presets;
pub mod pseudo;
pub mod rules;
pub mod stat_filters;
pub mod util;

use crate::parser::item::ItemInfluence;
use crate::parser::meta::Category;
use crate::parser::modifiers::StatSource;
use serde::Serialize;

/// A name or base type to search for, with the text the trade site wants.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Search {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_trade: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_type_trade: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator_trade: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Category>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<Box<Self>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FilterNumeric {
    pub value: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    pub disabled: bool,
}

impl FilterNumeric {
    #[must_use]
    pub const fn new(value: i64, disabled: bool) -> Self {
        Self {
            value,
            max: None,
            disabled,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Rarity {
    pub value: &'static str,
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Corrupted {
    pub value: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Value<T> {
    pub value: T,
}

#[derive(Clone, Debug, Serialize)]
pub struct DisabledOnly {
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct HideableToggle {
    pub disabled: bool,
    pub hidden: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct InfluenceFilter {
    pub value: ItemInfluence,
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapCompletionReward {
    pub name: String,
    pub name_trade: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Unidentified {
    pub value: bool,
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Veiled {
    pub stat_refs: Vec<String>,
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeFilters {
    pub currency: Option<String>,
    pub league: String,
    pub collapse_listings: &'static str,
    pub collapse_merchant: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemFilters {
    pub search_exact: Search,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_relaxed: Option<Search>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity: Option<Rarity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_sockets: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub white_sockets: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrupted: Option<Corrupted>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fractured: Option<Value<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imbued_gem: Option<DisabledOnly>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirrored: Option<HideableToggle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<HideableToggle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foil: Option<DisabledOnly>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foulborn: Option<Value<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vestigial: Option<Value<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub influences: Option<Vec<InfluenceFilter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem_level: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_tier: Option<FilterNumeric>,
    /// "Blighted", "Blight-ravaged" or false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_blighted: Option<Value<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_completion_reward: Option<MapCompletionReward>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_level: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_size: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unidentified: Option<Unidentified>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub veiled: Option<Veiled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_level: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentinel_charge: Option<FilterNumeric>,
    pub trade: TradeFilters,
}

/// What a stat filter is about; also used to colour it.
/// Values are the modifier types and mechanics, plus a few filter-only kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct FilterTag(pub &'static str);

impl FilterTag {
    pub const PSEUDO: Self = Self("pseudo");
    pub const EXPLICIT: Self = Self("explicit");
    pub const IMPLICIT: Self = Self("implicit");
    pub const ENCHANT: Self = Self("enchant");
    pub const FRACTURED: Self = Self("fractured");
    pub const FOULBORN: Self = Self("foulborn");
    pub const VESTIGIAL: Self = Self("vestigial");
    pub const VARIANT: Self = Self("variant");
    pub const PROPERTY: Self = Self("property");
}

#[derive(Clone, Debug, Serialize)]
pub struct FilterOption {
    pub value: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Bounds {
    pub min: f64,
    pub max: f64,
}

/// The numbers of a stat filter. `min` and `max` are what the user edits; None = no limit.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterRoll {
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    pub default: Bounds,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_invert: Option<bool>,
    pub dp: bool,
    pub is_negated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goodness: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatFilter {
    pub trade_id: Vec<String>,
    pub stat_ref: String,
    pub text: String,
    pub tag: FilterTag,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oils: Option<Vec<&'static str>>,
    pub sources: Vec<StatSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll: Option<FilterRoll>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<FilterOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<String>,
    pub disabled: bool,
}

/// Trade ids that are not stats on the trade site but item properties (armour, dps, ...).
pub const INTERNAL_TRADE_IDS: &[&str] = &[
    "item.not_group",
    "item.count_one_group",
    "item.base_percentile",
    "item.memory_strands",
    "item.armour",
    "item.evasion_rating",
    "item.energy_shield",
    "item.ward",
    "item.block",
    "item.total_dps",
    "item.physical_dps",
    "item.elemental_dps",
    "item.crit",
    "item.aps",
    "item.has_empty_modifier",
    "item.mercenary_6link",
    "item.map_item_quantity",
    "item.map_item_rarity",
    "item.map_pack_size",
    "item.heist_job_lockpicking",
    "item.heist_job_bruteforce",
    "item.heist_job_perception",
    "item.heist_job_demolition",
    "item.heist_job_counterthaumaturgy",
    "item.heist_job_trapdisarmament",
    "item.heist_job_agility",
    "item.heist_job_deception",
    "item.heist_job_engineering",
    "item.heist_target_priceless",
    "item.heist_wings_revealed",
    "item.heist_wings_total",
    "item.map.chart_sulphur",
];

pub const HAS_EMPTY_MODIFIER_ANY: f64 = 0.0;
pub const HAS_EMPTY_MODIFIER_PREFIX: f64 = 1.0;
pub const HAS_EMPTY_MODIFIER_SUFFIX: f64 = 2.0;
