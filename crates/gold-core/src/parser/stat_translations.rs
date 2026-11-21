//! Turning stat text lines into stats.
use super::meta::{is_armour, is_heist_equipment, is_weapon, Category};
use super::modifiers::ModifierType;
use super::{ParseError, Result};
use crate::client_strings::ClientStrings;
use crate::data::{GameData, Stat, StatGroup, StatGroupResolver, StatMatcher, StatOrGroup};
use regex::Regex;
use serde::Serialize;
use std::sync::Arc;
use std::sync::LazyLock;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RollGeneration {
    Legacy,
    Volatile,
    Reflecting,
}

#[derive(Clone, Debug, Serialize)]
pub struct Roll {
    pub unscalable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<RollGeneration>,
    pub dp: bool,
    pub value: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ParsedStat {
    pub stat: Arc<Stat>,
    pub translation: Arc<StatMatcher>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll: Option<Roll>,
}

pub struct StatString {
    pub string: String,
    pub unscalable: bool,
}

#[expect(clippy::unwrap_used, reason = "constant pattern")]
static PAREN_LEFT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(?:\(|（)").unwrap());

#[expect(clippy::unwrap_used, reason = "constant pattern")]
static PAREN_RIGHT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:\)|）)\s*$").unwrap());

/// Offers every run of consecutive lines to `try_parse` (which says whether the run
/// was recognized) and returns the lines nothing recognized.
pub fn lines_to_stat_strings(
    cs: &ClientStrings,
    lines: &[String],
    try_parse: &mut dyn FnMut(StatString) -> Result<bool>,
) -> Result<Vec<String>> {
    let mut not_parsed = Vec::new();
    let mut reminder_string = false;

    let mut start = 0;
    'outer: while let Some(first) = lines.get(start) {
        if PAREN_LEFT.is_match(first) {
            reminder_string = true;
        }
        if reminder_string && PAREN_RIGHT.is_match(first) {
            reminder_string = false;
            start += 1;
            continue;
        }
        if reminder_string {
            start += 1;
            continue;
        }

        let mut end = start;
        while let Some(run) = lines.get(start..=end) {
            let mut string = run.join("\n");
            let unscalable = string.ends_with(&cs.UNSCALABLE_VALUE);
            if unscalable {
                string.truncate(string.len() - cs.UNSCALABLE_VALUE.len());
            }
            if try_parse(StatString { string, unscalable })? {
                start = end + 1;
                continue 'outer;
            }
            end += 1;
        }
        not_parsed.push(first.clone());
        start += 1;
    }
    Ok(not_parsed.into_iter().filter(|l| !l.is_empty()).collect())
}

const PLACEHOLDER_MAP: [&[&[usize]]; 5] = [
    // 0 # -> max 0 #
    &[&[]],
    // 1 # -> max 1 #
    &[&[0], &[]],
    // 2 # -> max 2 #
    &[&[0, 1], &[0], &[1], &[]],
    // 3 # -> max 2 #
    &[&[0, 1, 2], &[1, 2], &[0, 2], &[0, 1], &[2], &[1], &[0]],
    // 4 # -> max 2 #
    &[
        &[0, 1, 2, 3],
        &[1, 2, 3],
        &[0, 2, 3],
        &[0, 1, 3],
        &[0, 1, 2],
        &[2, 3],
        &[1, 3],
        &[1, 2],
        &[0, 3],
        &[0, 2],
        &[0, 1],
    ],
];

#[derive(Clone, Debug)]
struct Match {
    roll: f64,
    roll_str: String,
    decimal: bool,
    bounds: Option<(f64, f64)>,
}

struct Combination {
    stat: String,
    values: Vec<Match>,
}

#[expect(clippy::unwrap_used, reason = "constant pattern")]
static VALUE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?<value>[+-]?\d+(?:\.\d+)?)(?:\((?<min>.[^)-]*)(?:-(?<max>[^)]+))?\))?").unwrap());

/// Matches of `VALUE_RE` that do not directly follow a digit or `)`.
fn value_matches(source: &str) -> Vec<regex::Captures<'_>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(caps) = VALUE_RE.captures_at(source, pos) {
        let Some(whole) = caps.get(0) else { break };
        let after_digit = source
            .get(..whole.start())
            .and_then(|before| before.chars().next_back())
            .is_some_and(|c| c.is_numeric() || c == ')');
        if after_digit {
            // a match here is not allowed: try again one character later
            pos = whole.start()
                + source
                    .get(whole.start()..)
                    .and_then(|s| s.chars().next())
                    .map_or(1, char::len_utf8);
        } else {
            pos = whole.end();
            out.push(caps);
        }
    }
    out
}

/// Candidate readings of a stat line: numbers replaced by `#`, from "all numbers are
/// rolls" down to "none are" (then the numbers stay in the text).
fn stat_placeholder_combinations(stat: &str) -> Vec<Combination> {
    let mut matches: Vec<Match> = Vec::new();
    // when GGG didn't provide advanced desc, like in "Passives in Radius of Wicked Ward() can be Allocated"
    let source = stat.replace("()", "");

    let mut with_placeholders = String::new();
    let mut last = 0;
    for caps in value_matches(&source) {
        let (Some(whole), Some(value)) = (caps.get(0), caps.name("value")) else {
            continue;
        };
        with_placeholders.push_str(source.get(last..whole.start()).unwrap_or_default());
        last = whole.end();

        let roll = value.as_str();
        let min = caps.name("min").map(|m| m.as_str());
        let mut max = caps.name("max").map(|m| m.as_str());
        if min.is_some() && max.is_none() {
            // example: Sextant "# uses remaining", legacy rolls
            max = min;
        }

        let bounds = min
            .zip(max)
            .map(|(lo, hi)| (crate::js::number(lo), crate::js::number(hi)))
            .filter(|(lo, hi)| !lo.is_nan() && !hi.is_nan());
        let valid = bounds.is_some();
        matches.push(Match {
            roll: crate::js::number(roll),
            roll_str: roll.to_string(),
            decimal: roll.contains('.') || min.is_some_and(|m| m.contains('.')) || max.is_some_and(|m| m.contains('.')),
            bounds,
        });

        if valid {
            with_placeholders.push('#');
        } else if let (Some(min), Some(max)) = (min, max) {
            with_placeholders.push_str(&["#(", min, "-", max, ")"].concat());
        } else {
            with_placeholders.push('#');
        }
    }
    with_placeholders.push_str(source.get(last..).unwrap_or_default());

    let mut out = Vec::new();
    if let Some(all_replacements) = PLACEHOLDER_MAP.get(matches.len()) {
        for replacements in *all_replacements {
            let mut idx = 0usize;
            let mut replaced = String::new();
            for ch in with_placeholders.chars() {
                if ch == '#' {
                    match matches.get(idx) {
                        Some(m) if replacements.contains(&idx) => replaced.push_str(&m.roll_str),
                        _ => replaced.push('#'),
                    }
                    idx += 1;
                } else {
                    replaced.push(ch);
                }
            }
            let values = matches
                .iter()
                .enumerate()
                .filter(|(i, _)| !replacements.contains(i))
                .map(|(_, m)| m.clone())
                .collect();
            out.push(Combination { stat: replaced, values });
        }
    }

    // fallback to exact stat text, without any placeholders
    out.push(Combination {
        stat: stat.to_string(),
        values: vec![],
    });
    out
}

pub fn try_parse_translation(
    data: &GameData,
    stat: &StatString,
    mod_type: ModifierType,
    item_category: Option<&Category>,
) -> Result<Option<ParsedStat>> {
    for mut combination in stat_placeholder_combinations(&stat.string) {
        let roll = match combination.values.as_slice() {
            [only] => Some(only.roll),
            _ => None,
        };
        let found = find_and_resolve_translation(data, &combination.stat, mod_type, item_category, roll);
        let Some((found_stat, matcher)) = found else { continue };
        if !found_stat.trade.ids.contains_key(mod_type.as_str()) {
            continue;
        }

        // Modifiers must be upgraded to the new values with a Divine Orb
        let mut legacy_stat_rolls = false;

        if matcher.negate.unwrap_or(false) {
            for v in &mut combination.values {
                v.roll *= -1.0;
                if let Some((min, max)) = v.bounds {
                    v.bounds = Some((-min, -max));
                }
            }
        }

        if found_stat.ref_ == "# uses remaining" {
            let uses = combination
                .values
                .get_mut(0)
                .ok_or_else(|| ParseError::parse("uses remaining without a number"))?;
            uses.bounds = Some((1.0, uses.bounds.map_or(uses.roll, |b| b.1)));
        }

        for v in &mut combination.values {
            let Some((mut min, mut max)) = v.bounds else { continue };
            if min > max {
                // some stats granted by legacy Modifiers (not legacy rolls)
                // can have same stat translations as granted by new Modifiers
                // but swapped translation strings for positive and negative rolls
                std::mem::swap(&mut min, &mut max);
                // don't consider them as a legacy rolls
            }
            if v.roll > max {
                max = v.roll;
                legacy_stat_rolls = true;
            }
            if v.roll < min {
                min = v.roll;
                legacy_stat_rolls = true;
            }
            v.bounds = Some((min, max));
        }

        if combination.values.is_empty() {
            if let Some(value) = matcher.value.filter(|v| *v != 0.0 && !v.is_nan()) {
                combination.values = vec![Match {
                    roll: value,
                    roll_str: String::new(),
                    decimal: false,
                    bounds: Some((value, value)),
                }];
            }
        }

        let roll = (!combination.values.is_empty()).then(|| Roll {
            unscalable: stat.unscalable,
            generation: legacy_stat_rolls.then_some(RollGeneration::Legacy),
            dp: found_stat.dp.unwrap_or(false) || combination.values.iter().any(|v| v.decimal),
            value: roll_or_minmax_avg(&combination.values.iter().map(|v| v.roll).collect::<Vec<_>>()),
            min: roll_or_minmax_avg(
                &combination
                    .values
                    .iter()
                    .map(|v| v.bounds.map_or(v.roll, |b| b.0))
                    .collect::<Vec<_>>(),
            ),
            max: roll_or_minmax_avg(
                &combination
                    .values
                    .iter()
                    .map(|v| v.bounds.map_or(v.roll, |b| b.1))
                    .collect::<Vec<_>>(),
            ),
        });
        return Ok(Some(ParsedStat {
            stat: Arc::new(found_stat),
            translation: Arc::new(matcher),
            roll,
        }));
    }
    Ok(None)
}

/// `getRollOrMinmaxAvg`: the average when given a min and max, else the first value.
#[must_use]
pub fn roll_or_minmax_avg(values: &[f64]) -> f64 {
    match values {
        [a, b] => f64::midpoint(*a, *b),
        _ => values.first().copied().unwrap_or(f64::NAN),
    }
}

fn matches_str(m: &StatMatcher, match_str: &str) -> bool {
    m.string == match_str || m.advanced.as_deref() == Some(match_str)
}

fn find_and_resolve_translation(
    data: &GameData,
    match_str: &str,
    mod_type: ModifierType,
    item_category: Option<&Category>,
    roll: Option<f64>,
) -> Option<(Stat, StatMatcher)> {
    let stat = match data.stats.by_match_str_v2(match_str)? {
        StatOrGroup::Stat(s) => (**s).clone(),
        StatOrGroup::Group(g) => resolve_translation(g, match_str, mod_type, item_category, roll)?,
    };
    let matcher = stat.matchers.iter().find(|m| matches_str(m, match_str))?.clone();
    Some((stat, matcher))
}

#[must_use]
pub fn resolve_translation(
    group: &StatGroup,
    match_str: &str,
    mod_type: ModifierType,
    item_category: Option<&Category>,
    roll: Option<f64>,
) -> Option<Stat> {
    let StatGroup { resolve, stats, .. } = group;

    if let StatGroupResolver::Select { test } = resolve {
        // give priority to exact match
        let mut idx = test.iter().position(|expected| {
            expected
                .as_deref()
                .is_some_and(|e| test_item_category(item_category, e))
        });
        // fallback to any match (if it exists at all)
        if idx.is_none() {
            idx = test.iter().position(std::option::Option::is_none);
        }
        return idx.and_then(|i| stats.get(i).cloned());
    }

    let on_trade: Vec<usize> = stats
        .iter()
        .enumerate()
        .filter(|(_, s)| s.trade.ids.contains_key(mod_type.as_str()))
        .map(|(i, _)| i)
        .collect();
    if let [only] = on_trade.as_slice() {
        return stats.get(*only).cloned();
    }

    match resolve {
        StatGroupResolver::TrivialMerge => {
            let with_match: Vec<usize> = if match_str.is_empty() {
                on_trade
            } else {
                on_trade
                    .iter()
                    .copied()
                    .filter(|&i| {
                        stats
                            .get(i)
                            .is_some_and(|s| s.matchers.iter().any(|m| matches_str(m, match_str)))
                    })
                    .collect()
            };
            let (&first, rest) = with_match.split_first()?;
            let others: Vec<Stat> = rest.iter().filter_map(|&i| stats.get(i).cloned()).collect();
            let mut merged = stats.get(first)?.clone();
            for other in &others {
                merge_trade_ids_into(&mut merged, other, None);
            }
            Some(merged)
        }
        StatGroupResolver::PercentMerge { kind } => {
            let pct = kind.iter().position(|k| k == "percent")?;
            let pct_base = stats.get(pct)?;
            let matcher = pct_base.matchers.iter().find(|m| matches_str(m, match_str));
            let roll100 = matcher.is_some_and(|m| m.value == Some(100.0));
            if roll100 {
                let other = stats.get(kind.iter().position(|k| k == "value")?)?.clone();
                let flag = matches!(other.matchers.as_slice(), [only] if !only.string.contains('#'));
                let mut pct_stat = pct_base.clone();
                merge_trade_ids_into(
                    &mut pct_stat,
                    &other,
                    Some(if flag { "{empty_if_100}" } else { "{div_by_100}" }),
                );
                return Some(pct_stat);
            }
            stats
                .iter()
                .find(|s| s.matchers.iter().any(|m| matches_str(m, match_str)))
                .cloned()
        }
        StatGroupResolver::FlagMerge { kind } => {
            let roll = roll?;
            let val = kind.iter().position(|k| k == "value")?;
            let flag = kind.iter().position(|k| k == "flag")?;
            let flag_stat = stats.get(flag)?.clone();
            let flag_roll = flag_stat.matchers.first()?.value?;
            let mut val_stat = stats.get(val)?.clone();
            if crate::js::eq(roll, flag_roll) {
                merge_trade_ids_into(&mut val_stat, &flag_stat, Some("{empty}"));
            }
            Some(val_stat)
        }
        StatGroupResolver::Select { .. } => None,
    }
}

fn merge_trade_ids_into(dest: &mut Stat, source: &Stat, prefix: Option<&str>) {
    for (mod_type, ids) in &source.trade.ids {
        let Some(first) = ids.first() else { continue };
        let trade_id = prefix.map_or_else(|| first.clone(), |p| format!("{p}{first}"));
        match dest.trade.ids.get_mut(mod_type) {
            Some(existing) => {
                if !existing.contains(&trade_id) {
                    existing.push(trade_id);
                }
            }
            None => {
                dest.trade.ids.insert(mod_type.clone(), vec![trade_id]);
            }
        }
    }
}

fn test_item_category(actual: Option<&Category>, expected: &str) -> bool {
    let Some(actual) = actual else { return false };
    let known = actual.known();
    match expected {
        "WEAPON" => known.is_some_and(is_weapon),
        "ARMOUR" => known.is_some_and(is_armour),
        "HEIST_EQUIPMENT" => known.is_some_and(is_heist_equipment),
        other => match actual {
            Category::Known(c) => c.as_str() == other,
            Category::Other(o) => o == other,
        },
    }
}
