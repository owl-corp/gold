//! The trade site search request.
//!
//! Not ported yet: "not"/"one of" filter groups.
use super::category_to_trade_id;
use crate::data::{GameData, StatOrGroup};
use crate::price_check::filters::{ItemFilters, StatFilter, INTERNAL_TRADE_IDS};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

/// A JSON number the way JavaScript would print it.
fn num(x: f64) -> Value {
    if x.is_finite() && x == x.trunc() && x.abs() < 1e15 {
        json!(x as i64)
    } else {
        json!(x)
    }
}

/// `dot-prop`'s `setProperty`: creates the objects along `path`, and sets the leaf unless
/// `value` is None (an undefined leaf still leaves the objects behind).
fn set(root: &mut Value, path: &str, value: Option<Value>) {
    let mut node = root;
    let mut parts = path.split('.').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            if let Some(v) = value {
                node[part] = v;
            }
            return;
        }
        if !node[part].is_object() {
            node[part] = json!({});
        }
        node = &mut node[part];
    }
}

fn set_num(root: &mut Value, path: &str, value: f64) {
    set(root, path, Some(num(value)));
}

fn set_bool(root: &mut Value, path: &str, value: bool) {
    set(root, path, Some(json!(value.to_string())));
}

fn name_to_query(name: &str, discriminator: Option<&str>) -> Value {
    match discriminator {
        Some(d) if !d.is_empty() => json!({ "discriminator": d, "option": name }),
        _ => json!(name),
    }
}

fn pseudo_id(data: &GameData, ref_: &str) -> Result<String> {
    crate::price_check::filters::pseudo::pseudo_stat(data, ref_)
        .and_then(|s| s.trade.ids.get("pseudo").and_then(|ids| ids.first().cloned()))
        .ok_or_else(|| anyhow!("missing pseudo stat {ref_}"))
}

/// `getMinMax`
fn min_max(roll: Option<&crate::price_check::filters::FilterRoll>, divisor: f64) -> (Option<f64>, Option<f64>) {
    let Some(roll) = roll else { return (None, None) };
    let invert = roll.trade_invert == Some(true);
    let sign = if invert { -1.0 } else { 1.0 };
    let a = roll.min.map(|m| m * sign / divisor);
    let b = roll.max.map(|m| m * sign / divisor);
    if invert {
        (b, a)
    } else {
        (a, b)
    }
}

/// What the property-style trade ids (armour, dps, ...) write into the request:
/// the filter path, and a default for a missing minimum.
fn internal_target(id: &str) -> Option<(&'static str, Option<f64>)> {
    Some(match id {
        "item.base_percentile" => ("armour_filters.filters.base_defence_percentile", None),
        "item.memory_strands" => ("misc_filters.filters.memory_level", None),
        "item.armour" => ("armour_filters.filters.ar", None),
        "item.evasion_rating" => ("armour_filters.filters.ev", None),
        "item.energy_shield" => ("armour_filters.filters.es", None),
        "item.ward" => ("armour_filters.filters.ward", None),
        "item.block" => ("armour_filters.filters.block", None),
        "item.total_dps" => ("weapon_filters.filters.dps", None),
        "item.physical_dps" => ("weapon_filters.filters.pdps", None),
        "item.elemental_dps" => ("weapon_filters.filters.edps", None),
        "item.crit" => ("weapon_filters.filters.crit", None),
        "item.aps" => ("weapon_filters.filters.aps", None),
        "item.map_item_quantity" => ("map_filters.filters.map_iiq", None),
        "item.map_item_rarity" => ("map_filters.filters.map_iir", None),
        "item.map_pack_size" => ("map_filters.filters.map_packsize", None),
        "item.heist_job_agility" => ("heist_filters.filters.heist_agility", Some(1.0)),
        "item.heist_job_bruteforce" => ("heist_filters.filters.heist_brute_force", Some(1.0)),
        "item.heist_job_counterthaumaturgy" => ("heist_filters.filters.heist_counter_thaumaturgy", Some(1.0)),
        "item.heist_job_deception" => ("heist_filters.filters.heist_deception", Some(1.0)),
        "item.heist_job_demolition" => ("heist_filters.filters.heist_demolition", Some(1.0)),
        "item.heist_job_engineering" => ("heist_filters.filters.heist_engineering", Some(1.0)),
        "item.heist_job_lockpicking" => ("heist_filters.filters.heist_lockpicking", Some(1.0)),
        "item.heist_job_perception" => ("heist_filters.filters.heist_perception", Some(1.0)),
        "item.heist_job_trapdisarmament" => ("heist_filters.filters.heist_trap_disarmament", Some(1.0)),
        "item.heist_wings_revealed" => ("heist_filters.filters.heist_wings", None),
        "item.heist_wings_total" => ("heist_filters.filters.heist_max_wings", None),
        "item.chart_sulphur" => ("map_filters.filters.chart_sulphur", None),
        _ => return None,
    })
}

/// One entry of a stat group, from a trade id and the filter's numbers.
fn trade_id_to_query(id: &str, stat: &StatFilter) -> Value {
    let mut roll = stat.roll.as_ref();

    let div: f64 = if id.starts_with("{div_by_100}") { 100.0 } else { 1.0 };
    if id.starts_with("{empty}") || (id.starts_with("{empty_if_100}") && roll.is_some_and(|r| r.value == 100.0)) {
        roll = None;
    }
    let id = if id.starts_with('{') {
        id.split_once('}').map(|(_, rest)| rest).unwrap_or(id)
    } else {
        id
    };

    let (min, max) = min_max(roll, div);
    let mut value = Map::new();
    if let Some(m) = min {
        value.insert("min".into(), num(m));
    }
    if let Some(m) = max {
        value.insert("max".into(), num(m));
    }
    if let Some(o) = &stat.option {
        value.insert("option".into(), num(o.value));
    }
    json!({ "id": id, "value": value, "disabled": stat.disabled })
}

fn every_trade_id_to_query(stat: &StatFilter) -> Vec<Value> {
    stat.trade_id.iter().map(|id| trade_id_to_query(id, stat)).collect()
}

fn append_and_filter(stat: &StatFilter, and_group: &mut Value, all_groups: &mut Vec<Value>) {
    if stat.trade_id.len() == 1 {
        and_group["filters"]
            .as_array_mut()
            .unwrap()
            .push(trade_id_to_query(&stat.trade_id[0], stat));
    } else {
        all_groups.push(json!({
            "type": "count",
            "value": { "min": 1 },
            "disabled": stat.disabled,
            "filters": every_trade_id_to_query(stat),
        }));
    }
}

pub fn create_trade_request(data: &GameData, filters: &ItemFilters, stats: &[StatFilter]) -> Result<Value> {
    let trade = &filters.trade;
    let mut query = json!({
        "status": { "option": if trade.offline { "any" } else if trade.merchant_only { "securable" } else { "available" } },
        "stats": [ { "type": "and", "filters": [] } ],
        "filters": {},
    });
    let mut all_stats: Vec<Value> = vec![query["stats"][0].clone()];
    let mut q_filters = json!({});

    if let Some(currency) = &trade.currency {
        set(
            &mut q_filters,
            "trade_filters.filters.price.option",
            Some(json!(currency)),
        );
    }
    if trade.collapse_listings == "api" && (trade.offline || !trade.merchant_only || trade.collapse_merchant) {
        set_bool(&mut q_filters, "trade_filters.filters.collapse.option", true);
    }
    if let Some(listed) = &trade.listed {
        set(
            &mut q_filters,
            "trade_filters.filters.indexed.option",
            Some(json!(listed)),
        );
    }

    let mut active = match &filters.search_relaxed {
        Some(r) if r.disabled != Some(true) => r,
        _ => &filters.search_exact,
    };
    if let Some(sub) = active.sub.as_deref() {
        if sub.disabled != Some(true) {
            active = sub;
        }
    }

    let disc = active.discriminator_trade.as_deref();
    if let Some(name) = active.name_trade.as_deref().or(active.name.as_deref()) {
        query["name"] = name_to_query(name, disc);
    }
    if let Some(base) = active.base_type_trade.as_deref().or(active.base_type.as_deref()) {
        query["type"] = name_to_query(base, disc);
    }

    if filters.foil.as_ref().is_some_and(|f| !f.disabled) {
        set(
            &mut q_filters,
            "type_filters.filters.rarity.option",
            Some(json!("uniquefoil")),
        );
    } else if let Some(r) = &filters.rarity {
        set(
            &mut q_filters,
            "type_filters.filters.rarity.option",
            Some(json!(if r.disabled { "nonunique" } else { r.value })),
        );
    }

    if let Some(category) = &active.category {
        let id = category
            .known()
            .and_then(category_to_trade_id)
            .ok_or_else(|| anyhow!("Invalid category: {category:?}"))?;
        set(&mut q_filters, "type_filters.filters.category.option", Some(json!(id)));
    }

    if let Some(c) = &filters.corrupted {
        if !c.value || c.exact == Some(true) {
            set_bool(&mut q_filters, "misc_filters.filters.corrupted.option", c.value);
        }
    }
    if filters.fractured.as_ref().is_some_and(|f| !f.value) {
        set_bool(&mut q_filters, "misc_filters.filters.fractured_item.option", false);
    }
    if filters.imbued_gem.as_ref().is_some_and(|f| f.disabled) {
        set_bool(&mut q_filters, "misc_filters.filters.gem_imbued.option", false);
    }
    if filters.split.as_ref().is_some_and(|f| f.disabled) {
        set_bool(&mut q_filters, "misc_filters.filters.split.option", false);
    }
    if filters.foulborn.as_ref().is_some_and(|f| !f.value) {
        set_bool(&mut q_filters, "misc_filters.filters.foulborn_item.option", false);
    }
    if filters.vestigial.as_ref().is_some_and(|f| !f.value) {
        set_bool(&mut q_filters, "misc_filters.filters.vestigial.option", false);
    }
    if filters.mirrored.as_ref().is_some_and(|f| f.disabled) {
        set_bool(&mut q_filters, "misc_filters.filters.mirrored.option", false);
    }

    if let Some(f) = filters.gem_level.as_ref().filter(|f| !f.disabled) {
        set_num(&mut q_filters, "misc_filters.filters.gem_level.min", f.value);
    }
    if let Some(f) = filters.quality.as_ref().filter(|f| !f.disabled) {
        set_num(&mut q_filters, "misc_filters.filters.quality.min", f.value);
    }
    if let Some(f) = filters.item_level.as_ref().filter(|f| !f.disabled) {
        set_num(&mut q_filters, "misc_filters.filters.ilvl.min", f.value);
        if let Some(max) = f.max.filter(|m| *m != 0.0) {
            set_num(&mut q_filters, "misc_filters.filters.ilvl.max", max);
        }
    }
    if let Some(f) = filters.stack_size.as_ref().filter(|f| !f.disabled) {
        set_num(&mut q_filters, "misc_filters.filters.stack_size.min", f.value);
    }
    if let Some(f) = filters.linked_sockets.as_ref().filter(|f| !f.disabled) {
        set_num(&mut q_filters, "socket_filters.filters.links.min", f.value);
    }
    if let Some(f) = filters.white_sockets.as_ref().filter(|f| !f.disabled) {
        set_num(&mut q_filters, "socket_filters.filters.sockets.w", f.value);
    }
    if let Some(f) = filters.map_tier.as_ref().filter(|f| !f.disabled) {
        set_num(&mut q_filters, "map_filters.filters.map_tier.min", f.value);
        set_num(&mut q_filters, "map_filters.filters.map_tier.max", f.value);
    }

    if let Some(b) = &filters.map_blighted {
        match b.value.as_str() {
            Some("Blighted") => set_bool(&mut q_filters, "map_filters.filters.map_blighted.option", true),
            Some("Blight-ravaged") => set_bool(&mut q_filters, "map_filters.filters.map_uberblighted.option", true),
            _ if b.value == json!(false) => {
                set_bool(&mut q_filters, "map_filters.filters.map_blighted.option", false);
                set_bool(&mut q_filters, "map_filters.filters.map_uberblighted.option", false);
            }
            _ => {}
        }
    }
    if let Some(r) = &filters.map_completion_reward {
        set(
            &mut q_filters,
            "map_filters.filters.map_completion_reward.option",
            Some(json!(r.name_trade)),
        );
    }
    if filters.unidentified.as_ref().is_some_and(|f| !f.disabled) {
        set_bool(&mut q_filters, "misc_filters.filters.identified.option", false);
    }
    if let Some(f) = filters.area_level.as_ref().filter(|f| !f.disabled) {
        set_num(&mut q_filters, "map_filters.filters.area_level.min", f.value);
        if let Some(max) = f.max.filter(|m| *m != 0.0) {
            set_num(&mut q_filters, "map_filters.filters.area_level.max", max);
        }
    }
    if let Some(f) = filters.sentinel_charge.as_ref().filter(|f| !f.disabled) {
        set_num(
            &mut q_filters,
            "sentinel_filters.filters.sentinel_durability.min",
            f.value,
        );
    }

    for stat in stats {
        let first = stat.trade_id.first().map(String::as_str).unwrap_or("");
        if !first.starts_with("item.") {
            continue;
        }

        if first == "item.has_empty_modifier" {
            let which = stat.option.as_ref().map(|o| o.value as usize).unwrap_or(0).min(2);
            let crafted = pseudo_id(
                data,
                [
                    "# Crafted Modifiers",
                    "# Crafted Prefix Modifiers",
                    "# Crafted Suffix Modifiers",
                ][which],
            )?;
            let empty = pseudo_id(
                data,
                [
                    "# Empty Modifiers",
                    "# Empty Prefix Modifiers",
                    "# Empty Suffix Modifiers",
                ][which],
            )?;
            let total = pseudo_id(data, "# Modifiers")?;

            all_stats.push(json!({
                "type": "count",
                "value": { "min": 1, "max": 1 },
                "disabled": stat.disabled,
                "filters": [
                    { "id": empty, "value": { "min": 1, "max": 1 }, "disabled": stat.disabled },
                    { "id": crafted, "value": { "min": 1 }, "disabled": stat.disabled },
                ],
            }));
            all_stats.push(json!({
                "type": "count",
                "value": { "min": 1, "max": 1 },
                "disabled": stat.disabled,
                "filters": [
                    { "id": empty, "value": { "min": 1, "max": 1 }, "disabled": stat.disabled },
                    { "id": total, "value": { "min": 6 }, "disabled": stat.disabled },
                ],
            }));
        }

        if stat.disabled {
            continue;
        }

        if first == "item.heist_target_priceless" {
            set(
                &mut q_filters,
                "heist_filters.filters.heist_objective_value.option",
                Some(json!("priceless")),
            );
        } else if let Some((path, default_min)) = internal_target(first) {
            let roll = stat
                .roll
                .as_ref()
                .ok_or_else(|| anyhow!("property filter without a roll"))?;
            set(
                &mut q_filters,
                &format!("{path}.min"),
                roll.min.or(default_min).map(num),
            );
            set(&mut q_filters, &format!("{path}.max"), roll.max.map(num));
        }
    }

    // stats that are real modifiers on the trade site
    let mut real: Vec<StatFilter> = stats
        .iter()
        .filter(|s| !INTERNAL_TRADE_IDS.contains(&s.trade_id.first().map(String::as_str).unwrap_or("")))
        .cloned()
        .collect();

    if let Some(veiled) = &filters.veiled {
        for stat_ref in &veiled.stat_refs {
            let found = data
                .stats
                .by_ref(stat_ref)
                .ok_or_else(|| anyhow!("unknown veiled stat {stat_ref}"))?;
            let db_stats = match found {
                StatOrGroup::Group(g) => g.stats,
                StatOrGroup::Stat(s) => vec![s],
            };
            real.push(bare_filter(
                veiled.disabled,
                db_stats
                    .iter()
                    .filter_map(|s| s.trade.ids.get("veiled").and_then(|i| i.first().cloned()))
                    .collect(),
            ));
        }
    }

    if let Some(influences) = &filters.influences {
        for influence in influences {
            let ref_ = format!("Has {:?} Influence", influence.value);
            let stat = crate::price_check::filters::pseudo::pseudo_stat(data, &ref_)
                .ok_or_else(|| anyhow!("missing pseudo stat {ref_}"))?;
            real.push(bare_filter(
                influence.disabled,
                stat.trade.ids.get("pseudo").cloned().unwrap_or_default(),
            ));
        }
    }

    let mut q_and = all_stats[0].clone();
    let mut q_not = json!({ "type": "not", "filters": [] });
    let mut groups: Vec<Value> = Vec::new();

    for stat in &real {
        if stat.not == Some(true) {
            q_not["filters"]
                .as_array_mut()
                .unwrap()
                .extend(every_trade_id_to_query(stat));
        } else {
            append_and_filter(stat, &mut q_and, &mut groups);
        }
    }

    let mut stat_groups = vec![q_and];
    stat_groups.extend(all_stats.into_iter().skip(1));
    stat_groups.extend(groups);
    if !q_not["filters"].as_array().unwrap().is_empty() {
        stat_groups.push(q_not);
    }

    query["stats"] = Value::Array(stat_groups);
    query["filters"] = q_filters;
    if query["type"].is_null() {
        query.as_object_mut().unwrap().remove("type");
    }
    if query["name"].is_null() {
        query.as_object_mut().unwrap().remove("name");
    }

    if query["stats"].as_array().is_none() {
        bail!("invalid stats");
    }
    Ok(json!({ "query": query, "sort": { "price": "asc" } }))
}

fn bare_filter(disabled: bool, trade_id: Vec<String>) -> StatFilter {
    use crate::price_check::filters::FilterTag;
    StatFilter {
        trade_id,
        stat_ref: String::new(),
        text: String::new(),
        tag: FilterTag::PSEUDO,
        oils: None,
        sources: vec![],
        not: None,
        roll: None,
        option: None,
        hidden: None,
        disabled,
    }
}
