//! Item text parser.
//!
//! The item text is split into sections (separated by `--------`). Each parser in
//! `PARSERS` looks for the section it understands; a section is consumed by at most
//! one parser. Parsers run in order, and the order matters.
pub mod advanced;
pub mod calc_q20;
pub mod item;
pub mod magic_name;
pub mod meta;
pub mod modifiers;
pub mod stat_translations;

use self::advanced::*;
use self::item::*;
use self::meta::{is, is_jewellery, known, Category, ItemCategory};
use self::modifiers::{sum_stats_by_mod_type, ModifierMechanic, ModifierType};
use self::stat_translations::*;
use crate::data::{BaseType, GameData};
use crate::js;
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// `item.parse_error`: not an item, or the parser hit something it can't handle.
    Parse,
    /// `item.unknown`: an item that isn't in the game data.
    Unknown,
}

impl ParseError {
    pub fn code(self) -> &'static str {
        match self {
            ParseError::Parse => "item.parse_error",
            ParseError::Unknown => "item.unknown",
        }
    }
}

#[derive(Debug)]
struct UnknownItem;

impl std::fmt::Display for UnknownItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "item.unknown")
    }
}

impl std::error::Error for UnknownItem {}

#[derive(Debug, PartialEq, Eq)]
enum SectionResult {
    Parsed,
    Skipped,
    ParserSkipped,
}
use SectionResult::*;

type SectionFn = fn(&GameData, &[String], &mut ParsedItem) -> Result<SectionResult>;
type VirtualFn = fn(&GameData, &mut ParsedItem) -> Result<()>;

enum Parser {
    Section(SectionFn),
    Virtual(VirtualFn),
}
use Parser::{Section, Virtual};

const PARSERS: &[Parser] = &[
    Section(parse_unidentified),
    Virtual(parse_superior),
    Virtual(parse_foulborn),
    Virtual(parse_vestigial),
    Section(parse_synthesised),
    Section(parse_category_by_help_text),
    Virtual(parse_map_tier),
    Virtual(normalize_name),
    Virtual(find_in_database),
    // -----------
    Section(parse_item_level),
    Section(parse_talisman_tier),
    Section(parse_gem),
    Section(parse_vaal_gem),
    Section(parse_armour),
    Section(parse_weapon),
    Section(parse_accessory),
    Section(parse_flask),
    Section(parse_tincture),
    Section(parse_stack_size),
    Section(parse_corrupted),
    Section(parse_imbued_gem),
    Section(parse_foil),
    Section(parse_influence),
    Section(parse_map),
    Section(parse_sockets),
    Section(parse_heist_contract),
    Section(parse_heist_blueprint),
    Section(parse_chart),
    Section(parse_area_level),
    Section(parse_atzoatl_rooms),
    Section(parse_filled_coffin),
    Section(parse_mirrored),
    Section(parse_split),
    Section(parse_sentinel_charge),
    Section(parse_scrying_orb),
    Section(parse_mercenary),
    Section(parse_logbook_area),
    Section(parse_logbook_area),
    Section(parse_logbook_area),
    Section(parse_mercenary_gems),
    Section(parse_mercenary_gems),
    Section(parse_mercenary_gems),
    Section(parse_mercenary_gems),
    Section(parse_mercenary_gems),
    Section(parse_mercenary_gems),
    Section(parse_modifiers), // enchant
    Section(parse_modifiers), // scourge
    Section(parse_modifiers), // implicit
    Section(parse_modifiers), // explicit
    Virtual(augment_modifiers),
    Virtual(transform_to_legacy_modifiers),
    Virtual(parse_fractured),
    Virtual(pick_correct_variant),
    Virtual(calc_disenchant_dust),
    Virtual(calc_base_percentile),
];

const VALUE_AUGMENTED: &str = " (augmented)";

/// `parseClipboard`
pub fn parse_clipboard(data: &GameData, clipboard: &str) -> Result<ParsedItem, ParseError> {
    let run = std::panic::AssertUnwindSafe(|| parse_clipboard_inner(data, clipboard));
    match std::panic::catch_unwind(run) {
        Ok(Ok(item)) => Ok(item),
        Ok(Err(e)) if e.is::<UnknownItem>() => Err(ParseError::Unknown),
        Ok(Err(_)) | Err(_) => Err(ParseError::Parse),
    }
}

fn parse_clipboard_inner(d: &GameData, clipboard: &str) -> Result<ParsedItem> {
    let mut sections = item_text_to_sections(clipboard);

    if sections.first().and_then(|s| s.get(2)).map(String::as_str) == Some(d.cs.CANNOT_USE_ITEM.as_str()) {
        sections[0].pop(); // remove CANNOT_USE_ITEM line
        let first = sections[0].clone();
        // prepend item class & rarity into second section
        sections
            .get_mut(1)
            .ok_or_else(|| anyhow!("no second section"))?
            .splice(0..0, first);
        sections.remove(0); // remove first section where CANNOT_USE_ITEM line was
    }
    let nameplate = sections.first_mut().ok_or_else(|| anyhow!("empty item"))?;
    let mut item = parse_name_plate(d, nameplate)?;

    sections.remove(0);
    item.raw_text = clipboard.to_string();

    // each section can be parsed at most by one parser
    for parser in PARSERS {
        match parser {
            Virtual(f) => f(d, &mut item)?,
            Section(f) => {
                let mut i = 0;
                while i < sections.len() {
                    // Workaround: eldritch influence lines are appended to whichever section comes last,
                    // so strip them before any parser looks at the section.
                    parse_eldritch_item(d, &mut sections[i]);

                    match f(d, &sections[i], &mut item)? {
                        Parsed => {
                            sections.remove(i);
                            break;
                        }
                        ParserSkipped => break,
                        Skipped => i += 1,
                    }
                }
            }
        }
    }
    Ok(item)
}

fn item_text_to_sections(text: &str) -> Vec<Vec<String>> {
    let mut lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }

    let mut sections: Vec<Vec<String>> = vec![vec![]];
    for line in lines {
        if line == "--------" {
            sections.push(vec![]);
        } else {
            sections.last_mut().unwrap().push(line.to_string());
        }
    }
    sections.into_iter().filter(|s| !s.is_empty()).collect()
}

/// `section[0].…`, which throws when the section is empty.
fn s0(section: &[String]) -> Result<&str> {
    section
        .first()
        .map(String::as_str)
        .ok_or_else(|| anyhow!("empty section"))
}

/// `section[0] === x`
fn first_is(section: &[String], x: &str) -> bool {
    section.first().map(String::as_str) == Some(x)
}

/// The rest of `line` after `prefix`, if it starts with it.
fn after<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.strip_prefix(prefix)
}

fn find_info(infos: &Option<Vec<BaseType>>) -> Option<&BaseType> {
    infos.as_ref().and_then(|v| v.first())
}

fn markup_condition_re() -> &'static (Regex, Regex) {
    static RE: OnceLock<(Regex, Regex)> = OnceLock::new();
    RE.get_or_init(|| {
        (
            Regex::new(r"<<set:.+?>>").unwrap(),
            Regex::new(r"<(if:.+?|elif:.+?|else)>\{(.+?)\}").unwrap(),
        )
    })
}

fn markup_condition_parser(text: &str) -> String {
    // ignores state set by <<set:__>>
    // always evaluates first condition to true <if:__>{...}
    let (set, cond) = markup_condition_re();
    let text = set.replace_all(text, "");
    cond.replace_all(&text, |c: &regex::Captures| {
        if c[1].starts_with("if:") {
            c[2].to_string()
        } else {
            String::new()
        }
    })
    .into_owned()
}

fn parse_name_plate(d: &GameData, section: &mut Vec<String>) -> Result<ParsedItem> {
    let cs = &d.cs;
    let mut lines = std::mem::take(section).into_iter();

    let line = lines.next();
    if !line.is_some_and(|l| l.starts_with(&cs.ITEM_CLASS)) {
        bail!("item.parse_error");
    }

    let mut line = lines.next();
    let mut rarity_text: Option<String> = None;
    if let Some(rest) = line.as_deref().and_then(|l| after(l, &cs.RARITY)) {
        rarity_text = Some(rest.to_string());
        line = lines.next();
    }

    let name = match line {
        Some(l) => markup_condition_parser(&l),
        None => bail!("item.parse_error"),
    };

    let base_type = lines.next().map(|l| markup_condition_parser(&l));

    let mut item = ParsedItem {
        name,
        base_type,
        ..Default::default()
    };

    if let Some(rarity) = rarity_text {
        let r = rarity.as_str();
        if r == cs.RARITY_CURRENCY {
            item.category = Some(ItemCategory::Currency.into());
        } else if r == cs.RARITY_DIVCARD {
            item.category = Some(ItemCategory::DivinationCard.into());
        } else if r == cs.RARITY_GEM {
            item.category = Some(ItemCategory::Gem.into());
        } else if r == cs.RARITY_NORMAL || r == cs.RARITY_QUEST {
            item.rarity = Some(ItemRarity::Normal);
        } else if r == cs.RARITY_MAGIC {
            item.rarity = Some(ItemRarity::Magic);
        } else if r == cs.RARITY_RARE {
            item.rarity = Some(ItemRarity::Rare);
        } else if r == cs.RARITY_UNIQUE {
            item.rarity = Some(ItemRarity::Unique);
        }
    }
    Ok(item)
}

fn normalize_name(d: &GameData, item: &mut ParsedItem) -> Result<()> {
    let cs = &d.cs;
    if item.rarity == Some(ItemRarity::Magic) {
        if let Some(base_type) = magic_name::magic_basetype(&d.items, &item.name) {
            item.name = base_type;
        }
    }

    if is(&item.category, ItemCategory::MetamorphSample) {
        let name = item.name.clone();
        if cs.METAMORPH_BRAIN.is_match(&name) {
            item.name = "Metamorph Brain".into();
        } else if cs.METAMORPH_EYE.is_match(&name) {
            item.name = "Metamorph Eye".into();
        } else if cs.METAMORPH_LUNG.is_match(&name) {
            item.name = "Metamorph Lung".into();
        } else if cs.METAMORPH_HEART.is_match(&name) {
            item.name = "Metamorph Heart".into();
        } else if cs.METAMORPH_LIVER.is_match(&name) {
            item.name = "Metamorph Liver".into();
        }
    }
    Ok(())
}

fn find_in_database(d: &GameData, item: &mut ParsedItem) -> Result<()> {
    let base_or_name = item.base_type.clone().unwrap_or_else(|| item.name.clone());
    let mut info: Option<Vec<BaseType>> = if is(&item.category, ItemCategory::DivinationCard) {
        d.items.by_translated("DIVINATION_CARD", &item.name)
    } else if is(&item.category, ItemCategory::CapturedBeast) {
        d.items.by_translated("CAPTURED_BEAST", &base_or_name)
    } else if is(&item.category, ItemCategory::Gem) || is(&item.category, ItemCategory::MetamorphSample) {
        d.items.by_translated(
            if is(&item.category, ItemCategory::Gem) {
                "GEM"
            } else {
                "ITEM"
            },
            &item.name,
        )
    } else if is(&item.category, ItemCategory::Voidstone) {
        d.items.by_ref("ITEM", "Charged Compass")
    } else if item.rarity == Some(ItemRarity::Unique) && !item.is_unidentified {
        d.items.by_translated("UNIQUE", &item.name)
    } else {
        d.items.by_translated("ITEM", &base_or_name)
    };

    // Viscera names carry a tier prefix ("Lesser Crustacean's Claw") that
    // neither the data nor the trade API has, so look the name up again without its first word.
    let is_viscera = item
        .raw_text
        .lines()
        .next()
        .and_then(|l| after(l, &d.cs.ITEM_CLASS))
        .is_some_and(|class| class == "Viscera");
    if is_viscera && info.as_ref().is_none_or(Vec::is_empty) {
        if let Some((_, rest)) = base_or_name.split_once(' ') {
            info = d.items.by_translated("ITEM", rest);
        }
    }

    let infos = match info.as_mut() {
        Some(v) if !v.is_empty() => v,
        _ => return Err(UnknownItem.into()),
    };
    if infos[0].unique.is_some() {
        // a missing base type or name becomes the text "undefined" in the lookup string; kept so the same items fail the same way
        let base_name = item.base_type.as_deref().unwrap_or("undefined");
        let base_types = d.items.by_translated("ITEM", base_name);
        let Some(base_ref) = find_info(&base_types).map(|b| b.ref_name.clone()) else {
            return Err(UnknownItem.into());
        };
        infos.retain(|i| i.unique.as_ref().is_some_and(|u| u.base == base_ref));
    }
    // only a check for "no result" before the unique filter above; with no
    // variants left it crashes on the missing `info`, which is a parse error
    if infos.is_empty() {
        bail!("no variants of the unique left");
    }
    item.info_variants = info.unwrap();
    // choose 1st variant, correct one will be picked at the end of parsing
    item.info = item.info_variants.first().cloned().unwrap_or_default();
    // same for every variant
    if item.category.is_none() {
        if let Some(c) = &item.info.craftable {
            item.category = Some(c.category.clone());
            // Viscera have no "Rarity:" line, and without a rarity no modifier is read
            if item.rarity.is_none() && matches!(&c.category, Category::Other(o) if o == "Viscera") {
                item.rarity = Some(ItemRarity::Normal);
            }
        } else if let Some(u) = &item.info.unique {
            let base = d
                .items
                .by_ref("ITEM", &u.base)
                .ok_or_else(|| anyhow!("unique base missing"))?;
            let craftable = base
                .first()
                .and_then(|b| b.craftable.as_ref())
                .ok_or_else(|| anyhow!("base has no category"))?;
            item.category = Some(craftable.category.clone());
        }
    }
    Ok(())
}

fn parse_map_tier(d: &GameData, item: &mut ParsedItem) -> Result<()> {
    let text = match item.base_type.as_deref() {
        Some(b) if !b.is_empty() => b.to_string(),
        _ => item.name.clone(),
    };
    let Some(m) = d.cs.MAP_TIER.captures(&text) else {
        return Ok(());
    };

    item.map_tier = Some(js::number(&m[1]));
    let matched = m[0].to_string();

    match item.base_type.as_deref() {
        Some(b) if !b.is_empty() => item.base_type = Some(b.replacen(&matched, "", 1)),
        _ => item.name = item.name.replacen(&matched, "", 1),
    }
    Ok(())
}

fn parse_area_prop_nested(d: &GameData, line: &str, item: &mut ParsedItem) -> bool {
    let cs = &d.cs;
    if let Some(v) = after(line, &cs.MAP_ITEM_QUANTITY) {
        item.area_item_quantity = Some(js::parse_int(v));
        true
    } else if let Some(v) = after(line, &cs.MAP_ITEM_RARITY) {
        item.area_item_rarity = Some(js::parse_int(v));
        true
    } else if let Some(v) = after(line, &cs.MAP_MONSTER_PACK_SIZE) {
        item.area_pack_size = Some(js::parse_int(v));
        true
    } else {
        false
    }
}

fn parse_map(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    let cs = &d.cs;
    if !is(&item.category, ItemCategory::Map) {
        return Ok(ParserSkipped);
    }

    let mut result = Skipped;

    for line in section {
        if parse_area_prop_nested(d, line, item) {
            result = Parsed;
        } else if let Some(v) = after(line, &cs.MAP_MORE_MAPS) {
            item.map_more_maps = Some(js::parse_int(v));
            result = Parsed;
        } else if let Some(v) = after(line, &cs.MAP_MORE_CURRENCY) {
            item.map_more_currency = Some(js::parse_int(v));
            result = Parsed;
        } else if let Some(v) = after(line, &cs.MAP_MORE_DIVINATION_CARDS) {
            item.map_more_div_cards = Some(js::parse_int(v));
            result = Parsed;
        } else if line.starts_with(&cs.MAP_AREA) {
            // the area name is read from the section's first line, not the matching line
            let area_name = s0(section)?.get(cs.MAP_AREA.len()..).unwrap_or("");
            let area = d
                .items
                .by_translated("AREA", area_name)
                .ok_or_else(|| anyhow!("Unknown Area name."))?;
            item.map_area = area.into_iter().next();
            result = Parsed;
        } else if let Some(m) = cs.MAP_COMPLETION_REWARD.captures(line) {
            let reward = d
                .items
                .by_translated("UNIQUE", &m[1])
                .ok_or_else(|| anyhow!("Unknown Unique Item."))?;
            item.map_completion_reward = reward.into_iter().next();
            result = Parsed;
        }
    }

    Ok(result)
}

fn parse_fractured(_: &GameData, item: &mut ParsedItem) -> Result<()> {
    if item.new_mods.iter().any(|m| m.info.type_ == ModifierType::Fractured) {
        item.is_fractured = Some(true);
    }
    Ok(())
}

fn pick_correct_variant(d: &GameData, item: &mut ParsedItem) -> Result<()> {
    let variants = item.info_variants.clone();
    item.info = _pick_correct_variant(&variants, item)?
        .or_else(|| variants.first())
        .cloned()
        .unwrap_or_default();
    if let Some(unique) = &item.info.unique {
        let base_variants = d
            .items
            .by_ref("ITEM", &unique.base)
            .ok_or_else(|| anyhow!("unique base missing"))?;
        item.unique_base = _pick_correct_variant(&base_variants, item)?
            .or_else(|| base_variants.first())
            .cloned();
    }
    Ok(())
}

fn _pick_correct_variant<'a>(variants: &'a [BaseType], item: &ParsedItem) -> Result<Option<&'a BaseType>> {
    if variants.len() <= 1 {
        return Ok(variants.first());
    }

    let truthy = |v: Option<f64>| v.is_some_and(|v| v != 0.0 && !v.is_nan());

    'variant: for variant in variants {
        let cond = variant
            .disc
            .as_ref()
            .ok_or_else(|| anyhow!("variant without conditions"))?;

        if cond.prop_ar.is_some() && !truthy(item.armour_ar) {
            continue;
        }
        if cond.prop_ev.is_some() && !truthy(item.armour_ev) {
            continue;
        }
        if cond.prop_es.is_some() && !truthy(item.armour_es) {
            continue;
        }

        if let Some(tier) = &cond.map_tier {
            let Some(map_tier) = item.map_tier.filter(|t| *t != 0.0 && !t.is_nan()) else {
                continue;
            };
            match tier.as_str() {
                "W" if map_tier > 5.0 => continue,
                "Y" if !(6.0..=10.0).contains(&map_tier) => continue,
                "R" if map_tier < 11.0 => continue,
                _ => {}
            }
        }

        if let Some(r) = &cond.has_implicit {
            if !item
                .stats_by_type
                .iter()
                .any(|c| c.type_ == ModifierType::Implicit && c.stat.ref_ == r.ref_)
            {
                continue 'variant;
            }
        }
        if let Some(r) = &cond.has_explicit {
            if !item
                .stats_by_type
                .iter()
                .any(|c| c.type_ == ModifierType::Explicit && c.stat.ref_ == r.ref_)
            {
                continue 'variant;
            }
        }

        if let Some(text) = &cond.section_text {
            if !item.raw_text.contains(text.as_str()) {
                continue;
            }
        }

        return Ok(Some(variant));
    }

    // it may happen that we don't find correct variant
    // i.e. corrupted implicit on Two-Stone Ring
    Ok(None)
}

fn parse_influence(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    let cs = &d.cs;
    if section.len() <= 2 {
        let count_before = item.influences.len();

        for line in section {
            let l = line.as_str();
            if l == cs.INFLUENCE_CRUSADER {
                item.influences.push(ItemInfluence::Crusader);
            } else if l == cs.INFLUENCE_ELDER {
                item.influences.push(ItemInfluence::Elder);
            } else if l == cs.INFLUENCE_SHAPER {
                item.influences.push(ItemInfluence::Shaper);
            } else if l == cs.INFLUENCE_HUNTER {
                item.influences.push(ItemInfluence::Hunter);
            } else if l == cs.INFLUENCE_REDEEMER {
                item.influences.push(ItemInfluence::Redeemer);
            } else if l == cs.INFLUENCE_WARLORD {
                item.influences.push(ItemInfluence::Warlord);
            }
        }

        if count_before < item.influences.len() {
            return Ok(Parsed);
        }
    }
    Ok(Skipped)
}

fn parse_corrupted(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if first_is(section, &d.cs.CORRUPTED) {
        item.is_corrupted = true;
        return Ok(Parsed);
    } else if first_is(section, &d.cs.UNMODIFIABLE) {
        item.is_corrupted = true;
        item.is_unmodifiable = Some(true);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_foil(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if item.rarity != Some(ItemRarity::Unique) {
        return Ok(ParserSkipped);
    }
    if first_is(section, &d.cs.FOIL_UNIQUE) {
        item.is_foil = Some(true);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_unidentified(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if first_is(section, &d.cs.UNIDENTIFIED) {
        item.is_unidentified = true;
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_item_level(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    let prefix = if item.info.ref_name == "Filled Coffin" {
        &d.cs.CORPSE_LEVEL
    } else {
        &d.cs.ITEM_LEVEL
    };

    for line in section {
        if let Some(v) = after(line, prefix) {
            item.item_level = Some(js::number(v));
            return Ok(Parsed);
        }
    }
    Ok(Skipped)
}

fn parse_talisman_tier(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if let Some(v) = after(s0(section)?, &d.cs.TALISMAN_TIER) {
        item.talisman_tier = Some(js::number(v));
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_vaal_gem(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if !is(&item.category, ItemCategory::Gem) {
        return Ok(ParserSkipped);
    }

    if section.len() == 1 {
        if let Some(gem_info) = d.items.by_translated("GEM", &section[0]) {
            item.vaal_gem = gem_info.into_iter().next();
            return Ok(Parsed);
        }
    }
    Ok(Skipped)
}

fn parse_gem(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if !is(&item.category, ItemCategory::Gem) {
        return Ok(ParserSkipped);
    }
    if let Some(v) = section.get(1).and_then(|l| after(l, &d.cs.GEM_LEVEL)) {
        // "Level: 20 (Max)"
        item.gem_level = Some(js::parse_int(v));

        parse_quality_nested(d, section, item);

        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_imbued_gem(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if !is(&item.category, ItemCategory::Gem) {
        return Ok(ParserSkipped);
    }

    if section.len() == 1 {
        let Some((stat, matcher)) = d.stats.by_match_str(&section[0]) else {
            return Ok(Skipped);
        };

        item.new_mods.push(ParsedModifier {
            info: ModifierInfo::new(ModifierType::Imbued),
            stats: vec![ParsedStat {
                stat,
                translation: matcher,
                roll: None,
            }],
        });
        item.imbued_gem = Some(true);

        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_stack_size(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if item.rarity != Some(ItemRarity::Normal)
        && !is(&item.category, ItemCategory::Currency)
        && !is(&item.category, ItemCategory::DivinationCard)
    {
        return Ok(ParserSkipped);
    }
    if let Some(v) = after(s0(section)?, &d.cs.STACK_SIZE) {
        // Portal Scroll "Stack Size: 2[localized separator]448/40"
        let digits: String = v.chars().filter(|c| c.is_ascii_digit() || *c == '/').collect();
        let mut parts = digits.split('/').map(js::number);
        item.stack_size = Some(StackSize {
            value: parts.next().unwrap_or(f64::NAN),
            max: parts.next(),
        });

        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_sockets(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if let Some(v) = after(s0(section)?, &d.cs.SOCKETS) {
        let sockets = v.trim_end();

        let mut result = Sockets {
            white: sockets.matches('W').count() as f64,
            linked: None,
        };

        let pattern: String = sockets
            .chars()
            .map(|c| if c == ' ' || c == '-' { c } else { '#' })
            .collect();
        if pattern == "#-#-#-#-#-#" {
            result.linked = Some(6.0);
        } else if pattern == "# #-#-#-#-#" || pattern == "#-#-#-#-# #" || pattern == "#-#-#-#-#" {
            result.linked = Some(5.0);
        }
        item.sockets = Some(result);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_quality_nested(d: &GameData, section: &[String], item: &mut ParsedItem) -> bool {
    for line in section {
        if let Some(v) = after(line, &d.cs.QUALITY) {
            // "Quality: +20% (augmented)"
            item.quality = Some(js::parse_int(v));
            return true;
        }
    }
    false
}

fn parse_memory_strands_nested(d: &GameData, section: &[String], item: &mut ParsedItem) -> bool {
    for line in section {
        if let Some(v) = after(line, &d.cs.MEMORY_STRANDS) {
            item.memory_strands = Some(js::parse_int(v));
            return true;
        }
    }
    false
}

fn parse_armour(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    let cs = &d.cs;
    let mut result = Skipped;

    for line in section {
        if let Some(v) = after(line, &cs.ARMOUR) {
            item.armour_ar = Some(js::parse_int(v));
        } else if let Some(v) = after(line, &cs.EVASION) {
            item.armour_ev = Some(js::parse_int(v));
        } else if let Some(v) = after(line, &cs.ENERGY_SHIELD) {
            item.armour_es = Some(js::parse_int(v));
        } else if let Some(v) = after(line, &cs.TAG_WARD) {
            item.armour_ward = Some(js::parse_int(v));
        } else if let Some(v) = after(line, &cs.BLOCK_CHANCE) {
            item.armour_block = Some(js::parse_int(v));
        } else {
            continue;
        }
        result = Parsed;
    }

    if result == Parsed {
        parse_quality_nested(d, section, item);
        parse_memory_strands_nested(d, section, item);
    }

    Ok(result)
}

/// `str.split('-').map(parseInt)` averaged.
fn range_avg(text: &str) -> f64 {
    let values: Vec<f64> = text.split('-').map(js::parse_int).collect();
    roll_or_minmax_avg(&values)
}

fn parse_weapon(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    let cs = &d.cs;
    let mut result = Skipped;

    for line in section {
        if let Some(v) = after(line, &cs.CRIT_CHANCE) {
            item.weapon_crit = Some(js::parse_float(v));
        } else if let Some(v) = after(line, &cs.ATTACK_SPEED) {
            item.weapon_as = Some(js::parse_float(v));
        } else if let Some(v) = after(line, &cs.PHYSICAL_DAMAGE) {
            item.weapon_physical = Some(range_avg(v));
        } else if let Some(v) = after(line, &cs.ELEMENTAL_DAMAGE) {
            item.weapon_elemental = Some(v.split(", ").map(range_avg).fold(0.0, |sum, x| sum + x));
        } else {
            continue;
        }
        result = Parsed;
    }

    if result == Parsed {
        parse_quality_nested(d, section, item);
        parse_memory_strands_nested(d, section, item);
    }

    Ok(result)
}

fn parse_accessory(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    let jewellery = known(&item.category).is_some_and(is_jewellery);
    if !jewellery && !is(&item.category, ItemCategory::Quiver) {
        return Ok(ParserSkipped);
    }

    let mut result = Skipped;

    for line in section {
        if let Some(text) = line.strip_suffix(VALUE_AUGMENTED) {
            let stat = StatString {
                string: text.to_string(),
                unscalable: true,
            };
            if let Some(found) = try_parse_translation(d, &stat, ModifierType::Pseudo, None)? {
                if found.stat.jewellery_quality.is_some() {
                    item.quality = Some(found.roll.as_ref().ok_or_else(|| anyhow!("no roll"))?.value);
                    item.new_mods.push(ParsedModifier {
                        info: ModifierInfo::new(ModifierType::Pseudo),
                        stats: vec![found],
                    });
                    result = Parsed;
                }
            }
        }
    }

    if parse_memory_strands_nested(d, section, item) {
        result = Parsed;
    }

    Ok(result)
}

fn pseudo_mod(stat: crate::data::Stat, matcher: crate::data::StatMatcher, type_: ModifierType) -> ParsedModifier {
    ParsedModifier {
        info: ModifierInfo::new(type_),
        stats: vec![ParsedStat {
            stat,
            translation: matcher,
            roll: None,
        }],
    }
}

fn parse_logbook_area(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if item.info.ref_name != "Expedition Logbook" {
        return Ok(ParserSkipped);
    }
    if section.len() < 3 {
        return Ok(Skipped);
    }

    // skip Logbook Area line, parse Faction
    let Some((faction, faction_matcher)) = d.stats.by_match_str(&section[1]) else {
        return Ok(Skipped);
    };
    if !faction.ref_.starts_with("Has Logbook Faction:") {
        return Ok(Skipped);
    }

    let mut area_mods = vec![pseudo_mod(faction, faction_matcher, ModifierType::Pseudo)];

    let (mod_type, lines) = parse_mod_type(&section[2..])?;
    for line in &lines {
        // Area contains an Expedition Boss (#)
        if let Some((stat, matcher)) = d.stats.by_match_str(line) {
            if stat.better == 0 {
                area_mods.push(pseudo_mod(stat, matcher, mod_type));
            }
        }
    }

    item.logbook_area_mods.get_or_insert_with(Vec::new).push(area_mods);

    Ok(Parsed)
}

fn parse_mercenary(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    let cs = &d.cs;
    if item.info.ref_name != "Mercenary Warrant" {
        return Ok(ParserSkipped);
    }

    for line in section {
        if let Some(v) = after(line, &cs.MERCENARY_LEVEL) {
            item.item_level = Some(js::number(v));
        } else if let Some(v) = after(line, &cs.MERCENARY_BUILD) {
            let mut build = d
                .items
                .by_translated("MERCENARY_BUILD", v)
                .ok_or_else(|| anyhow!("Unknown Mercenary Build."))?;
            if let Some(ref_name) = build
                .first()
                .and_then(|b| b.mercenary_build.as_ref())
                .and_then(|m| m.as_str())
            {
                build = d
                    .items
                    .by_ref("MERCENARY_BUILD", ref_name)
                    .ok_or_else(|| anyhow!("missing build"))?;
            }
            item.mercenary_build = build.into_iter().next();
        }
    }

    if item.mercenary_build.is_some() {
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_mercenary_gems(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if item.info.ref_name != "Mercenary Warrant" {
        return Ok(ParserSkipped);
    }

    let warrant: Option<Category> = Some(ItemCategory::MercenaryWarrant.into());
    let first = s0(section)?;
    let Some(skill) = try_parse_translation(
        d,
        &StatString {
            string: first.to_string(),
            unscalable: true,
        },
        ModifierType::Pseudo,
        warrant.as_ref(),
    )?
    else {
        return Ok(Skipped);
    };

    let mut group = vec![skill];

    for line in &section[1..] {
        let support = try_parse_translation(
            d,
            &StatString {
                string: line.clone(),
                unscalable: true,
            },
            ModifierType::Pseudo,
            warrant.as_ref(),
        )?;
        let unknown = match &support {
            None => true,
            Some(s) => {
                let m = s
                    .stat
                    .mercenary
                    .as_ref()
                    .ok_or_else(|| anyhow!("mercenary stat without mercenary info"))?;
                m.synthetic_family == Some(true) && m.tier != Some(3.0)
            }
        };
        if let Some(s) = support {
            group.push(s);
        }
        if unknown {
            item.unknown_modifiers.push(UnknownModifier {
                text: format!("{line} [{first}]"),
                type_: ModifierType::Pseudo,
            });
        }
    }

    item.mercenary_skills.get_or_insert_with(Vec::new).push(group);

    Ok(Parsed)
}

fn parse_modifiers(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if item.rarity.is_none() {
        return Ok(ParserSkipped);
    }

    let recognized = section
        .iter()
        .find(|l| l.ends_with(ENCHANT_LINE) || l.ends_with(SCOURGE_LINE) || is_mod_info_line(l));
    let Some(recognized) = recognized else {
        return Ok(Skipped);
    };

    if is_mod_info_line(recognized) {
        for group in group_lines_by_mod(section) {
            let mut mod_info = parse_mod_info_line(&d.cs, &group.mod_line)?;
            let veiled = group
                .stat_lines
                .first()
                .is_some_and(|l| *l == d.cs.VEILED_PREFIX || *l == d.cs.VEILED_SUFFIX);
            if veiled {
                mod_info.type_ = ModifierType::Veiled;
                item.is_veiled = Some(true);
            }
            parse_stats_from_mod(
                d,
                &group.stat_lines,
                item,
                ParsedModifier {
                    info: mod_info,
                    stats: vec![],
                },
            )?;
        }
    } else {
        let (mod_type, lines) = parse_mod_type(section)?;
        parse_stats_from_mod(
            d,
            &lines,
            item,
            ParsedModifier {
                info: ModifierInfo::new(mod_type),
                stats: vec![],
            },
        )?;
    }

    Ok(Parsed)
}

fn parse_mirrored(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if first_is(section, &d.cs.MIRRORED) {
        item.is_mirrored = Some(true);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_split(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if section.len() == 1 && section[0] == d.cs.SPLIT {
        item.is_split = Some(true);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_flask(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if !is(&item.category, ItemCategory::Flask) {
        return Ok(ParserSkipped);
    }

    // the purpose of this parser is to "consume" flask buffs
    // so they are not recognized as modifiers
    if section.iter().any(|l| d.cs.FLASK_CHARGES.is_match(l)) {
        parse_quality_nested(d, section, item);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_tincture(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if !is(&item.category, ItemCategory::Tincture) {
        return Ok(ParserSkipped);
    }

    if parse_quality_nested(d, section, item) {
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_sentinel_charge(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if !is(&item.category, ItemCategory::Sentinel) {
        return Ok(ParserSkipped);
    }

    if section.len() == 1 {
        if let Some(v) = after(&section[0], &d.cs.SENTINEL_CHARGE) {
            item.sentinel_charge = Some(js::parse_int(v));
            return Ok(Parsed);
        }
    }
    Ok(Skipped)
}

fn parse_scrying_orb(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if item.info.ref_name != "Scrying Orb" {
        return Ok(ParserSkipped);
    }

    if section.len() == 1 {
        if let Some(area_name) = after(&section[0], &d.cs.MAP_AREA) {
            let area = d
                .items
                .by_translated("AREA", area_name)
                .ok_or_else(|| anyhow!("Unknown Area name."))?;
            item.map_area = area.into_iter().next();
            return Ok(Parsed);
        }
    }
    Ok(Skipped)
}

fn parse_eldritch_item(d: &GameData, section: &mut Vec<String>) {
    // these don't have a dedicated section and are appended to last-ish one,
    // it can be Explicit Mods section, Corrupted, or Mirrored line.
    while let Some(last) = section.last() {
        if *last == d.cs.ITEM_EATER || *last == d.cs.ITEM_EXARCH {
            section.pop();
        } else {
            break;
        }
    }
}

fn parse_synthesised(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if section.len() == 1 && section[0] == d.cs.SECTION_SYNTHESISED {
        item.is_synthesised = Some(true);
        let strip = |text: &str| -> Result<String> {
            let m =
                d.cs.ITEM_SYNTHESISED
                    .captures(text)
                    .ok_or_else(|| anyhow!("not a synthesised name"))?;
            Ok(m[1].to_string())
        };
        match item.base_type.as_deref() {
            Some(b) if !b.is_empty() => item.base_type = Some(strip(b)?),
            _ => item.name = strip(&item.name)?,
        }
        return Ok(Parsed);
    }

    Ok(Skipped)
}

fn parse_superior(d: &GameData, item: &mut ParsedItem) -> Result<()> {
    let applies = match item.rarity {
        Some(ItemRarity::Normal) => true,
        Some(ItemRarity::Magic | ItemRarity::Rare | ItemRarity::Unique) => item.is_unidentified,
        None => false,
    };
    if applies {
        if let Some(m) = d.cs.ITEM_SUPERIOR.captures(&item.name) {
            item.name = m[1].to_string();
        }
    }
    Ok(())
}

fn parse_foulborn(d: &GameData, item: &mut ParsedItem) -> Result<()> {
    if item.rarity != Some(ItemRarity::Unique) || item.is_unidentified {
        return Ok(());
    }

    if let Some(m) = d.cs.FOULBORN_NAME.captures(&item.name) {
        item.name = m[1].to_string();
        item.is_foulborn = Some(true);
    }
    Ok(())
}

fn parse_vestigial(d: &GameData, item: &mut ParsedItem) -> Result<()> {
    let Some(base_type) = item.base_type.as_deref().filter(|b| !b.is_empty()) else {
        return Ok(());
    };
    if item.rarity != Some(ItemRarity::Unique) {
        return Ok(());
    }

    if let Some(m) = d.cs.VESTIGIAL_NAME.captures(base_type) {
        item.base_type = Some(m[1].to_string());
        item.is_vestigial = Some(true);
    }
    Ok(())
}

fn parse_category_by_help_text(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if first_is(section, &d.cs.BEAST_HELP) {
        item.category = Some(ItemCategory::CapturedBeast.into());
        return Ok(Parsed);
    } else if first_is(section, &d.cs.METAMORPH_HELP) {
        item.category = Some(ItemCategory::MetamorphSample.into());
        return Ok(Parsed);
    } else if first_is(section, &d.cs.VOIDSTONE_HELP) {
        item.category = Some(ItemCategory::Voidstone.into());
        return Ok(Parsed);
    }

    Ok(Skipped)
}

fn parse_heist_contract(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    let cs = &d.cs;
    if !is(&item.category, ItemCategory::HeistContract) {
        return Ok(ParserSkipped);
    }

    if !parse_area_level_nested(d, section, item) {
        return Ok(Skipped);
    }

    let mut contract = HeistContract::default();

    for line in section {
        if let Some(m) = cs.HEIST_CONTRACT_JOB.captures(line) {
            let job = &m["job"];
            let jobs: [(&String, &'static str); 9] = [
                (&cs.HEIST_JOB_LOCKPICKING, "Lockpicking"),
                (&cs.HEIST_JOB_BRUTEFORCE, "Brute Force"),
                (&cs.HEIST_JOB_PERCEPTION, "Perception"),
                (&cs.HEIST_JOB_DEMOLITION, "Demolition"),
                (&cs.HEIST_JOB_COUNTERTHAUMATURGY, "Counter-Thaumaturgy"),
                (&cs.HEIST_JOB_TRAPDISARMAMENT, "Trap Disarmament"),
                (&cs.HEIST_JOB_AGILITY, "Agility"),
                (&cs.HEIST_JOB_DECEPTION, "Deception"),
                (&cs.HEIST_JOB_ENGINEERING, "Engineering"),
            ];
            if let Some((_, name)) = jobs.iter().find(|(translated, _)| translated.as_str() == job) {
                contract.required_job = Some(name);
            }
            contract.job_level = Some(js::number(&m["level"]));
            continue;
        }

        if let Some(m) = cs.HEIST_CONTRACT_TARGET.captures(line) {
            if m[1] == cs.HEIST_TARGET_PRICELESS {
                contract.target_value = Some("Priceless");
            }
            continue;
        }
    }

    item.heist_contract = Some(contract);
    Ok(Parsed)
}

fn parse_heist_blueprint(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    let cs = &d.cs;
    if !is(&item.category, ItemCategory::HeistBlueprint) {
        return Ok(ParserSkipped);
    }

    if !parse_area_level_nested(d, section, item) {
        return Ok(Skipped);
    }

    let mut blueprint = HeistBlueprint::default();

    for line in section {
        if let Some(target) = after(line, &cs.HEIST_BLUEPRINT_TARGET) {
            if target == cs.HEIST_BLUEPRINT_ENCHANTS {
                blueprint.target = Some("Enchants");
            } else if target == cs.HEIST_BLUEPRINT_GEMS {
                blueprint.target = Some("Gems");
            } else if target == cs.HEIST_BLUEPRINT_REPLICAS {
                blueprint.target = Some("Replicas");
            } else if target == cs.HEIST_BLUEPRINT_TRINKETS {
                blueprint.target = Some("Trinkets");
            }
        } else if let Some(wings) = after(line, &cs.HEIST_WINGS_REVEALED) {
            let mut parts = wings.split('/');
            blueprint.wings_revealed = Some(js::parse_int(parts.next().unwrap_or("")));
            blueprint.wings_total = Some(js::parse_int(parts.next().unwrap_or("undefined")));
        }
    }

    item.heist_blueprint = Some(blueprint);
    Ok(Parsed)
}

fn parse_chart(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if !is(&item.category, ItemCategory::Chart) {
        return Ok(ParserSkipped);
    }

    if !parse_area_level_nested(d, section, item) {
        return Ok(Skipped);
    }

    let area = d
        .items
        .by_translated("AREA", s0(section)?)
        .ok_or_else(|| anyhow!("Unknown Area name."))?;
    item.map_area = area.into_iter().next();

    for line in section {
        if parse_area_prop_nested(d, line, item) {
            // line parsed
        } else if let Some(v) = after(line, &d.cs.CHART_SULPHUR) {
            item.chart_sulphur = Some(js::parse_int(v));
        }
    }

    Ok(Parsed)
}

fn parse_area_level_nested(d: &GameData, section: &[String], item: &mut ParsedItem) -> bool {
    for line in section {
        if let Some(v) = after(line, &d.cs.AREA_LEVEL) {
            item.area_level = Some(js::number(v));
            return true;
        }
    }
    false
}

fn parse_area_level(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    let r = item.info.ref_name.as_str();
    if r != "Chronicle of Atzoatl" && r != "Expedition Logbook" && r != "Forbidden Tome" {
        return Ok(ParserSkipped);
    }

    if !parse_area_level_nested(d, section, item) {
        return Ok(Skipped);
    }

    Ok(Parsed)
}

fn parse_atzoatl_rooms(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    let cs = &d.cs;
    if item.info.ref_name != "Chronicle of Atzoatl" {
        return Ok(ParserSkipped);
    }
    if !first_is(section, &cs.INCURSION_OPEN) {
        return Ok(Skipped);
    }

    let mut state = INCURSION_OPEN;
    for line in &section[1..] {
        if *line == cs.INCURSION_OBSTRUCTED {
            state = INCURSION_OBSTRUCTED;
            continue;
        }

        if let Some((stat, matcher)) = d.stats.by_match_str(line) {
            let string = if state == INCURSION_OPEN {
                matcher.string.clone()
            } else {
                format!("{} {}", cs.INCURSION_OBSTRUCTED, matcher.string)
            };
            item.new_mods.push(ParsedModifier {
                info: ModifierInfo::new(ModifierType::Pseudo),
                stats: vec![ParsedStat {
                    stat,
                    translation: crate::data::StatMatcher {
                        string,
                        ..Default::default()
                    },
                    roll: Some(Roll {
                        unscalable: true,
                        generation: None,
                        dp: false,
                        value: state,
                        min: state,
                        max: state,
                    }),
                }],
            });
        } else {
            item.unknown_modifiers.push(UnknownModifier {
                text: line.clone(),
                type_: ModifierType::Pseudo,
            });
        }
    }

    Ok(Parsed)
}

fn parse_filled_coffin(d: &GameData, section: &[String], item: &mut ParsedItem) -> Result<SectionResult> {
    if item.info.ref_name != "Filled Coffin" {
        return Ok(ParserSkipped);
    }
    if !section.iter().any(|l| l.ends_with(IMPLICIT_LINE)) {
        return Ok(Skipped);
    }

    let (_, lines) = parse_mod_type(section)?;
    parse_stats_from_mod(
        d,
        &lines,
        item,
        ParsedModifier {
            info: ModifierInfo::new(ModifierType::Necropolis),
            stats: vec![],
        },
    )?;

    Ok(Parsed)
}

fn parse_stats_from_mod(
    d: &GameData,
    lines: &[String],
    item: &mut ParsedItem,
    mut modifier: ParsedModifier,
) -> Result<()> {
    if modifier.info.type_ == ModifierType::Veiled {
        let name = modifier.info.name.clone();
        // a missing name is looked up as the text "undefined"
        match d.stats.by_match_str(name.as_deref().unwrap_or("undefined")) {
            Some((stat, matcher)) => modifier.stats.push(ParsedStat {
                stat,
                translation: matcher,
                roll: None,
            }),
            None => item.unknown_modifiers.push(UnknownModifier {
                text: name.unwrap_or_else(|| "undefined".into()),
                type_: modifier.info.type_,
            }),
        }
        item.new_mods.push(modifier);
        return Ok(());
    }

    let mod_type = modifier.info.type_;
    let category = item.category.clone();
    let not_parsed = lines_to_stat_strings(&d.cs, lines, &mut |stat| match try_parse_translation(
        d,
        &stat,
        mod_type,
        category.as_ref(),
    )? {
        Some(parsed) => {
            modifier.stats.push(parsed);
            Ok(true)
        }
        None => Ok(false),
    })?;

    item.new_mods.push(modifier);
    item.unknown_modifiers.extend(
        not_parsed
            .into_iter()
            .map(|text| UnknownModifier { text, type_: mod_type }),
    );
    Ok(())
}

fn augment_modifiers(_: &GameData, item: &mut ParsedItem) -> Result<()> {
    let rarity = item.rarity;
    let corrupted = item.is_corrupted;
    let mirrored = item.is_mirrored == Some(true);
    let ring_or_amulet = is(&item.category, ItemCategory::Ring) || is(&item.category, ItemCategory::Amulet);
    let synthesised = item.is_synthesised == Some(true);

    for m in &mut item.new_mods {
        if synthesised && m.info.type_ == ModifierType::Implicit && m.info.mechanic.is_none() {
            m.info.mechanic = Some(ModifierMechanic::Synthesised);
        }

        for stat in &mut m.stats {
            let Some(roll) = stat.roll.as_mut() else { continue };
            if !matches!(roll.generation, Some(RollGeneration::Legacy)) || roll.unscalable {
                continue;
            }

            if rarity == Some(ItemRarity::Unique) && corrupted {
                roll.generation = Some(RollGeneration::Volatile);
            } else if rarity == Some(ItemRarity::Rare) && mirrored && ring_or_amulet {
                roll.generation = Some(RollGeneration::Reflecting);
            }
        }
    }
    Ok(())
}

/// Deprecated, but the filters and price lookups still read `stats_by_type`.
fn transform_to_legacy_modifiers(_: &GameData, item: &mut ParsedItem) -> Result<()> {
    item.stats_by_type = sum_stats_by_mod_type(&item.new_mods);
    Ok(())
}

fn truthy(v: Option<f64>) -> bool {
    v.is_some_and(|v| v != 0.0 && !v.is_nan())
}

fn calc_base_percentile(_: &GameData, item: &mut ParsedItem) -> Result<()> {
    let info = item
        .unique_base
        .as_ref()
        .and_then(|b| b.armour.clone())
        .or_else(|| item.info.armour.clone());
    let Some(info) = info else { return Ok(()) };

    // Base percentile is the same for all defences.
    // Using `AR/EV -> ES -> WARD` order to improve accuracy
    // of calculation (larger rolls = more precise).
    let candidates = [
        (item.armour_ar, info.ar, &calc_q20::ARMOUR),
        (item.armour_ev, info.ev, &calc_q20::EVASION),
        (item.armour_es, info.es, &calc_q20::ENERGY_SHIELD),
        (item.armour_ward, info.ward, &calc_q20::WARD),
    ];
    let hit = candidates
        .iter()
        .find_map(|&(total, bounds, stats)| match (total, bounds) {
            (Some(total), Some(bounds)) if truthy(Some(total)) => Some((total, bounds, stats)),
            _ => None,
        });
    if let Some((total, bounds, stats)) = hit {
        item.base_percentile = Some(calc_q20::calc_prop_percentile(total, bounds, stats, item)?);
    }
    Ok(())
}

fn calc_disenchant_dust(_: &GameData, item: &mut ParsedItem) -> Result<()> {
    let Some(disenchant_value) = item
        .info
        .unique
        .as_ref()
        .and_then(|u| u.disenchant_value)
        .filter(|v| *v != 0.0 && !v.is_nan())
    else {
        return Ok(());
    };

    let mut increase_by_factors = 0.0;

    // +50% per Influence Type
    increase_by_factors += item.influences.len() as f64 * 50.0;

    // +2% per 1% Item Quality
    if truthy(item.quality) {
        increase_by_factors += item.quality.unwrap() * 2.0;
    }

    // +50% per Corruption Implicit
    if item.is_corrupted {
        for m in &item.new_mods {
            if m.info.mechanic == Some(ModifierMechanic::Corruption) {
                increase_by_factors += 50.0;
            }
        }
    }

    let factors_multi = (increase_by_factors + 100.0) / 100.0;
    // an item without an item level makes these NaN
    let ilvl = item.item_level.unwrap_or(f64::NAN);
    let clamp = |lo: f64, hi: f64| js::min(js::max(ilvl, lo), hi);
    let term1 = 50.0; // ilvl 46 and below
    let term2 = 2.0 * (clamp(46.0, 68.0) - 46.0); // ilvl 47 to 68
    let term3 = (3.0 * (clamp(46.0, 68.0) - 46.0) / 11.0).floor(); // ilvl 47 to 68
    let term4 = 25.0 * (clamp(68.0, 84.0) - 68.0); // ilvl 69 to 84
    let total_multi = 5.0 * (term1 + term2 + term3 + term4) * factors_multi;

    item.dust_equivalent = Some((disenchant_value * total_multi).floor());
    Ok(())
}
