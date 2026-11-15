//! Rules for maps, charts, heists and flasks.
use super::pseudo::{no_source_pseudo_to_filter, prop_to_filter, prop_to_filter_in_range, pseudo_stat, PropFilter};
use super::stat_filters::Ctx;
use super::*;
use crate::data::{Stat, StatGroup, StatGroupResolver, StatOrGroup};
use crate::parser::item::ItemRarity;
use crate::parser::meta::{is, Category, ItemCategory};
use crate::parser::modifiers::{ModifierType, StatRoll};
use crate::parser::stat_translations::resolve_translation;
use anyhow::{anyhow, bail, Result};

const MAX_SAFE_INTEGER: f64 = 9007199254740991.0;

/// `findAndResolveByRef` without a hint: groups that merge several stats can't be resolved.
pub fn find_and_resolve_by_ref(
    ctx: &Ctx,
    name: &str,
    mod_type: ModifierType,
    category: Option<&Category>,
) -> Result<Stat> {
    let found = ctx
        .data
        .stats
        .by_ref(name)
        .ok_or_else(|| anyhow!("Unexpected stat shape: {name}"))?;
    let stat = match found {
        StatOrGroup::Stat(s) => Some(s),
        StatOrGroup::Group(g) => match g.resolve {
            StatGroupResolver::Select { .. } | StatGroupResolver::TrivialMerge => resolve_translation(
                StatGroup {
                    resolve: g.resolve,
                    stats: g.stats,
                    extra: g.extra,
                },
                "",
                mod_type,
                category,
                None,
            ),
            _ => None,
        },
    };
    stat.filter(|s| s.trade.ids.contains_key(mod_type.as_str()))
        .ok_or_else(|| anyhow!("Unexpected stat shape: {name}"))
}

/// `statToNotFilter`: a filter that excludes listings having the stat.
fn stat_to_not_filter(stat: &Stat, type_: ModifierType, negate_string: bool, disabled: bool) -> StatFilter {
    let matcher = stat
        .matchers
        .iter()
        .find(|m| m.negate.unwrap_or(false) == negate_string)
        .unwrap_or(&stat.matchers[0]);
    StatFilter {
        trade_id: stat.trade.ids.get(type_.as_str()).cloned().unwrap_or_default(),
        stat_ref: stat.ref_.clone(),
        text: matcher.string.clone(),
        tag: FilterTag(type_.as_str()),
        oils: None,
        sources: vec![],
        not: Some(true),
        roll: None,
        option: None,
        hidden: None,
        disabled,
    }
}

fn big_roll(value: f64) -> StatRoll {
    StatRoll {
        value,
        min: 0.0,
        max: MAX_SAFE_INTEGER,
    }
}

fn truthy(v: Option<f64>) -> Option<f64> {
    v.filter(|v| *v != 0.0 && !v.is_nan())
}

struct AreaDisabled {
    item_quantity: bool,
    item_rarity: bool,
    pack_size: bool,
}

fn area_props(ctx: &mut Ctx, disabled: AreaDisabled) -> Result<()> {
    let item = ctx.item;
    for (value, ref_, trade_id, off) in [
        (
            item.area_item_quantity,
            "Item Quantity: +#%",
            "item.map_item_quantity",
            disabled.item_quantity,
        ),
        (
            item.area_item_rarity,
            "Item Rarity: +#%",
            "item.map_item_rarity",
            disabled.item_rarity,
        ),
        (
            item.area_pack_size,
            "Monster Pack Size: +#%",
            "item.map_pack_size",
            disabled.pack_size,
        ),
    ] {
        if let Some(v) = truthy(value) {
            let f = prop_to_filter(
                ctx,
                PropFilter {
                    ref_,
                    trade_id,
                    roll: Some(big_roll(v)),
                    sources: vec![],
                    dp: false,
                    disabled: Some(off),
                    hidden: None,
                },
            )?;
            ctx.filters.push(f);
        }
    }
    Ok(())
}

pub fn map_props(bulk: bool, ctx: &mut Ctx) -> Result<()> {
    let item = ctx.item;
    if !is(&item.category, ItemCategory::Map)
        || item.map_completion_reward.is_some()
        || item.rarity == Some(ItemRarity::Unique)
    {
        return Ok(());
    }

    if !bulk {
        let has_valuable_drops = truthy(item.map_more_currency).is_some() || truthy(item.map_more_div_cards).is_some();

        area_props(
            ctx,
            AreaDisabled {
                item_quantity: false,
                item_rarity: has_valuable_drops,
                pack_size: false,
            },
        )?;

        for (value, ref_, disabled) in [
            (item.map_more_maps, "More Maps: #%", true),
            (item.map_more_currency, "More Currency: #%", false),
            (item.map_more_div_cards, "More Divination Cards: #%", false),
        ] {
            if let Some(v) = truthy(value) {
                let pseudo = pseudo_stat(ctx.data, ref_).ok_or_else(|| anyhow!("missing pseudo stat {ref_}"))?;
                let f = no_source_pseudo_to_filter(
                    ctx,
                    ctx.search_in_range,
                    pseudo,
                    Some(big_roll(v)),
                    false,
                    Some(disabled),
                    None,
                )?;
                ctx.filters.push(f);
            }
        }
    }

    use crate::parser::advanced::Generation;
    let explicit_mods = item
        .new_mods
        .iter()
        .filter(|m| matches!(m.info.generation, Some(Generation::Prefix | Generation::Suffix)))
        .count();
    if explicit_mods == 8 {
        let pseudo = pseudo_stat(ctx.data, "# Modifiers").ok_or_else(|| anyhow!("missing pseudo stat # Modifiers"))?;
        let roll = StatRoll {
            value: explicit_mods as f64,
            min: 0.0,
            max: 8.0,
        };
        let f = no_source_pseudo_to_filter(ctx, 0.0, pseudo, Some(roll), false, Some(false), None)?;
        ctx.filters.push(f);
    }
    Ok(())
}

pub fn chart_props(bulk: bool, ctx: &mut Ctx) -> Result<()> {
    let item = ctx.item;
    if bulk || !is(&item.category, ItemCategory::Chart) {
        return Ok(());
    }

    area_props(
        ctx,
        AreaDisabled {
            item_quantity: false,
            item_rarity: false,
            pack_size: false,
        },
    )?;

    if let Some(v) = truthy(item.chart_sulphur) {
        let f = prop_to_filter(
            ctx,
            PropFilter {
                ref_: "Dead Man's Sulphur: +#%",
                trade_id: "item.chart_sulphur",
                roll: Some(big_roll(v)),
                sources: vec![],
                dp: false,
                disabled: Some(false),
                hidden: None,
            },
        )?;
        ctx.filters.push(f);
    }
    Ok(())
}

pub fn valdo_bad_mods(ctx: &mut Ctx) -> Result<()> {
    if ctx.item.map_completion_reward.is_none() {
        return Ok(());
    }
    for lethal in ["Players who Die in area are sent to the Void"] {
        if ctx.item.stats_by_type.iter().any(|c| c.stat.ref_ == lethal) {
            continue;
        }
        let stat = find_and_resolve_by_ref(ctx, lethal, ModifierType::Explicit, ctx.item.category.as_ref())?;
        let filter = stat_to_not_filter(&stat, ModifierType::Explicit, false, false);
        ctx.filters.push(filter);
    }
    Ok(())
}

pub fn apply_heist_rules(ctx: &mut Ctx) -> Result<()> {
    apply_contract_rules(ctx)?;
    apply_blueprint_rules(ctx)
}

fn apply_contract_rules(ctx: &mut Ctx) -> Result<()> {
    let item = ctx.item;
    if !is(&item.category, ItemCategory::HeistContract) || item.rarity == Some(ItemRarity::Unique) {
        return Ok(());
    }

    if let Some(contract) = &item.heist_contract {
        if let (Some(job), Some(level)) = (contract.required_job, truthy(contract.job_level)) {
            let id = match job {
                "Lockpicking" => "item.heist_job_lockpicking",
                "Brute Force" => "item.heist_job_bruteforce",
                "Perception" => "item.heist_job_perception",
                "Demolition" => "item.heist_job_demolition",
                "Counter-Thaumaturgy" => "item.heist_job_counterthaumaturgy",
                "Trap Disarmament" => "item.heist_job_trapdisarmament",
                "Agility" => "item.heist_job_agility",
                "Deception" => "item.heist_job_deception",
                "Engineering" => "item.heist_job_engineering",
                other => bail!("unknown heist job {other}"),
            };
            let f = prop_to_filter_in_range(
                ctx,
                0.0,
                PropFilter {
                    ref_: id,
                    trade_id: id,
                    roll: Some(big_roll(level)),
                    sources: vec![],
                    dp: false,
                    disabled: Some(false),
                    hidden: None,
                },
            )?;
            ctx.filters.push(f);
        }

        if contract.target_value == Some("Priceless") {
            let f = prop_to_filter(
                ctx,
                PropFilter {
                    ref_: "Heist Target: Priceless",
                    trade_id: "item.heist_target_priceless",
                    roll: None,
                    sources: vec![],
                    dp: false,
                    disabled: Some(false),
                    hidden: None,
                },
            )?;
            ctx.filters.push(f);
        }
    }
    Ok(())
}

fn apply_blueprint_rules(ctx: &mut Ctx) -> Result<()> {
    let item = ctx.item;
    if !is(&item.category, ItemCategory::HeistBlueprint) || item.rarity == Some(ItemRarity::Unique) {
        return Ok(());
    }

    if !ctx.filters.iter().any(|f| f.tag == FilterTag::ENCHANT) {
        let stat = pseudo_stat(ctx.data, "# Enchant Modifiers").ok_or_else(|| anyhow!("missing pseudo stat"))?;
        ctx.filters
            .push(stat_to_not_filter(&stat, ModifierType::Pseudo, false, false));
    }

    let blueprint = item
        .heist_blueprint
        .as_ref()
        .ok_or_else(|| anyhow!("blueprint without data"))?;
    for (value, ref_, id) in [
        (
            blueprint.wings_revealed,
            "Wings Revealed: #",
            "item.heist_wings_revealed",
        ),
        (blueprint.wings_total, "Total Wings: #", "item.heist_wings_total"),
    ] {
        let value = value.ok_or_else(|| anyhow!("blueprint without wings"))?;
        let f = prop_to_filter_in_range(
            ctx,
            0.0,
            PropFilter {
                ref_,
                trade_id: id,
                roll: Some(big_roll(value)),
                sources: vec![],
                dp: false,
                disabled: Some(false),
                hidden: None,
            },
        )?;
        ctx.filters.push(f);
    }
    Ok(())
}

pub fn apply_flask_hybrid_mod(ctx: &mut Ctx) -> Result<()> {
    const INCR_CHARGE_RECOVERY: &str = "#% increased Charge Recovery";
    const INCR_EFFECT: &str = "#% increased effect";

    let applicable = ctx.item.rarity == Some(ItemRarity::Magic)
        && ctx.filters.iter().any(|f| f.stat_ref == INCR_CHARGE_RECOVERY)
        && !ctx.filters.iter().any(|f| f.stat_ref == INCR_EFFECT);
    if !applicable {
        return Ok(());
    }

    let effect = find_and_resolve_by_ref(ctx, INCR_EFFECT, ModifierType::Explicit, ctx.item.category.as_ref())?;
    let filter = stat_to_not_filter(&effect, ModifierType::Explicit, true, false);
    let insert_after = ctx
        .filters
        .iter()
        .position(|f| f.stat_ref == INCR_CHARGE_RECOVERY)
        .unwrap_or(0);
    ctx.filters.insert(insert_after + 1, filter);
    Ok(())
}
