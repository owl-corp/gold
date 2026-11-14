//! Pseudo stats (total resistances, total life, ...) and item property filters
//! (armour, dps, ...).
use super::stat_filters::{calculated_stat_to_filter, Ctx};
use super::*;
use crate::data::{GameData, Stat, StatGroup, StatMatcher, StatOrGroup, StatTrade};
use crate::parser::calc_q20::{self, calc_prop_bounds, prop_at_20_quality, QualityStats};
use crate::parser::item::{ItemRarity, ParsedItem};
use crate::parser::meta::{is_armour, is_weapon, known, ItemCategory};
use crate::parser::modifiers::{ModifierType, StatCalculated, StatRoll, StatSource};
use crate::parser::stat_translations::{ParsedStat, Roll};
use anyhow::{anyhow, Result};
use std::collections::BTreeMap;

pub const BASE_PCTL_AFFECTED_IDS: [&str; 4] = ["item.armour", "item.evasion_rating", "item.energy_shield", "item.ward"];

/// `pseudoStatByRef`
pub fn pseudo_stat(data: &GameData, ref_: &str) -> Option<Stat> {
    match data.stats.by_ref(ref_)? {
        StatOrGroup::Group(StatGroup { stats, .. }) => stats
            .into_iter()
            .find(|s| s.ref_ == ref_ && s.trade.ids.contains_key("pseudo")),
        StatOrGroup::Stat(s) => Some(s),
    }
}

struct ResInfo {
    ref_: &'static str,
    elements: &'static [&'static str],
    chaos: bool,
}

const RESISTANCES: &[ResInfo] = &[
    ResInfo {
        ref_: "+#% to All Resistances",
        elements: &["fire", "cold", "lightning"],
        chaos: true,
    },
    ResInfo {
        ref_: "+#% to all Elemental Resistances",
        elements: &["fire", "cold", "lightning"],
        chaos: false,
    },
    ResInfo {
        ref_: "+#% to Fire Resistance",
        elements: &["fire"],
        chaos: false,
    },
    ResInfo {
        ref_: "+#% to Cold Resistance",
        elements: &["cold"],
        chaos: false,
    },
    ResInfo {
        ref_: "+#% to Lightning Resistance",
        elements: &["lightning"],
        chaos: false,
    },
    ResInfo {
        ref_: "+#% to Fire and Lightning Resistances",
        elements: &["fire", "lightning"],
        chaos: false,
    },
    ResInfo {
        ref_: "+#% to Fire and Cold Resistances",
        elements: &["fire", "cold"],
        chaos: false,
    },
    ResInfo {
        ref_: "+#% to Cold and Lightning Resistances",
        elements: &["cold", "lightning"],
        chaos: false,
    },
    ResInfo {
        ref_: "+#% to Chaos Resistance",
        elements: &[],
        chaos: true,
    },
    ResInfo {
        ref_: "+#% to Fire and Chaos Resistances",
        elements: &["fire"],
        chaos: true,
    },
    ResInfo {
        ref_: "+#% to Cold and Chaos Resistances",
        elements: &["cold"],
        chaos: true,
    },
    ResInfo {
        ref_: "+#% to Lightning and Chaos Resistances",
        elements: &["lightning"],
        chaos: true,
    },
];

struct AttrInfo {
    ref_: &'static str,
    attributes: &'static [&'static str],
}

const ATTRIBUTES: &[AttrInfo] = &[
    AttrInfo {
        ref_: "+# to all Attributes",
        attributes: &["str", "dex", "int"],
    },
    AttrInfo {
        ref_: "+# to Strength",
        attributes: &["str"],
    },
    AttrInfo {
        ref_: "+# to Dexterity",
        attributes: &["dex"],
    },
    AttrInfo {
        ref_: "+# to Intelligence",
        attributes: &["int"],
    },
    AttrInfo {
        ref_: "+# to Strength and Intelligence",
        attributes: &["str", "int"],
    },
    AttrInfo {
        ref_: "+# to Strength and Dexterity",
        attributes: &["str", "dex"],
    },
    AttrInfo {
        ref_: "+# to Dexterity and Intelligence",
        attributes: &["dex", "int"],
    },
];

#[derive(Clone)]
struct RuleStat {
    ref_: &'static str,
    multiplier: f64,
    required: bool,
    keep: bool,
}

fn rs(ref_: &'static str) -> RuleStat {
    RuleStat {
        ref_,
        multiplier: 1.0,
        required: false,
        keep: false,
    }
}

fn req(ref_: &'static str) -> RuleStat {
    RuleStat {
        required: true,
        ..rs(ref_)
    }
}

struct Rule {
    group: Option<&'static str>,
    pseudo: &'static str,
    disabled: Option<bool>,
    replaces: Option<&'static str>,
    stats: Vec<RuleStat>,
    chaos_res_mutation: bool,
}

fn rule(pseudo: &'static str, stats: Vec<RuleStat>) -> Rule {
    Rule {
        group: None,
        pseudo,
        disabled: None,
        replaces: None,
        stats,
        chaos_res_mutation: false,
    }
}

fn rules() -> Vec<Rule> {
    let res_with = |element: &'static str| -> Vec<RuleStat> {
        RESISTANCES
            .iter()
            .filter(|i| i.elements.contains(&element))
            .map(|i| RuleStat {
                keep: i.elements.len() > 1 || i.chaos,
                ..rs(i.ref_)
            })
            .collect()
    };
    let attr_with = |attr: &'static str| -> Vec<RuleStat> {
        ATTRIBUTES
            .iter()
            .filter(|i| i.attributes.contains(&attr))
            .map(|i| RuleStat {
                keep: i.attributes.len() > 1,
                ..rs(i.ref_)
            })
            .collect()
    };

    let mut out = vec![
        Rule {
            disabled: Some(false),
            ..rule(
                "+#% total Elemental Resistance",
                RESISTANCES
                    .iter()
                    .filter(|i| !i.elements.is_empty())
                    .map(|i| RuleStat {
                        multiplier: i.elements.len() as f64,
                        keep: i.chaos,
                        ..rs(i.ref_)
                    })
                    .collect(),
            )
        },
        Rule {
            group: Some("to_x_ele_res"),
            ..rule("+#% total to Fire Resistance", res_with("fire"))
        },
        Rule {
            group: Some("to_x_ele_res"),
            ..rule("+#% total to Cold Resistance", res_with("cold"))
        },
        Rule {
            group: Some("to_x_ele_res"),
            ..rule("+#% total to Lightning Resistance", res_with("lightning"))
        },
        Rule {
            chaos_res_mutation: true,
            ..rule(
                "+#% total to Chaos Resistance",
                RESISTANCES
                    .iter()
                    .filter(|i| i.chaos)
                    .map(|i| RuleStat {
                        keep: !i.elements.is_empty(),
                        ..rs(i.ref_)
                    })
                    .collect(),
            )
        },
        Rule {
            group: Some("to_all_attrs"),
            ..rule("+# total to all Attributes", vec![rs("+# to all Attributes")])
        },
        Rule {
            group: Some("to_x_attr"),
            ..rule("+# total to Strength", attr_with("str"))
        },
        Rule {
            group: Some("to_x_attr"),
            ..rule("+# total to Dexterity", attr_with("dex"))
        },
        Rule {
            group: Some("to_x_attr"),
            ..rule("+# total to Intelligence", attr_with("int"))
        },
    ];

    let mut life = vec![req("+# to maximum Life")];
    life.extend(
        ATTRIBUTES
            .iter()
            .filter(|i| i.attributes.contains(&"str"))
            .map(|i| RuleStat {
                multiplier: 5.0 / 10.0,
                keep: true,
                ..rs(i.ref_)
            }),
    );
    out.push(Rule {
        disabled: Some(false),
        ..rule("+# total maximum Life", life)
    });

    let mut mana = vec![req("+# to maximum Mana")];
    mana.extend(
        ATTRIBUTES
            .iter()
            .filter(|i| i.attributes.contains(&"int"))
            .map(|i| RuleStat {
                multiplier: 5.0 / 10.0,
                keep: true,
                ..rs(i.ref_)
            }),
    );
    out.push(rule("+# total maximum Mana", mana));

    out.extend([
        rule(
            "#% total increased maximum Energy Shield",
            vec![rs("#% increased maximum Energy Shield")],
        ),
        rule(
            "+# total maximum Energy Shield",
            vec![rs("+# to maximum Energy Shield")],
        ),
        rule("+#% total Attack Speed", vec![rs("#% increased Attack Speed")]),
        rule("+#% total Cast Speed", vec![rs("#% increased Cast Speed")]),
        rule("#% increased Movement Speed", vec![rs("#% increased Movement Speed")]),
        rule(
            "#% total increased Physical Damage",
            vec![rs("#% increased Global Physical Damage")],
        ),
        Rule {
            group: Some("global_crit_chance"),
            ..rule(
                "+#% Global Critical Strike Chance",
                vec![rs("#% increased Global Critical Strike Chance")],
            )
        },
        Rule {
            replaces: Some("global_crit_chance"),
            ..rule(
                "+#% total Critical Strike Chance for Spells",
                vec![
                    req("#% increased Spell Critical Strike Chance"),
                    rs("#% increased Global Critical Strike Chance"),
                ],
            )
        },
        rule(
            "+#% Global Critical Strike Multiplier",
            vec![rs("+#% to Global Critical Strike Multiplier")],
        ),
        Rule {
            group: Some("incr_ele_dmg"),
            ..rule(
                "#% increased Elemental Damage",
                vec![rs("#% increased Elemental Damage")],
            )
        },
        Rule {
            replaces: Some("incr_ele_dmg"),
            ..rule(
                "#% increased Lightning Damage",
                vec![
                    req("#% increased Lightning Damage"),
                    rs("#% increased Elemental Damage"),
                ],
            )
        },
        Rule {
            replaces: Some("incr_ele_dmg"),
            ..rule(
                "#% increased Cold Damage",
                vec![req("#% increased Cold Damage"), rs("#% increased Elemental Damage")],
            )
        },
        Rule {
            group: Some("incr_fire_dmg"),
            replaces: Some("incr_ele_dmg"),
            ..rule(
                "#% increased Fire Damage",
                vec![req("#% increased Fire Damage"), rs("#% increased Elemental Damage")],
            )
        },
        Rule {
            group: Some("incr_spell_dmg"),
            ..rule("#% increased Spell Damage", vec![rs("#% increased Spell Damage")])
        },
        Rule {
            replaces: Some("incr_spell_dmg"),
            ..rule(
                "#% increased Lightning Spell Damage",
                vec![
                    req("#% increased Lightning Spell Damage"),
                    rs("#% increased Spell Damage"),
                ],
            )
        },
        Rule {
            replaces: Some("incr_spell_dmg"),
            ..rule(
                "#% increased Cold Spell Damage",
                vec![req("#% increased Cold Spell Damage"), rs("#% increased Spell Damage")],
            )
        },
        Rule {
            replaces: Some("incr_spell_dmg"),
            ..rule(
                "#% increased Fire Spell Damage",
                vec![req("#% increased Fire Spell Damage"), rs("#% increased Spell Damage")],
            )
        },
        Rule {
            replaces: Some("incr_ele_dmg"),
            ..rule(
                "#% increased Elemental Damage with Attack Skills",
                vec![
                    req("#% increased Elemental Damage with Attack Skills"),
                    rs("#% increased Elemental Damage"),
                ],
            )
        },
        Rule {
            replaces: Some("incr_fire_dmg"),
            ..rule(
                "#% increased Burning Damage",
                vec![
                    req("#% increased Burning Damage"),
                    rs("#% increased Fire Damage"),
                    rs("#% increased Elemental Damage"),
                ],
            )
        },
        rule(
            "# Life Regenerated per Second",
            vec![rs("Regenerate # Life per second")],
        ),
        rule(
            "#% of Life Regenerated per Second",
            vec![rs("Regenerate #% of Life per second")],
        ),
        rule(
            "#% of Physical Attack Damage Leeched as Life",
            vec![rs("#% of Physical Attack Damage Leeched as Life")],
        ),
        rule(
            "#% of Physical Attack Damage Leeched as Mana",
            vec![rs("#% of Physical Attack Damage Leeched as Mana")],
        ),
        rule(
            "#% increased Mana Regeneration Rate",
            vec![rs("#% increased Mana Regeneration Rate")],
        ),
    ]);
    out
}

pub fn filter_pseudo(ctx: &mut Ctx) -> Result<()> {
    let mut by_group: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut applied: Vec<Rule> = Vec::new();

    'rules: for rule in rules() {
        let mut sources: Vec<StatSource> = Vec::new();
        for calc in &ctx.stats_by_type {
            for source in &calc.sources {
                let Some(info) = rule.stats.iter().find(|s| s.ref_ == calc.stat.ref_) else {
                    continue;
                };
                let c = source
                    .contributes
                    .ok_or_else(|| anyhow!("pseudo source without a roll"))?;
                let m = info.multiplier;
                sources.push(StatSource {
                    contributes: Some(StatRoll {
                        value: c.value * m,
                        min: c.min * m,
                        max: c.max * m,
                    }),
                    ..source.clone()
                });
            }
        }
        if sources.is_empty() {
            continue;
        }

        if rule.stats.iter().any(|s| s.required) {
            for s in rule.stats.iter().filter(|s| s.required) {
                if !sources.iter().any(|src| src.stat.stat.ref_ == s.ref_) {
                    continue 'rules;
                }
            }
        }

        if ctx.item.rarity == Some(ItemRarity::Unique) {
            if ctx.item.is_corrupted {
                if sources.len() < 2 {
                    continue;
                }
            } else if sources
                .iter()
                .filter(|s| s.modifier.info.type_ == ModifierType::Explicit)
                .count()
                < 2
            {
                continue;
            }
        }

        let pseudo =
            pseudo_stat(ctx.data, rule.pseudo).ok_or_else(|| anyhow!("missing pseudo stat {}", rule.pseudo))?;
        let mut filter = calculated_stat_to_filter(
            ctx.data,
            &StatCalculated {
                stat: pseudo,
                type_: ModifierType::Pseudo,
                sources,
            },
            ctx.search_in_range,
            ctx.item,
        )?;

        filter.disabled = rule.disabled.unwrap_or(true);

        if rule.chaos_res_mutation {
            if filter.sources.len() == 1 && filter.sources[0].modifier.info.type_ == ModifierType::Crafted {
                filter.hidden = Some("filters.hide_crafted_chaos".into());
            } else {
                filter.disabled = false;
            }
        }

        let filter_ref = filter.stat_ref.clone();
        ctx.filters.push(filter);

        if let Some(replaced) = rule.replaces.and_then(|g| by_group.get(g)) {
            ctx.filters.retain(|f| !replaced.contains(&f.stat_ref));
        }
        if let Some(group) = rule.group {
            by_group.entry(group).or_default().push(filter_ref);
        }
        applied.push(rule);
    }

    let rarity = ctx.item.rarity;
    ctx.stats_by_type.retain(|m| {
        !applied.iter().any(|r| {
            r.stats
                .iter()
                .any(|s| m.stat.ref_ == s.ref_ && (!s.keep || rarity != Some(ItemRarity::Unique)))
        })
    });

    let value_of = |filters: &[StatFilter], ref_: &str| -> f64 {
        filters
            .iter()
            .find(|f| f.stat_ref == ref_)
            .and_then(|f| f.roll.as_ref())
            .map(|r| r.value)
            .unwrap_or(f64::NAN)
    };

    if let Some(res_refs) = by_group.get("to_x_ele_res") {
        let mut sorted: Vec<(String, f64)> = res_refs
            .iter()
            .map(|r| (r.clone(), value_of(&ctx.filters, r)))
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let max = if sorted.len() > 1 && sorted[0].1 == sorted[1].1 {
            None
        } else {
            sorted.first().map(|s| s.0.clone())
        };

        if let Some(max) = &max {
            if let Some(f) = ctx.filters.iter_mut().find(|f| &f.stat_ref == max) {
                f.hidden = Some("filters.hide_ele_res".into());
            }
        }
        ctx.filters
            .retain(|f| !res_refs.contains(&f.stat_ref) || Some(&f.stat_ref) == max.as_ref());
    }

    if let Some(attr_refs) = by_group.get("to_x_attr") {
        let mut sorted: Vec<(String, f64)> = attr_refs
            .iter()
            .map(|r| (r.clone(), value_of(&ctx.filters, r)))
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if sorted.len() == 3 {
            let to_all = by_group.get("to_all_attrs");
            if sorted.iter().all(|s| s.1 == sorted[0].1) && to_all.is_some() {
                ctx.filters.retain(|f| !attr_refs.contains(&f.stat_ref));
            } else {
                if let Some(to_all) = to_all {
                    ctx.filters.retain(|f| !to_all.contains(&f.stat_ref));
                }

                if sorted[2].1 / sorted[0].1 < 0.3 {
                    let hide = |filters: &mut Vec<StatFilter>, ref_: &str, why: &str| {
                        if let Some(f) = filters.iter_mut().find(|f| f.stat_ref == ref_) {
                            f.hidden = Some(why.to_string());
                        }
                    };
                    if sorted[1].1 == sorted[2].1 {
                        hide(&mut ctx.filters, &sorted[1].0, "hide_attr_same_2nd_n_3rd");
                        hide(&mut ctx.filters, &sorted[2].0, "hide_attr_same_2nd_n_3rd");
                    } else {
                        hide(&mut ctx.filters, &sorted[2].0, "hide_attr_smallest_total");
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn filter_equipment_props(ctx: &mut Ctx) -> Result<()> {
    if known(&ctx.item.category).is_some_and(is_armour) {
        armour_props(ctx)?;
    }
    if known(&ctx.item.category).is_some_and(is_weapon) {
        weapon_props(ctx)?;
    }
    Ok(())
}

pub fn filter_base_percentile(ctx: &mut Ctx) -> Result<()> {
    if let Some(p) = ctx.item.base_percentile {
        let filter = prop_to_filter(
            ctx,
            PropFilter {
                ref_: "Base Percentile: #%",
                trade_id: "item.base_percentile",
                roll: Some(StatRoll {
                    value: p,
                    min: 0.0,
                    max: 100.0,
                }),
                sources: vec![],
                dp: false,
                disabled: Some(p < 50.0),
                hidden: None,
            },
        )?;
        ctx.filters.push(filter);
    }
    Ok(())
}

pub fn filter_memory_strands(ctx: &mut Ctx, hidden: Option<&str>) -> Result<()> {
    if let Some(n) = ctx.item.memory_strands {
        let filter = prop_to_filter(
            ctx,
            PropFilter {
                ref_: "Memory Strands: #",
                trade_id: "item.memory_strands",
                roll: Some(StatRoll {
                    value: n,
                    min: 0.0,
                    max: 100.0,
                }),
                sources: vec![],
                dp: false,
                disabled: Some(n < 60.0 || hidden.is_some()),
                hidden: hidden.map(String::from),
            },
        )?;
        ctx.filters.push(filter);
    }
    Ok(())
}

fn armour_stat_refs() -> Vec<&'static str> {
    let mut v = Vec::new();
    for q in [
        &calc_q20::ARMOUR,
        &calc_q20::EVASION,
        &calc_q20::ENERGY_SHIELD,
        &calc_q20::WARD,
    ] {
        v.extend(q.flat);
        v.extend(q.incr);
    }
    v.push("+#% Chance to Block");
    v
}

fn weapon_stat_refs() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    v.extend(calc_q20::PHYSICAL_DAMAGE.flat);
    v.extend(calc_q20::PHYSICAL_DAMAGE.incr);
    v.extend([
        "#% increased Attack Speed",
        "#% increased Critical Strike Chance",
        "Adds # to # Lightning Damage",
        "Adds # to # Cold Damage",
        "Adds # to # Fire Damage",
    ]);
    v
}

fn is_single_attr_armour(item: &ParsedItem) -> bool {
    [item.armour_ar, item.armour_ev, item.armour_es, item.armour_ward]
        .iter()
        .filter(|v| v.is_some())
        .count()
        == 1
}

fn truthy(v: Option<f64>) -> Option<f64> {
    v.filter(|v| *v != 0.0 && !v.is_nan())
}

fn armour_props(ctx: &mut Ctx) -> Result<()> {
    let item = ctx.item;
    let base = item
        .unique_base
        .as_ref()
        .unwrap_or(&item.info)
        .armour
        .clone()
        .unwrap_or_default();
    let single = is_single_attr_armour(item);

    let defences: [(Option<f64>, &QualityStats, Option<[f64; 2]>, &'static str, &'static str); 4] = [
        (item.armour_ar, &calc_q20::ARMOUR, base.ar, "Armour: #", "item.armour"),
        (
            item.armour_ev,
            &calc_q20::EVASION,
            base.ev,
            "Evasion Rating: #",
            "item.evasion_rating",
        ),
        (
            item.armour_es,
            &calc_q20::ENERGY_SHIELD,
            base.es,
            "Energy Shield: #",
            "item.energy_shield",
        ),
        (item.armour_ward, &calc_q20::WARD, base.ward, "Ward: #", "item.ward"),
    ];
    for (value, stats, bounds, ref_, trade_id) in defences {
        if let Some(total) = truthy(value) {
            let q20 = prop_at_20_quality(total, stats, bounds, item)?;
            let f = prop_to_filter(
                ctx,
                PropFilter {
                    ref_,
                    trade_id,
                    roll: Some(q20.roll),
                    sources: q20.sources,
                    dp: false,
                    disabled: Some(!single),
                    hidden: None,
                },
            )?;
            ctx.filters.push(f);
        }
    }

    if let Some(block) = truthy(item.armour_block) {
        let b = calc_prop_bounds(
            block,
            &QualityStats {
                flat: &["+#% Chance to Block"],
                incr: &[],
            },
            item,
        )?;
        let f = prop_to_filter(
            ctx,
            PropFilter {
                ref_: "Block: #%",
                trade_id: "item.block",
                roll: Some(b.roll),
                sources: b.sources,
                dp: false,
                disabled: Some(true),
                hidden: None,
            },
        )?;
        ctx.filters.push(f);
    }

    if [
        item.armour_ar,
        item.armour_ev,
        item.armour_es,
        item.armour_ward,
        item.armour_block,
    ]
    .iter()
    .any(|v| truthy(*v).is_some())
    {
        let used = armour_stat_refs();
        ctx.stats_by_type.retain(|m| !used.contains(&m.stat.ref_.as_str()));
    }
    Ok(())
}

fn is_pdps_important(item: &ParsedItem) -> bool {
    use ItemCategory::*;
    matches!(
        known(&item.category),
        Some(OneHandedAxe | TwoHandedAxe | OneHandedSword | TwoHandedSword | Bow | Warstaff)
    )
}

fn weapon_props(ctx: &mut Ctx) -> Result<()> {
    let item = ctx.item;

    let attack_speed = calc_prop_bounds(
        item.weapon_as.unwrap_or(0.0),
        &QualityStats {
            incr: &["#% increased Attack Speed"],
            flat: &[],
        },
        item,
    )?;
    let phys_q20 = prop_at_20_quality(
        item.weapon_physical.unwrap_or(0.0),
        &calc_q20::PHYSICAL_DAMAGE,
        None,
        item,
    )?;
    let times = |a: StatRoll, b: StatRoll| StatRoll {
        value: a.value * b.value,
        min: a.min * b.min,
        max: a.max * b.max,
    };
    let pdps = times(phys_q20.roll, attack_speed.roll);

    let ele_dmg = calc_prop_bounds(
        item.weapon_elemental.unwrap_or(0.0),
        &QualityStats {
            flat: &[
                "Adds # to # Lightning Damage",
                "Adds # to # Cold Damage",
                "Adds # to # Fire Damage",
            ],
            incr: &[],
        },
        item,
    )?;
    let edps = times(ele_dmg.roll, attack_speed.roll);
    let dps = StatRoll {
        value: pdps.value + edps.value,
        min: pdps.min + edps.min,
        max: pdps.max + edps.max,
    };

    if truthy(item.weapon_elemental).is_some() {
        if truthy(item.weapon_physical).is_some() {
            let f = prop_to_filter(
                ctx,
                PropFilter {
                    ref_: "Total DPS: #",
                    trade_id: "item.total_dps",
                    roll: Some(dps),
                    sources: vec![],
                    dp: false,
                    disabled: Some(false),
                    hidden: None,
                },
            )?;
            ctx.filters.push(f);
        }

        let low_ele = edps.value / dps.value < 0.67;
        let f = prop_to_filter(
            ctx,
            PropFilter {
                ref_: "Elemental DPS: #",
                trade_id: "item.elemental_dps",
                roll: Some(edps),
                sources: vec![],
                dp: false,
                disabled: Some(low_ele),
                hidden: low_ele.then(|| "filters.hide_ele_dps".to_string()),
            },
        )?;
        ctx.filters.push(f);
    }

    if truthy(item.weapon_physical).is_some() {
        let low_phys = pdps.value / dps.value < 0.67;
        let f = prop_to_filter(
            ctx,
            PropFilter {
                ref_: "Physical DPS: #",
                trade_id: "item.physical_dps",
                roll: Some(pdps),
                sources: vec![],
                dp: false,
                disabled: Some(!is_pdps_important(item) || low_phys),
                hidden: low_phys.then(|| "filters.hide_phys_dps".to_string()),
            },
        )?;
        ctx.filters.push(f);
    }

    let f = prop_to_filter(
        ctx,
        PropFilter {
            ref_: "Attacks per Second: #",
            trade_id: "item.aps",
            roll: Some(attack_speed.roll),
            sources: attack_speed.sources,
            dp: true,
            disabled: Some(true),
            hidden: None,
        },
    )?;
    ctx.filters.push(f);

    if let Some(crit) = truthy(item.weapon_crit) {
        let c = calc_prop_bounds(
            crit,
            &QualityStats {
                incr: &["#% increased Critical Strike Chance"],
                flat: &[],
            },
            item,
        )?;
        let f = prop_to_filter(
            ctx,
            PropFilter {
                ref_: "Critical Strike Chance: #%",
                trade_id: "item.crit",
                roll: Some(c.roll),
                sources: c.sources,
                dp: true,
                disabled: Some(true),
                hidden: None,
            },
        )?;
        ctx.filters.push(f);
    }

    if [
        item.weapon_as,
        item.weapon_crit,
        item.weapon_elemental,
        item.weapon_physical,
    ]
    .iter()
    .any(|v| truthy(*v).is_some())
    {
        let used = weapon_stat_refs();
        ctx.stats_by_type.retain(|m| !used.contains(&m.stat.ref_.as_str()));
    }
    Ok(())
}

pub struct PropFilter {
    pub ref_: &'static str,
    pub trade_id: &'static str,
    pub roll: Option<StatRoll>,
    pub sources: Vec<StatSource>,
    pub dp: bool,
    pub disabled: Option<bool>,
    pub hidden: Option<String>,
}

/// `noSourcePseudoToFilter`: a filter for a made-up stat that no modifier of the item provides.
pub fn no_source_pseudo_to_filter(
    ctx: &Ctx,
    range: f64,
    pseudo: Stat,
    roll: Option<StatRoll>,
    dp: bool,
    disabled: Option<bool>,
    hidden: Option<String>,
) -> Result<StatFilter> {
    let parsed = ParsedStat {
        stat: pseudo.clone(),
        translation: pseudo.matchers[0].clone(),
        roll: roll.map(|r| Roll {
            unscalable: false,
            generation: None,
            dp,
            value: r.value,
            min: r.min,
            max: r.max,
        }),
    };
    let calc = StatCalculated {
        stat: pseudo,
        type_: ModifierType::Pseudo,
        sources: vec![StatSource {
            modifier: crate::parser::advanced::ParsedModifier {
                info: crate::parser::advanced::ModifierInfo::new(ModifierType::Pseudo),
                stats: vec![],
            },
            stat: parsed,
            contributes: roll,
        }],
    };
    let mut filter = calculated_stat_to_filter(ctx.data, &calc, range, ctx.item)?;

    filter.sources = vec![];
    if let Some(d) = disabled {
        filter.disabled = d;
    }
    if let Some(h) = hidden {
        filter.hidden = Some(h);
    }
    Ok(filter)
}

/// `propToFilter`: an item property (armour, dps, ...) as a filter.
pub fn prop_to_filter(ctx: &Ctx, opts: PropFilter) -> Result<StatFilter> {
    prop_to_filter_in_range(ctx, ctx.search_in_range, opts)
}

pub fn prop_to_filter_in_range(ctx: &Ctx, range: f64, opts: PropFilter) -> Result<StatFilter> {
    let stat = Stat {
        ref_: opts.ref_.to_string(),
        matchers: vec![StatMatcher {
            string: opts.ref_.to_string(),
            ..Default::default()
        }],
        trade: StatTrade {
            ids: BTreeMap::from([("pseudo".to_string(), vec![opts.trade_id.to_string()])]),
            ..Default::default()
        },
        better: 1,
        ..Default::default()
    };
    let mut filter = no_source_pseudo_to_filter(ctx, range, stat, opts.roll, opts.dp, opts.disabled, opts.hidden)?;
    filter.tag = FilterTag::PROPERTY;
    filter.sources = opts.sources;
    Ok(filter)
}
