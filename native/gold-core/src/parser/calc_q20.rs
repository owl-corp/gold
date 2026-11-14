//! Armour, evasion and damage at base quality and at 20% quality
//!.
use super::item::ParsedItem;
use super::modifiers::{stat_sources_total, StatRoll, StatSource, SumMode};
use anyhow::{anyhow, Result};

pub struct QualityStats {
    pub flat: &'static [&'static str],
    pub incr: &'static [&'static str],
}

pub const ARMOUR: QualityStats = QualityStats {
    flat: &["+# to Armour"],
    incr: &[
        "#% increased Armour",
        "#% increased Armour and Energy Shield",
        "#% increased Armour and Evasion",
        "#% increased Armour, Evasion and Energy Shield",
    ],
};

pub const EVASION: QualityStats = QualityStats {
    flat: &["+# to Evasion Rating"],
    incr: &[
        "#% increased Evasion Rating",
        "#% increased Armour and Evasion",
        "#% increased Evasion and Energy Shield",
        "#% increased Armour, Evasion and Energy Shield",
    ],
};

pub const ENERGY_SHIELD: QualityStats = QualityStats {
    flat: &["+# to maximum Energy Shield"],
    incr: &[
        "#% increased Energy Shield",
        "#% increased Armour and Energy Shield",
        "#% increased Evasion and Energy Shield",
        "#% increased Armour, Evasion and Energy Shield",
    ],
};

pub const WARD: QualityStats = QualityStats {
    flat: &["+# to Ward"],
    incr: &["#% increased Ward"],
};

pub const PHYSICAL_DAMAGE: QualityStats = QualityStats {
    flat: &["Adds # to # Physical Damage"],
    incr: &["#% increased Physical Damage"],
};

pub const QUALITY_CHANGING_ENCHANT: [&str; 2] = [
    "Quality does not increase Defences",
    "Quality does not increase Physical Damage",
];

fn has_quality_changing_enchant(item: &ParsedItem) -> bool {
    item.stats_by_type
        .iter()
        .any(|c| QUALITY_CHANGING_ENCHANT.contains(&c.stat.ref_.as_str()))
}

pub fn get_trade_max_quality(item: &ParsedItem) -> f64 {
    if has_quality_changing_enchant(item) {
        return 0.0;
    }
    // currently Trade Site applies 20% to everything else,
    // including Mirrored items
    crate::js::max(20.0, item.quality.unwrap_or(0.0))
}

pub fn get_prop_quality(item: &ParsedItem) -> f64 {
    if has_quality_changing_enchant(item) {
        return 0.0;
    }
    item.quality.unwrap_or(0.0)
}

pub struct PropRoll {
    pub roll: StatRoll,
    pub sources: Vec<StatSource>,
}

pub fn prop_at_20_quality(
    total: f64,
    stats: &QualityStats,
    bounds: Option<[f64; 2]>,
    item: &ParsedItem,
) -> Result<PropRoll> {
    let (incr, flat, sources) = calc_prop_base(stats, item)?;
    let base = calc_flat(total, incr.value, get_prop_quality(item)) - flat.value;
    let [base_min, base_max] = bounds.unwrap_or([base, base]);
    let quality = get_trade_max_quality(item);
    let roll = if !base.is_nan() {
        StatRoll {
            value: calc_increased(base + flat.value, incr.value, quality),
            min: calc_increased(base_min + flat.min, incr.min, quality),
            max: calc_increased(base_max + flat.max, incr.max, quality),
        }
    } else {
        StatRoll {
            value: 0.0,
            min: 0.0,
            max: 0.0,
        }
    };
    Ok(PropRoll {
        roll,
        sources: sources
            .into_iter()
            .map(|s| StatSource { contributes: None, ..s })
            .collect(),
    })
}

pub fn calc_prop_bounds(total: f64, stats: &QualityStats, item: &ParsedItem) -> Result<PropRoll> {
    let (incr, flat, sources) = calc_prop_base(stats, item)?;
    let base = calc_flat(total, incr.value, 0.0) - flat.value;
    let roll = StatRoll {
        value: calc_increased(base + flat.value, incr.value, 0.0),
        min: calc_increased(base + flat.min, incr.min, 0.0),
        max: calc_increased(base + flat.max, incr.max, 0.0),
    };
    let sources = if !stats.incr.is_empty() && !stats.flat.is_empty() {
        sources
            .into_iter()
            .map(|s| StatSource { contributes: None, ..s })
            .collect()
    } else if !stats.incr.is_empty() {
        sources
            .into_iter()
            .map(|s| {
                let c = s.contributes.ok_or_else(|| anyhow!("source without a roll"))?;
                Ok(StatSource {
                    contributes: Some(StatRoll {
                        value: base * c.value / 100.0,
                        min: base * c.min / 100.0,
                        max: base * c.max / 100.0,
                    }),
                    ..s
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        sources
    };
    Ok(PropRoll { roll, sources })
}

pub fn calc_prop_percentile(total: f64, bounds: [f64; 2], stats: &QualityStats, item: &ParsedItem) -> Result<f64> {
    let (incr, flat, _) = calc_prop_base(stats, item)?;
    let roll = calc_flat(total, incr.value, get_prop_quality(item)) - flat.value;
    let [min, max] = bounds;
    let result = crate::js::round(((roll - min) / (max - min)) * 100.0);
    Ok(crate::js::min(crate::js::max(result, 0.0), 100.0))
}

fn calc_prop_base(stats: &QualityStats, item: &ParsedItem) -> Result<(StatRoll, StatRoll, Vec<StatSource>)> {
    let mut incr = StatRoll::default();
    let mut flat = StatRoll::default();
    let mut sources = Vec::new();

    for calc in &item.stats_by_type {
        let total = if stats.flat.contains(&calc.stat.ref_.as_str()) {
            &mut flat
        } else if stats.incr.contains(&calc.stat.ref_.as_str()) {
            &mut incr
        } else {
            continue;
        };
        let roll = stat_sources_total(&calc.sources, SumMode::Sum).ok_or_else(|| anyhow!("stat without a roll"))?;
        total.value += roll.value;
        total.min += roll.min;
        total.max += roll.max;
        sources.extend(calc.sources.iter().cloned());
    }

    Ok((incr, flat, sources))
}

fn calc_flat(total: f64, incr_pct: f64, more_pct: f64) -> f64 {
    // We cannot know original roll if the `morePct` or `incrPct` is equal to -100,
    // in that case a NaN will be returned.
    total / (1.0 + more_pct / 100.0) / (1.0 + incr_pct / 100.0)
}

fn calc_increased(flat: f64, incr_pct: f64, more_pct: f64) -> f64 {
    flat * (1.0 + incr_pct / 100.0) * (1.0 + more_pct / 100.0)
}
