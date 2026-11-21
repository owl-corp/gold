//! Advanced modifier descriptions: the `{ Prefix Modifier "Name" (Tier: 1) — Tags }`
//! lines.
use super::modifiers::{ModifierMechanic, ModifierType};
use super::stat_translations::ParsedStat;
use super::{ParseError, Result};
use crate::client_strings::ClientStrings;
use serde::Serialize;

pub const SCOURGE_LINE: &str = " (scourge)";
pub const ENCHANT_LINE: &str = " (enchant)";
pub const IMPLICIT_LINE: &str = " (implicit)";

#[derive(Clone, Debug, Serialize)]
pub struct ParsedModifier {
    pub info: ModifierInfo,
    pub stats: Vec<ParsedStat>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Generation {
    Suffix,
    Prefix,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifierInfo {
    #[serde(rename = "type")]
    pub type_: ModifierType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<Generation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mechanic: Option<ModifierMechanic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<f64>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll_incr: Option<f64>,
}

impl ModifierInfo {
    #[must_use]
    pub const fn new(type_: ModifierType) -> Self {
        Self {
            type_,
            generation: None,
            mechanic: None,
            name: None,
            tier: None,
            rank: None,
            tags: vec![],
            roll_incr: None,
        }
    }
}

/// `Number(x) || undefined`
fn number_or_none(x: Option<&str>) -> Option<f64> {
    x.map(crate::js::number).filter(|n| !n.is_nan() && *n != 0.0)
}

pub fn parse_mod_info_line(cs: &ClientStrings, line: &str) -> Result<ModifierInfo> {
    // `line.slice(1, -1)`
    let inner = {
        let mut chars = line.chars();
        chars.next();
        chars.next_back();
        chars.as_str()
    };
    let mut parts = inner.split('\u{2014}').map(str::trim);
    let mod_text = parts.next().unwrap_or("");
    let x_text2 = parts.next();
    let x_text3 = parts.next();

    let mut info = ModifierInfo::new(ModifierType::Explicit);

    let eldritch = cs
        .EATER_IMPLICIT
        .captures(mod_text)
        .or_else(|| cs.EXARCH_IMPLICIT.captures(mod_text));
    if let Some(m) = eldritch {
        info.type_ = ModifierType::Implicit;
        info.mechanic = Some(ModifierMechanic::Eldritch);
        let rank = &m["rank"];
        info.rank = [
            &cs.ELDRITCH_MOD_R1,
            &cs.ELDRITCH_MOD_R2,
            &cs.ELDRITCH_MOD_R3,
            &cs.ELDRITCH_MOD_R4,
            &cs.ELDRITCH_MOD_R5,
            &cs.ELDRITCH_MOD_R6,
        ]
        .iter()
        .position(|r| r.as_str() == rank)
        .map(|i| (i + 1) as f64);
    } else {
        let m = cs
            .MODIFIER_LINE
            .captures(mod_text)
            .ok_or_else(|| ParseError::parse("Invalid regex for mod info line"))?;
        let kind = &m["type"];

        if [&cs.IMPLICIT_MODIFIER, &cs.CORRUPTED_IMPLICIT, &cs.VESTIGIAL_IMPLICIT]
            .iter()
            .any(|s| s.as_str() == kind)
        {
            info.type_ = ModifierType::Implicit;
        } else if kind == cs.FRACTURED_PREFIX || kind == cs.FRACTURED_SUFFIX {
            info.type_ = ModifierType::Fractured;
        } else if kind == cs.CRAFTED_PREFIX || kind == cs.CRAFTED_SUFFIX {
            info.type_ = ModifierType::Crafted;
        }

        if kind == cs.PREFIX_MODIFIER || kind == cs.FRACTURED_PREFIX || kind == cs.CRAFTED_PREFIX {
            info.generation = Some(Generation::Prefix);
        } else if kind == cs.SUFFIX_MODIFIER || kind == cs.FRACTURED_SUFFIX || kind == cs.CRAFTED_SUFFIX {
            info.generation = Some(Generation::Suffix);
        } else if kind == cs.CORRUPTED_IMPLICIT {
            info.mechanic = Some(ModifierMechanic::Corruption);
        } else if kind == cs.FOULBORN_MODIFIER {
            info.mechanic = Some(ModifierMechanic::Foulborn);
        } else if kind == cs.VESTIGIAL_IMPLICIT {
            info.mechanic = Some(ModifierMechanic::Vestigial);
        }

        info.name = m.name("name").map(|n| n.as_str().to_string());
        info.tier = number_or_none(m.name("tier").map(|n| n.as_str()));
        info.rank = number_or_none(m.name("rank").map(|n| n.as_str()));
    }

    if info.mechanic.is_none() {
        if let Some(name) = &info.name {
            info.mechanic = mechanic_from_mod_name(cs, name);
        }
    }

    let incr_text = match (x_text3, x_text2) {
        (Some(t3), _) => Some(t3),
        (None, Some(t2)) if cs.MODIFIER_INCREASED.is_match(t2) => Some(t2),
        _ => None,
    };
    let tags_text = match x_text2 {
        Some(t2) if incr_text != Some(t2) => Some(t2),
        _ => None,
    };

    info.tags = match tags_text {
        Some(t) if !t.is_empty() => t.split(", ").map(str::to_string).collect(),
        _ => vec![],
    };
    info.roll_incr = match incr_text {
        Some(t) if !t.is_empty() => {
            let m = cs
                .MODIFIER_INCREASED
                .captures(t)
                .ok_or_else(|| ParseError::parse("modifier roll increase does not match"))?;
            Some(crate::js::number(&m[1]))
        }
        _ => None,
    };

    Ok(info)
}

#[must_use]
pub fn is_mod_info_line(line: &str) -> bool {
    line.starts_with('{') && line.ends_with('}')
}

pub struct GroupedModLines {
    pub mod_line: String,
    pub stat_lines: Vec<String>,
}

#[must_use]
pub fn group_lines_by_mod(lines: &[&str]) -> Vec<GroupedModLines> {
    let mut out: Vec<GroupedModLines> = Vec::new();
    if !lines.first().is_some_and(|l| is_mod_info_line(l)) {
        return out;
    }
    for line in lines {
        if is_mod_info_line(line) {
            out.push(GroupedModLines {
                mod_line: (*line).to_string(),
                stat_lines: vec![],
            });
        } else if let Some(last) = out.last_mut() {
            last.stat_lines.push((*line).to_string());
        }
    }
    out
}

pub fn parse_mod_type(lines: &[&str]) -> Result<(ModifierType, Vec<String>)> {
    let ends = |suffix: &str| lines.iter().any(|l| l.ends_with(suffix));
    let strip = |suffix: &str| -> Vec<String> {
        lines
            .iter()
            .map(|l| l.strip_suffix(suffix).unwrap_or(l).to_string())
            .collect()
    };
    if ends(SCOURGE_LINE) {
        Ok((ModifierType::Scourge, strip(SCOURGE_LINE)))
    } else if ends(ENCHANT_LINE) {
        Ok((ModifierType::Enchant, strip(ENCHANT_LINE)))
    } else if ends(IMPLICIT_LINE) {
        Ok((ModifierType::Implicit, strip(IMPLICIT_LINE)))
    } else {
        Err(ParseError::parse(
            "Expected to be used only on lines that have modifier type",
        ))
    }
}

/// Stat values are stored as hundredths, so rolls with decimals are scaled by 2 decimal places.
const DIV_BY_100: i32 = 2;

#[must_use]
pub fn apply_incr(info: &ModifierInfo, parsed: &ParsedStat) -> Option<ParsedStat> {
    let incr = info.roll_incr?;
    let roll = parsed.roll.as_ref()?;
    if incr == 0.0 || incr.is_nan() || roll.unscalable {
        return None;
    }
    let dp = if roll.dp { DIV_BY_100 } else { 0 };
    let mut scaled = roll.clone();
    scaled.generation = None;
    scaled.value = incr_roll(roll.value, incr, dp);
    scaled.min = incr_roll(roll.min, incr, dp);
    scaled.max = incr_roll(roll.max, incr, dp);
    Some(ParsedStat {
        stat: parsed.stat.clone(),
        translation: parsed.translation.clone(),
        roll: Some(scaled),
    })
}

#[must_use]
pub fn incr_roll(value: f64, p: f64, dp: i32) -> f64 {
    let res = value + (value * p / 100.0);
    let rounding = 10f64.powi(dp);
    ((res + f64::EPSILON) * rounding).trunc() / rounding
}

fn mechanic_from_mod_name(cs: &ClientStrings, name: &str) -> Option<ModifierMechanic> {
    let has = |list: &Vec<String>| list.iter().any(|n| n == name);
    if has(&cs.SHAPER_MODS) {
        Some(ModifierMechanic::Shaper)
    } else if has(&cs.ELDER_MODS) {
        Some(ModifierMechanic::Elder)
    } else if has(&cs.HUNTER_MODS) {
        Some(ModifierMechanic::Hunter)
    } else if has(&cs.WARLORD_MODS) {
        Some(ModifierMechanic::Warlord)
    } else if has(&cs.REDEEMER_MODS) {
        Some(ModifierMechanic::Redeemer)
    } else if has(&cs.CRUSADER_MODS) {
        Some(ModifierMechanic::Crusader)
    } else if has(&cs.DELVE_MODS) {
        Some(ModifierMechanic::Delve)
    } else if has(&cs.VEILED_MODS) {
        Some(ModifierMechanic::Unveiled)
    } else if has(&cs.INCURSION_MODS) {
        Some(ModifierMechanic::Incursion)
    } else if has(&cs.ESSENCE_MODS) {
        Some(ModifierMechanic::Essence)
    } else if has(&cs.INFAMOUS_MODS) {
        Some(ModifierMechanic::Infamous)
    } else {
        None
    }
}
