//! Which filter presets an item gets.
use super::item_filters::{create_filters, CreateOptions};
use super::stat_filters::{create_exact_stat_filters, create_mercenary_filters, init_ui_mod_filters, Mode};
use super::{ItemFilters, StatFilter};
use crate::data::GameData;
use crate::parser::item::{ItemRarity, ParsedItem};
use crate::parser::meta::{known, ItemCategory};
use crate::parser::modifiers::ModifierType;
use anyhow::Result;

#[derive(Clone, Debug)]
pub struct FilterPreset {
    pub id: &'static str,
    pub filters: ItemFilters,
    pub stats: Vec<StatFilter>,
}

pub struct Presets {
    pub active: &'static str,
    pub presets: Vec<FilterPreset>,
}

/// How far stat filters are widened around the item's value, in percent.
const SEARCH_STAT_RANGE: f64 = 10.0;

#[derive(Clone, Debug)]
pub struct PresetOptions {
    pub league: String,
    pub currency: Option<String>,
    pub collapse_listings: &'static str,
    pub activate_stock_filter: bool,
}

/// An item kind whose search rules are not ported yet.
#[derive(Debug)]
pub struct Unsupported(pub String);

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "searching for {} is not supported yet", self.0)
    }
}

impl std::error::Error for Unsupported {}

fn item_opts(opts: &PresetOptions, exact: bool) -> CreateOptions {
    CreateOptions {
        league: opts.league.clone(),
        currency: opts.currency.clone(),
        collapse_listings: opts.collapse_listings,
        activate_stock_filter: opts.activate_stock_filter,
        exact,
    }
}

/// Presets of an unidentified-value map or chart: bulk, plus a props search for rares.
fn map_presets(data: &GameData, item: &ParsedItem, opts: &PresetOptions, cat: Option<ItemCategory>) -> Result<Presets> {
    let range = SEARCH_STAT_RANGE;
    let exact_filters = |exact: bool| create_filters(data, item, &item_opts(opts, exact));
    let bulk = FilterPreset {
        id: "filters.preset_bulk",
        filters: exact_filters(true)?,
        stats: create_exact_stat_filters(data, item, &item.stats_by_type, range, Mode::Bulk)?,
    };
    let blighted = item
        .info
        .area
        .as_ref()
        .and_then(|a| a.get("blighted"))
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    if item.rarity == Some(ItemRarity::Rare) && !item.status.is_unidentified && !blighted {
        let props = FilterPreset {
            id: "filters.preset_pseudo",
            filters: exact_filters(true)?,
            stats: create_exact_stat_filters(data, item, &item.stats_by_type, range, Mode::Props)?,
        };
        let special_area = item
            .map
            .area
            .as_ref()
            .and_then(|a| a.area.as_ref())
            .and_then(|a| a.get("special"))
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let active = if cat == Some(ItemCategory::Chart) && special_area {
            bulk.id
        } else {
            props.id
        };
        return Ok(Presets {
            active,
            presets: vec![props, bulk],
        });
    }
    Ok(Presets {
        active: bulk.id,
        presets: vec![bulk],
    })
}

pub fn create_presets(data: &GameData, item: &ParsedItem, opts: &PresetOptions) -> Result<Presets> {
    use ItemCategory::{
        AbyssJewel, Charm, Chart, ClusterJewel, Flask, HeistBlueprint, HeistContract, Idol, Invitation, Jewel, Map,
        MemoryLine, SanctumRelic, Sentinel, Tincture,
    };
    let name = item.info.ref_name.as_str();
    if name == "Expedition Logbook"
        || item
            .unique_base
            .as_ref()
            .is_some_and(|b| b.ref_name == "Timeless Jewel")
    {
        return Err(Unsupported(name.to_string()).into());
    }

    let cat = known(&item.category);
    let range = SEARCH_STAT_RANGE;
    let unique = item.rarity == Some(ItemRarity::Unique);

    let exact_filters = |exact: bool| create_filters(data, item, &item_opts(opts, exact));

    if name == "Mercenary Warrant" {
        let exact = FilterPreset {
            id: "filters.preset_exact",
            filters: exact_filters(false)?,
            stats: create_mercenary_filters(item),
        };
        return Ok(Presets {
            active: exact.id,
            presets: vec![exact],
        });
    }

    if (item.info.craftable.is_none() && !unique)
        || item.status.is_unidentified
        || item.rarity == Some(ItemRarity::Normal)
        || (cat == Some(Flask) && !unique)
        || (cat == Some(Tincture) && !unique)
        || (cat == Some(SanctumRelic) && !unique)
        || (cat == Some(Idol) && !unique)
        || matches!(
            cat,
            Some(Charm | Map | MemoryLine | Invitation | HeistContract | HeistBlueprint | Chart | Sentinel)
        )
    {
        if !unique && ((cat == Some(Map) && item.map.completion_reward.is_none()) || cat == Some(Chart)) {
            return map_presets(data, item, opts, cat);
        }

        let exact = FilterPreset {
            id: "filters.preset_exact",
            filters: exact_filters(true)?,
            stats: create_exact_stat_filters(data, item, &item.stats_by_type, range, Mode::Default)?,
        };
        return Ok(Presets {
            active: exact.id,
            presets: vec![exact],
        });
    }

    let pseudo = FilterPreset {
        id: "filters.preset_pseudo",
        filters: exact_filters(false)?,
        stats: init_ui_mod_filters(data, item, range)?,
    };

    let likely_finished = unique
        || item.stats_by_type.iter().any(|c| c.type_ == ModifierType::Crafted)
        || (item.quality == Some(20) && item.memory_strands.is_none()) // quality > 20 can be used for selling bases, quality < 20 drops sometimes
        || item.status.is_corrupted
        || item.origin.is_mirrored;

    let has_crafting_value = item.origin.is_synthesised
        || item.mod_marks.is_fractured
        || !item.influences.is_empty()
        || cat == Some(ClusterJewel)
        || (cat == Some(Jewel) && item.rarity == Some(ItemRarity::Magic))
        || (!matches!(cat, Some(Jewel | AbyssJewel)) && item.item_level.is_some_and(|l| l >= 82));

    if likely_finished || !has_crafting_value {
        return Ok(Presets {
            active: pseudo.id,
            presets: vec![pseudo],
        });
    }

    let base_item = FilterPreset {
        id: "filters.preset_base_item",
        filters: exact_filters(true)?,
        stats: create_exact_stat_filters(data, item, &item.stats_by_type, range, Mode::Default)?,
    };
    Ok(Presets {
        active: pseudo.id,
        presets: vec![pseudo, base_item],
    })
}
