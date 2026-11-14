//! Modifier types and per-stat totals.
use super::advanced::{apply_incr, ParsedModifier};
use super::stat_translations::ParsedStat;
use crate::data::{Stat, StatMatcher};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModifierType {
    Pseudo,
    Explicit,
    Implicit,
    Crafted,
    Enchant,
    Scourge,
    Necropolis,
    Veiled,
    Fractured,
    Imbued,
}

impl ModifierType {
    /// The key used in a stat's `trade.ids`.
    pub fn as_str(self) -> &'static str {
        match self {
            ModifierType::Pseudo => "pseudo",
            ModifierType::Explicit => "explicit",
            ModifierType::Implicit => "implicit",
            ModifierType::Crafted => "crafted",
            ModifierType::Enchant => "enchant",
            ModifierType::Scourge => "scourge",
            ModifierType::Necropolis => "necropolis",
            ModifierType::Veiled => "veiled",
            ModifierType::Fractured => "fractured",
            ModifierType::Imbued => "imbued",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ModifierMechanic {
    #[serde(rename = "synthesised")]
    Synthesised,
    #[serde(rename = "corrupted")]
    Corruption,
    #[serde(rename = "eldritch")]
    Eldritch,
    #[serde(rename = "foulborn")]
    Foulborn,
    #[serde(rename = "vestigial")]
    Vestigial,
    #[serde(rename = "explicit-shaper")]
    Shaper,
    #[serde(rename = "explicit-elder")]
    Elder,
    #[serde(rename = "explicit-crusader")]
    Crusader,
    #[serde(rename = "explicit-hunter")]
    Hunter,
    #[serde(rename = "explicit-redeemer")]
    Redeemer,
    #[serde(rename = "explicit-warlord")]
    Warlord,
    #[serde(rename = "explicit-delve")]
    Delve,
    #[serde(rename = "explicit-veiled")]
    Unveiled,
    #[serde(rename = "explicit-incursion")]
    Incursion,
    #[serde(rename = "explicit-infamous")]
    Infamous,
    #[serde(rename = "explicit-essence")]
    Essence,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct StatRoll {
    pub value: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatSource {
    pub modifier: ParsedModifier,
    pub stat: ParsedStat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributes: Option<StatRoll>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatCalculated {
    pub stat: Stat,
    #[serde(rename = "type")]
    pub type_: ModifierType,
    pub sources: Vec<StatSource>,
}

pub fn sum_stats_by_mod_type(mods: &[ParsedModifier]) -> Vec<StatCalculated> {
    let mut out: Vec<StatCalculated> = Vec::new();

    for mod_a in mods {
        for stat_a in &mod_a.stats {
            if out
                .iter()
                .any(|m| m.stat.ref_ == stat_a.stat.ref_ && m.type_ == mod_a.info.type_)
            {
                continue;
            }

            let mut sources = Vec::new();
            for mod_b in mods {
                if mod_b.info.type_ != mod_a.info.type_ {
                    continue;
                }
                if let Some(target) = mod_b.stats.iter().find(|s| s.stat.ref_ == stat_a.stat.ref_) {
                    let scaled = apply_incr(&mod_b.info, target);
                    let roll = scaled.as_ref().unwrap_or(target).roll.as_ref();
                    sources.push(StatSource {
                        modifier: mod_b.clone(),
                        stat: target.clone(),
                        contributes: roll.map(|r| StatRoll {
                            value: r.value,
                            min: r.min,
                            max: r.max,
                        }),
                    });
                }
            }

            out.push(StatCalculated {
                stat: stat_a.stat.clone(),
                type_: mod_a.info.type_,
                sources,
            });
        }
    }

    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SumMode {
    Sum,
    Max,
}

pub fn stat_sources_total(sources: &[StatSource], mode: SumMode) -> Option<StatRoll> {
    if sources.iter().all(|s| s.contributes.is_none()) {
        return None;
    }

    let combine = |a: f64, b: f64| match mode {
        SumMode::Sum => a + b,
        SumMode::Max => crate::js::max(a, b),
    };
    if sources.len() == 1 {
        return sources[0].contributes;
    }
    let mut sum = StatRoll::default();
    for source in sources {
        // NOTE: this also sums up stats that have "flag" semantics
        let c = source.contributes.unwrap_or(StatRoll {
            value: 1.0,
            min: 1.0,
            max: 1.0,
        });
        sum.value = combine(sum.value, c.value);
        sum.min = combine(sum.min, c.min);
        sum.max = combine(sum.max, c.max);
    }
    Some(sum)
}

pub struct Translation {
    pub string: String,
    pub negate: bool,
    pub dp: Option<bool>,
}

pub fn translate_stat_with_roll(calc: &StatCalculated, roll: Option<StatRoll>) -> anyhow::Result<Translation> {
    let matchers = &calc.stat.matchers;
    let translation: &StatMatcher = match roll {
        None => matchers.iter().find(|m| m.value.is_none()).unwrap_or(&matchers[0]),
        Some(roll) => {
            let mut t = matchers.iter().find(|m| m.value == Some(roll.value));
            if t.is_none() {
                let same_sign = crate::js::sign(roll.min) == crate::js::sign(roll.max);
                t = if same_sign {
                    matchers
                        .iter()
                        .find(|m| m.value.is_none() && m.negate.unwrap_or(false) == (roll.value < 0.0))
                } else {
                    matchers
                        .iter()
                        .find(|m| m.value.is_none() && !m.negate.unwrap_or(false))
                };
            }
            if t.is_none() {
                t = matchers.iter().find(|m| m.value.is_none());
            }
            t.ok_or_else(|| anyhow::anyhow!("never"))?
        }
    };

    let dp = roll.map(|_| {
        calc.stat.dp.unwrap_or(false)
            || calc
                .sources
                .iter()
                .any(|s| s.stat.stat.ref_ == calc.stat.ref_ && s.stat.roll.as_ref().is_some_and(|r| r.dp))
    });

    Ok(Translation {
        string: translation.string.clone(),
        negate: translation.negate.unwrap_or(false),
        dp,
    })
}
