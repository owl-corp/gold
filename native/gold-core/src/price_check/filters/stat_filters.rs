//! Stat filters: one per modifier of the item.
//!
//! Not ported yet: the special rules for mercenary warrants, Atzoatl, mirrored tablets and
//! timeless jewels.
use super::pseudo::{
    filter_base_percentile, filter_equipment_props, filter_memory_strands, filter_pseudo, BASE_PCTL_AFFECTED_IDS,
};
use super::rules::{apply_flask_hybrid_mod, apply_heist_rules, chart_props, map_props, valdo_bad_mods};
use super::util::{percent_roll, percent_roll_delta, round_roll};
use super::*;
use crate::data::GameData;
use crate::parser::calc_q20::{get_prop_quality, QUALITY_CHANGING_ENCHANT};
use crate::parser::item::{ItemRarity, ParsedItem};
use crate::parser::meta::{is, is_jewellery, known, ItemCategory};
use crate::parser::modifiers::{
    stat_sources_total, translate_stat_with_roll, ModifierType, StatCalculated, StatRoll, SumMode,
};
use anyhow::Result;

pub struct Ctx<'a> {
    pub data: &'a GameData,
    pub item: &'a ParsedItem,
    pub search_in_range: f64,
    pub filters: Vec<StatFilter>,
    pub stats_by_type: Vec<StatCalculated>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Default,
    Props,
    Bulk,
}

fn type_tag(t: ModifierType) -> FilterTag {
    FilterTag(t.as_str())
}

/// `createExactStatFilters`: every stat enabled, for items sold by their exact modifiers.
pub fn create_exact_stat_filters(
    data: &GameData,
    item: &ParsedItem,
    stats_by_type: &[StatCalculated],
    search_stat_range: f64,
    mode: Mode,
) -> Result<Vec<StatFilter>> {
    let area_blighted = item
        .info
        .area
        .as_ref()
        .and_then(|a| a.get("blighted"))
        .and_then(|b| b.as_bool())
        == Some(true);
    if area_blighted || is(&item.category, ItemCategory::Invitation) {
        return Ok(vec![]);
    }

    use ItemCategory::*;
    let cat = known(&item.category);
    let mut keep = vec![
        ModifierType::Pseudo,
        ModifierType::Fractured,
        ModifierType::Enchant,
        ModifierType::Necropolis,
        ModifierType::Imbued,
    ];

    if item.influences.is_empty() && item.is_fractured != Some(true) && !matches!(cat, Some(Tincture | Idol | Chart)) {
        keep.push(ModifierType::Implicit);
    }

    if item.rarity == Some(ItemRarity::Magic)
        && !matches!(
            cat,
            Some(ClusterJewel | Map | HeistContract | HeistBlueprint | Chart | Sentinel)
        )
    {
        keep.push(ModifierType::Explicit);
    } else if item.rarity == Some(ItemRarity::Rare) && cat == Some(Idol) {
        keep.push(ModifierType::Explicit);
    }

    if cat == Some(Flask) {
        keep.push(ModifierType::Crafted);
    }

    let mut ctx = Ctx {
        data,
        item,
        search_in_range: if mode != Mode::Props {
            search_stat_range.min(2.0)
        } else {
            search_stat_range
        },
        filters: vec![],
        stats_by_type: stats_by_type
            .iter()
            .filter(|c| keep.contains(&c.type_))
            .cloned()
            .collect(),
    };

    filter_base_percentile(&mut ctx)?;
    filter_memory_strands(&mut ctx, None)?;
    map_props(mode == Mode::Bulk, &mut ctx)?;
    chart_props(mode == Mode::Bulk, &mut ctx)?;
    valdo_bad_mods(&mut ctx)?;

    let converted = ctx
        .stats_by_type
        .iter()
        .map(|m| calculated_stat_to_filter(data, m, ctx.search_in_range, item))
        .collect::<Result<Vec<_>>>()?;
    ctx.filters.extend(converted);

    for filter in &mut ctx.filters {
        if filter.not == Some(true) {
            continue;
        }

        filter.hidden = None;

        if filter.tag == FilterTag::EXPLICIT {
            filter.disabled = !filter
                .sources
                .iter()
                .any(|s| s.modifier.info.tier.is_some_and(|t| t <= 2.0));
        } else if filter.tag != FilterTag::PROPERTY && filter.tag != FilterTag::PSEUDO {
            filter.disabled = false;
        }

        if filter.stat_ref == "# uses remaining" {
            if let Some(roll) = filter.roll.as_mut() {
                roll.min = Some(roll.value);
                roll.default = Bounds {
                    min: roll.value,
                    max: roll.value,
                };
            }
        }
    }

    if cat == Some(ClusterJewel) {
        apply_cluster_jewel_rules(&mut ctx.filters, true);
    }
    match cat {
        Some(HeistContract | HeistBlueprint) => apply_heist_rules(&mut ctx)?,
        Some(Flask) => {
            apply_flask_rules(&mut ctx.filters);
            apply_flask_hybrid_mod(&mut ctx)?;
        }
        Some(MemoryLine | SanctumRelic | Charm) => enable_all_filters(&mut ctx.filters),
        Some(Idol) => enable_good_rolled_filters(&mut ctx.filters, 0.66),
        _ => {}
    }

    Ok(ctx.filters)
}

/// `initUiModFilters`: the filters shown in the overlay, with most stats off by default.
pub fn init_ui_mod_filters(data: &GameData, item: &ParsedItem, search_stat_range: f64) -> Result<Vec<StatFilter>> {
    let mut ctx = Ctx {
        data,
        item,
        search_in_range: if item.rarity == Some(ItemRarity::Normal) {
            100.0
        } else {
            search_stat_range
        },
        filters: vec![],
        stats_by_type: item
            .stats_by_type
            .iter()
            .map(|calc| {
                if calc.type_ == ModifierType::Fractured && calc.stat.trade.ids.contains_key("explicit") {
                    StatCalculated {
                        type_: ModifierType::Explicit,
                        ..calc.clone()
                    }
                } else {
                    calc.clone()
                }
            })
            .collect(),
    };

    filter_equipment_props(&mut ctx)?;
    if item.rarity == Some(ItemRarity::Unique) {
        filter_base_percentile(&mut ctx)?;
    }
    filter_memory_strands(&mut ctx, Some("hide_memory_strands"))?;
    if item.info.ref_name != "Split Personality" {
        filter_pseudo(&mut ctx)?;
    }

    if !item.is_corrupted && item.is_mirrored != Some(true) {
        ctx.stats_by_type.retain(|m| m.type_ != ModifierType::Fractured);
        ctx.stats_by_type.extend(
            item.stats_by_type
                .iter()
                .filter(|m| m.type_ == ModifierType::Fractured)
                .cloned(),
        );
    }

    if item.is_veiled == Some(true) {
        ctx.stats_by_type.retain(|m| m.type_ != ModifierType::Veiled);
    }

    let converted = ctx
        .stats_by_type
        .iter()
        .map(|m| calculated_stat_to_filter(data, m, ctx.search_in_range, item))
        .collect::<Result<Vec<_>>>()?;
    ctx.filters.extend(converted);

    if item.is_veiled == Some(true) {
        for f in &mut ctx.filters {
            f.disabled = true;
        }
    }

    final_filter_tweaks(&mut ctx)?;

    Ok(ctx.filters)
}

fn decode_oils(calc: &StatCalculated) -> Option<Vec<&'static str>> {
    const OILS: [&str; 14] = [
        "Prismatic Oil",
        "Clear Oil",
        "Sepia Oil",
        "Amber Oil",
        "Verdant Oil",
        "Teal Oil",
        "Azure Oil",
        "Indigo Oil",
        "Violet Oil",
        "Crimson Oil",
        "Black Oil",
        "Opalescent Oil",
        "Silver Oil",
        "Golden Oil",
    ];
    if calc.type_ != ModifierType::Enchant {
        return None;
    }
    let anointments = calc.stat.anointments.as_ref()?.as_array()?;
    let encoded = if anointments.len() == 1 {
        anointments[0].get("oils")?.as_str()?
    } else {
        let roll = calc.sources.first()?.stat.roll.as_ref().map(|r| r.value);
        anointments
            .iter()
            .find(|a| a.get("roll").and_then(|r| r.as_f64()) == roll)?
            .get("oils")?
            .as_str()?
    };
    if encoded.is_empty() {
        return None;
    }
    let mut idx: Vec<f64> = encoded.split(',').map(crate::js::number).collect();
    idx.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)); // [Golden Oil ... Clear Oil]
    Some(idx.into_iter().filter_map(|i| OILS.get(i as usize).copied()).collect())
}

pub fn calculated_stat_to_filter(
    _data: &GameData,
    calc: &StatCalculated,
    mut percent: f64,
    item: &ParsedItem,
) -> Result<StatFilter> {
    let stat = &calc.stat;
    let sources = &calc.sources;
    let type_ = calc.type_;
    let trade_id = stat.trade.ids.get(type_.as_str()).cloned().unwrap_or_default();

    let mut option_filter: Option<StatFilter> = None;
    if stat.trade.option == Some(true) {
        let first = sources
            .first()
            .ok_or_else(|| anyhow::anyhow!("option stat without sources"))?;
        option_filter = Some(StatFilter {
            trade_id: trade_id.clone(),
            stat_ref: stat.ref_.clone(),
            text: first.stat.translation.string.clone(),
            tag: if type_ == ModifierType::Enchant {
                FilterTag::ENCHANT
            } else {
                FilterTag::VARIANT
            },
            oils: None,
            sources: sources.clone(),
            not: None,
            roll: None,
            option: Some(FilterOption {
                value: first
                    .contributes
                    .ok_or_else(|| anyhow::anyhow!("option without value"))?
                    .value,
            }),
            hidden: None,
            disabled: false,
        });
    }

    let mode = if item.info.ref_name == "Mirrored Tablet" {
        SumMode::Max
    } else {
        SumMode::Sum
    };
    let roll = stat_sources_total(&calc.sources, mode);
    let translation = translate_stat_with_roll(calc, roll)?;

    let mut filter = option_filter.unwrap_or_else(|| StatFilter {
        trade_id,
        stat_ref: stat.ref_.clone(),
        text: translation.string.clone(),
        tag: type_tag(type_),
        oils: decode_oils(calc),
        sources: sources.clone(),
        not: None,
        roll: None,
        option: None,
        hidden: None,
        disabled: true,
    });

    if stat.better == 0 {
        if type_ != ModifierType::Enchant {
            filter.tag = FilterTag::VARIANT;
        }
        if filter.oils.is_none() {
            filter.disabled = false;
        }
    }

    if type_ == ModifierType::Implicit || type_ == ModifierType::Explicit {
        if type_ == ModifierType::Explicit {
            if let Some(unique) = &item.info.unique {
                if let Some(fixed) = &unique.fixed_stats {
                    if !fixed.contains(&filter.stat_ref) {
                        filter.tag = FilterTag::VARIANT;
                    }
                } else if sources.iter().any(|s| s.modifier.info.generation.is_some()) {
                    // recognized unveiled mods are overridden below, this is kept as fallback
                    filter.tag = FilterTag::VARIANT;
                }
            }
        }

        if let Some(mechanic) = sources.iter().find_map(|s| s.modifier.info.mechanic) {
            filter.tag = FilterTag(mechanic.as_str());
        }
    }

    if let Some(roll) = roll.filter(|_| filter.option.is_none()) {
        if item.rarity == Some(ItemRarity::Magic)
            && (item.is_unmodifiable == Some(true) || item.is_corrupted || item.is_mirrored == Some(true))
        {
            percent = 0.0;
        } else if item.rarity == Some(ItemRarity::Unique)
            || (item.rarity == Some(ItemRarity::Magic) && is(&item.category, ItemCategory::Jewel))
            || sources
                .iter()
                .any(|s| s.modifier.info.tier == Some(1.0) && s.modifier.info.type_ == ModifierType::Fractured)
        {
            let perfect = (stat.better == 1 && roll.value >= roll.max) || (stat.better == -1 && roll.value <= roll.min);
            if perfect {
                percent = 0.0;
            }
        }

        let mut goodness: Option<f64> = None;
        if stat.better != 0 {
            let mut g = if roll.min == roll.max {
                1.0
            } else {
                (roll.value - roll.min) / (roll.max - roll.min)
            };
            if roll.min != roll.max && stat.better == -1 {
                g = 1.0 - g;
            }
            goodness = Some(g);
        }

        let dp = stat.dp == Some(true)
            || sources
                .iter()
                .any(|s| s.stat.stat.ref_ == stat.ref_ && s.stat.roll.as_ref().is_some_and(|r| r.dp));

        let bounds = Bounds {
            min: percent_roll(roll.min, -0.0, f64::floor, dp),
            max: percent_roll(roll.max, 0.0, f64::ceil, dp),
        };

        let mut default = if stat.better == 0 {
            Bounds {
                min: roll.value,
                max: roll.value,
            }
        } else if item.rarity == Some(ItemRarity::Unique) {
            Bounds {
                min: percent_roll_delta(roll.value, roll.max - roll.min, -percent, f64::floor, dp),
                max: percent_roll_delta(roll.value, roll.max - roll.min, percent, f64::ceil, dp),
            }
        } else {
            Bounds {
                min: percent_roll(roll.value, -percent, f64::floor, dp),
                max: percent_roll(roll.value, percent, f64::ceil, dp),
            }
        };
        default.min = crate::js::max(default.min, bounds.min);
        default.max = crate::js::min(default.max, bounds.max);

        let mut fr = FilterRoll {
            value: round_roll(roll.value, dp),
            min: None,
            max: None,
            default,
            bounds: (roll.min != roll.max && stat.better != 0).then_some(bounds),
            trade_invert: stat.trade.inverted,
            dp,
            is_negated: false,
            goodness,
        };

        filter_fill_min_max(&mut fr, stat.better);

        if translation.negate {
            filter_adjustment_for_negate(&mut fr);
        }
        filter.roll = Some(fr);
    }

    Ok(filter)
}

fn filter_fill_min_max(roll: &mut FilterRoll, better: i32) {
    match better {
        1 => roll.min = Some(roll.default.min),
        -1 => roll.max = Some(roll.default.max),
        0 => {
            roll.min = Some(roll.default.min);
            roll.max = Some(roll.default.max);
        }
        _ => {}
    }
}

fn filter_adjustment_for_negate(roll: &mut FilterRoll) {
    roll.trade_invert = Some(!roll.trade_invert.unwrap_or(false));
    roll.is_negated = true;
    let swap = roll.clone();

    if let (Some(sb), Some(b)) = (swap.bounds, roll.bounds.as_mut()) {
        b.min = -1.0 * sb.max;
        b.max = -1.0 * sb.min;
    }

    roll.default.min = -1.0 * swap.default.max;
    roll.default.max = -1.0 * swap.default.min;

    roll.value = -1.0 * swap.value;
    roll.min = swap.max.map(|m| -1.0 * m);
    roll.max = swap.min.map(|m| -1.0 * m);
}

fn hide_not_variable_stat(filter: &mut StatFilter, item: &ParsedItem) {
    if item.rarity != Some(ItemRarity::Unique)
        || (filter.tag != FilterTag::EXPLICIT && filter.tag != FilterTag::PROPERTY)
    {
        return;
    }

    // always show stat increased by Catalyst
    if item.quality.is_some_and(|q| q != 0.0)
        && known(&item.category).is_some_and(is_jewellery)
        && filter.sources.iter().any(|s| {
            s.modifier.info.roll_incr.is_some_and(|r| r != 0.0) && s.stat.roll.as_ref().is_some_and(|r| !r.unscalable)
        })
    {
        return;
    }

    // always show scalable stat on corrupted item
    let has_roll_and_scalable = filter
        .sources
        .iter()
        .any(|s| s.stat.roll.as_ref().is_some_and(|r| !r.unscalable));
    if item.is_corrupted && has_roll_and_scalable {
        let is_vvo = item.new_mods.iter().any(|m| {
            m.stats.iter().any(|s| {
                matches!(
                    s.roll.as_ref().and_then(|r| r.generation.as_ref()),
                    Some(crate::parser::stat_translations::RollGeneration::Volatile)
                )
            })
        });
        if !is_vvo {
            // some stat not being reduced compared to other VVO corrupted items can be important
            filter.disabled = false;
        }
        return;
    }

    match filter.roll.as_mut() {
        None => {
            filter.hidden = Some("filters.hide_const_roll".into());
            filter.disabled = true;
        }
        Some(roll) if roll.bounds.is_none() => {
            roll.min = None;
            roll.max = None;
            filter.hidden = Some("filters.hide_const_roll".into());
            filter.disabled = true;
        }
        Some(roll)
            if BASE_PCTL_AFFECTED_IDS.contains(&filter.trade_id.first().map(String::as_str).unwrap_or(""))
                && filter
                    .sources
                    .iter()
                    .all(|s| s.stat.roll.as_ref().map(|r| r.min) == s.stat.roll.as_ref().map(|r| r.max))
                && get_prop_quality(item) < 21.0 =>
        {
            roll.min = None;
            roll.max = None;
            filter.hidden = Some("hide_variable_by_base_percentile_only".into());
            filter.disabled = true;
        }
        Some(_) => {}
    }

    if item.is_foulborn == Some(true) && filter.tag == FilterTag::EXPLICIT {
        // some mod not being replaced with foulborn one can be important
        filter.disabled = false;
    }
}

fn final_filter_tweaks(ctx: &mut Ctx) -> Result<()> {
    let item = ctx.item;
    use ItemCategory::*;

    if is(&item.category, ClusterJewel) && item.rarity != Some(ItemRarity::Unique) {
        apply_cluster_jewel_rules(&mut ctx.filters, false);
    } else if is(&item.category, Flask) {
        apply_flask_rules(&mut ctx.filters);
        apply_flask_hybrid_mod(ctx)?;
    }

    if let Some(empty) = show_has_empty_modifier(item) {
        ctx.filters.push(StatFilter {
            trade_id: vec!["item.has_empty_modifier".into()],
            text: "1 Empty or Crafted Modifier".into(),
            stat_ref: "1 Empty or Crafted Modifier".into(),
            disabled: true,
            hidden: Some("filters.hide_empty_mod".into()),
            tag: FilterTag::PSEUDO,
            sources: vec![],
            option: Some(FilterOption { value: empty }),
            not: None,
            roll: None,
            oils: None,
        });
    }

    if matches!(known(&item.category), Some(Amulet | Ring)) {
        apply_anointment_rules(&mut ctx.filters, item);
    }

    for filter in &mut ctx.filters {
        hide_not_variable_stat(filter, item);

        if filter.tag == FilterTag::ENCHANT {
            if QUALITY_CHANGING_ENCHANT.contains(&filter.stat_ref.as_str()) {
                filter.hidden = Some("hide_enchant_meta_stat".into());
            }
        } else if filter.tag == FilterTag::FRACTURED {
            let has_explicit = item
                .stats_by_type
                .iter()
                .find(|m| m.stat.ref_ == filter.stat_ref)
                .is_some_and(|m| m.stat.trade.ids.contains_key("explicit"));
            if has_explicit {
                // hide only if fractured mod has corresponding explicit variant
                filter.hidden = Some("filters.hide_for_crafting".into());
            }
        } else if filter
            .sources
            .first()
            .is_some_and(|s| s.stat.stat.jewellery_quality.is_some())
        {
            filter.hidden = Some("hide_jewellery_quality".into());
        } else if filter.tag == FilterTag::IMPLICIT {
            if item.rarity == Some(ItemRarity::Unique) && !item.is_corrupted && !is(&item.category, Jewel) {
                // hide not Corrupted, Vestigial implicits etc., that were not consumed by pseudo stats
                filter.hidden = Some("hide_unique_base_implicit".into());
                filter.disabled = true;
            }
        } else if filter.tag == FilterTag::FOULBORN
            || filter.tag == FilterTag::VESTIGIAL
            || filter.tag == FilterTag::VARIANT
        {
            filter.disabled = false;
        }
    }

    if item.rarity == Some(ItemRarity::Unique) {
        let has_base_pctl = ctx
            .filters
            .iter()
            .any(|f| f.trade_id.first().map(String::as_str) == Some("item.base_percentile"));
        if has_base_pctl {
            let has_visible_armour = ctx.filters.iter().any(|f| {
                BASE_PCTL_AFFECTED_IDS.contains(&f.trade_id.first().map(String::as_str).unwrap_or(""))
                    && f.hidden.is_none()
            });
            if has_visible_armour {
                if let Some(bp) = ctx
                    .filters
                    .iter_mut()
                    .find(|f| f.trade_id.first().map(String::as_str) == Some("item.base_percentile"))
                {
                    bp.hidden = Some("hide_redundant".into());
                    bp.disabled = true;
                }
            }
        }

        let count_visible = ctx.filters.iter().filter(|f| f.hidden.is_none()).count();
        if count_visible <= 3 {
            enable_all_filters(&mut ctx.filters);
        }
    }
    Ok(())
}

fn apply_cluster_jewel_rules(filters: &mut [StatFilter], exact: bool) {
    for filter in filters {
        if filter.stat_ref == "# Added Passive Skills are Jewel Sockets" {
            filter.hidden = Some("filters.hide_const_roll".into());
            filter.disabled = true;
        }

        // https://www.poewiki.net/wiki/Cluster_Jewel#Optimal_passive_skill_amounts
        if filter.stat_ref == "Adds # Passive Skills" {
            filter.disabled = false;
            let Some(roll) = filter.roll.as_mut() else { continue };

            // 4 is [_, 5]
            if roll.value == 4.0 {
                roll.max = Some(5.0);
            // 5 is [5, 5] (and [_, 5] for Rare jewel)
            } else if roll.value == 5.0 && exact {
                roll.min = Some(roll.default.min);
            // 3, 6, 10, 11, 12 are [n, _]
            } else if [3.0, 6.0, 10.0, 11.0, 12.0].contains(&roll.value) {
                roll.min = Some(roll.default.min);
                roll.max = None;
            }
            // else 2, 5(Rare), 8, 9 are [_ , n]
        }
    }
}

fn apply_flask_rules(filters: &mut [StatFilter]) {
    let used_enkindling = filters
        .iter()
        .any(|f| f.stat_ref == "Gains no Charges during Flask Effect");
    for filter in filters {
        if filter.tag == FilterTag::ENCHANT && !used_enkindling {
            filter.hidden = Some("hide_harvest_and_instilling".into());
            filter.disabled = true;
        }
    }
}

fn apply_anointment_rules(filters: &mut [StatFilter], item: &ParsedItem) {
    let Some(anointment) = filters.iter_mut().find(|f| f.oils.is_some()) else {
        return;
    };

    if item.talisman_tier.is_some_and(|t| t != 0.0) {
        anointment.disabled = false;
    } else if !item.is_corrupted && item.is_mirrored != Some(true) {
        let first = anointment.oils.as_ref().and_then(|o| o.first().copied());
        if first != Some("Golden Oil") && first != Some("Silver Oil") {
            anointment.hidden = Some("filters.hide_anointment".into());
            anointment.disabled = true;
        }
    }
}

// TODO
// +1 Prefix Modifier allowed
// -1 Suffix Modifier allowed
fn show_has_empty_modifier(item: &ParsedItem) -> Option<f64> {
    if item.rarity != Some(ItemRarity::Rare) || item.is_corrupted || item.is_mirrored == Some(true) {
        return None;
    }

    use crate::parser::advanced::Generation;
    let random_mods: Vec<_> = item
        .new_mods
        .iter()
        .filter(|m| {
            matches!(
                m.info.type_,
                ModifierType::Explicit | ModifierType::Fractured | ModifierType::Veiled | ModifierType::Crafted
            )
        })
        .collect();
    let crafted = random_mods.iter().find(|m| m.info.type_ == ModifierType::Crafted);

    if (random_mods.len() == 5 && crafted.is_none()) || (random_mods.len() == 6 && crafted.is_some()) {
        let mut prefixes = random_mods
            .iter()
            .filter(|m| m.info.generation == Some(Generation::Prefix))
            .count() as i32;
        let mut suffixes = random_mods
            .iter()
            .filter(|m| m.info.generation == Some(Generation::Suffix))
            .count() as i32;

        if let Some(c) = crafted {
            if c.info.generation == Some(Generation::Prefix) {
                prefixes -= 1;
            } else {
                suffixes -= 1;
            }
        }

        if prefixes == 2 {
            return Some(HAS_EMPTY_MODIFIER_PREFIX);
        }
        if suffixes == 2 {
            return Some(HAS_EMPTY_MODIFIER_SUFFIX);
        }
    }

    None
}

pub fn enable_all_filters(filters: &mut [StatFilter]) {
    for f in filters {
        if f.hidden.is_none() {
            f.disabled = false;
        }
    }
}

fn enable_good_rolled_filters(filters: &mut [StatFilter], above_pct: f64) {
    for f in filters {
        if f.hidden.is_some() {
            continue;
        }
        match f.roll.as_ref().and_then(|r| r.goodness) {
            None => f.disabled = false,
            Some(g) if g >= above_pct => f.disabled = false,
            Some(_) => {}
        }
    }
}

/// Used by the item property filters: a roll with only a value and bounds.
pub fn stat_roll(value: f64, min: f64, max: f64) -> StatRoll {
    StatRoll { value, min, max }
}
