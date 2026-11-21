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

use self::advanced::{
    group_lines_by_mod, is_mod_info_line, parse_mod_info_line, parse_mod_type, ModifierInfo, ParsedModifier,
    ENCHANT_LINE, IMPLICIT_LINE, SCOURGE_LINE,
};
use self::item::{
    HeistBlueprint, HeistContract, ItemInfluence, ItemRarity, ParsedItem, Sockets, StackSize, UnknownModifier,
    INCURSION_OBSTRUCTED, INCURSION_OPEN,
};
use self::meta::{is, is_jewellery, known, Category, ItemCategory};
use self::modifiers::{sum_stats_by_mod_type, ModifierMechanic, ModifierType};
use self::stat_translations::{
    lines_to_stat_strings, roll_or_minmax_avg, try_parse_translation, ParsedStat, Roll, RollGeneration, StatString,
};
use crate::data::{BaseType, GameData};
use crate::js;
use regex::Regex;
use std::borrow::Borrow;
use std::sync::Arc;
use std::sync::LazyLock;

/// Why an item could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// `item.parse_error`: not an item, or the parser hit something it can't handle.
    #[error("item.parse_error: {0}")]
    Parse(String),
    /// `item.unknown`: an item that isn't in the game data.
    #[error("item.unknown")]
    Unknown,
}

pub type Result<T, E = ParseError> = std::result::Result<T, E>;

impl ParseError {
    pub fn parse(reason: impl Into<String>) -> Self {
        Self::Parse(reason.into())
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Parse(_) => "item.parse_error",
            Self::Unknown => "item.unknown",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SectionResult {
    /// The section was understood and is consumed.
    Parsed,
    /// Not this parser's section: offer it the next one.
    Skipped,
    /// The parser does not apply to this item: stop offering it sections.
    ParserSkipped,
}
use SectionResult::{Parsed, ParserSkipped, Skipped};

/// Parser scratch state that is not part of the result.
#[derive(Default)]
struct Scratch {
    name: String,
    base_type: Option<String>,
    /// Every database entry the name matched; the correct one is picked at the end.
    info_variants: Vec<BaseType>,
}

/// One section of the item text, borrowed from it.
struct ItemSection<'a> {
    lines: Vec<&'a str>,
    /// Consumed by a parser; later parsers do not see it.
    claimed: bool,
}

type SectionFn = fn(&GameData, &[&str], &mut ParsedItem) -> Result<SectionResult>;
type PlainSectionFn = fn(&GameData, &[&str], &mut ParsedItem) -> SectionResult;
type VirtualFn = fn(&GameData, &mut ParsedItem) -> Result<()>;
type NamedFn = fn(&GameData, &mut ParsedItem, &mut Scratch) -> Result<()>;

enum Parser {
    /// Looks for its section; fails on malformed input.
    Section(SectionFn),
    /// Same, but cannot fail.
    Plain(PlainSectionFn),
    /// A section parser that is run `n` times, claiming one section each time.
    Repeat(usize, SectionFn),
    /// Works on the item only, no section.
    Virtual(VirtualFn),
    /// Same, but cannot fail.
    PlainVirtual(fn(&GameData, &mut ParsedItem)),
    /// Works on the item and the name scratch state.
    Named(NamedFn),
    /// Same, but cannot fail.
    PlainNamed(fn(&GameData, &mut ParsedItem, &mut Scratch)),
}
use Parser::{Named, Plain, PlainNamed, PlainVirtual, Repeat, Section, Virtual};

const PARSERS: &[Parser] = &[
    Plain(parse_unidentified),
    PlainNamed(parse_superior),
    PlainNamed(parse_foulborn),
    PlainNamed(parse_vestigial),
    Plain(parse_synthesised),
    Named(strip_synthesised_name),
    Plain(parse_category_by_help_text),
    PlainNamed(parse_map_tier),
    PlainNamed(normalize_name),
    Named(find_in_database),
    // -----------
    Plain(parse_item_level),
    Section(parse_talisman_tier),
    Plain(parse_gem),
    Plain(parse_vaal_gem),
    Plain(parse_armour),
    Plain(parse_weapon),
    Section(parse_accessory),
    Plain(parse_flask),
    Plain(parse_tincture),
    Section(parse_stack_size),
    Plain(parse_corrupted),
    Plain(parse_imbued_gem),
    Plain(parse_foil),
    Plain(parse_influence),
    Section(parse_map),
    Section(parse_sockets),
    Plain(parse_heist_contract),
    Plain(parse_heist_blueprint),
    Section(parse_chart),
    Plain(parse_area_level),
    Plain(parse_atzoatl_rooms),
    Section(parse_filled_coffin),
    Plain(parse_mirrored),
    Plain(parse_split),
    Plain(parse_sentinel_charge),
    Section(parse_scrying_orb),
    Section(parse_mercenary),
    Repeat(3, parse_logbook_area),
    Repeat(6, parse_mercenary_gems),
    // enchant, scourge, implicit, explicit
    Repeat(4, parse_modifiers),
    PlainVirtual(augment_modifiers),
    PlainVirtual(transform_to_legacy_modifiers),
    PlainVirtual(parse_fractured),
    Named(pick_correct_variant),
    PlainVirtual(calc_disenchant_dust),
    Virtual(calc_base_percentile),
];

const VALUE_AUGMENTED: &str = " (augmented)";

/// `parseClipboard`
pub fn parse_clipboard(data: &GameData, clipboard: &str) -> Result<ParsedItem> {
    parse_clipboard_inner(data, clipboard)
}

fn parse_clipboard_inner(d: &GameData, clipboard: &str) -> Result<ParsedItem> {
    let mut texts = item_text_to_sections(clipboard);

    if texts.first().and_then(|s| s.get(2)) == Some(&d.cs.CANNOT_USE_ITEM.as_str()) {
        if let Some(first) = texts.first_mut() {
            first.pop();
        }
        let first = texts.remove(0);
        // prepend item class & rarity into the second section
        texts
            .first_mut()
            .ok_or_else(|| ParseError::parse("no second section"))?
            .splice(0..0, first);
    }
    if texts.is_empty() {
        return Err(ParseError::parse("empty item"));
    }
    let nameplate = texts.remove(0);
    let (mut item, mut scratch) = parse_name_plate(d, &nameplate)?;
    item.raw_text = clipboard.to_string();

    let mut sections: Vec<ItemSection> = texts
        .into_iter()
        .map(|lines| ItemSection { lines, claimed: false })
        .collect();

    // each section can be parsed at most by one parser
    for parser in PARSERS {
        match parser {
            Virtual(f) => f(d, &mut item)?,
            Named(f) => f(d, &mut item, &mut scratch)?,
            PlainVirtual(f) => f(d, &mut item),
            PlainNamed(f) => f(d, &mut item, &mut scratch),
            Plain(f) => offer_sections(d, &mut sections, &mut item, |d, s, i| Ok(f(d, s, i)))?,
            Section(f) => offer_sections(d, &mut sections, &mut item, f)?,
            Repeat(times, f) => {
                for _ in 0..*times {
                    offer_sections(d, &mut sections, &mut item, f)?;
                }
            }
        }
    }
    Ok(item)
}

/// Offers the unclaimed sections to `parse` in order until it claims one or opts out.
fn offer_sections(
    d: &GameData,
    sections: &mut [ItemSection],
    item: &mut ParsedItem,
    parse: impl Fn(&GameData, &[&str], &mut ParsedItem) -> Result<SectionResult>,
) -> Result<()> {
    for section in sections.iter_mut().filter(|s| !s.claimed) {
        // Workaround: eldritch influence lines are appended to whichever section comes last,
        // so strip them before any parser looks at the section.
        parse_eldritch_item(d, &mut section.lines);

        match parse(d, &section.lines, item)? {
            Parsed => {
                section.claimed = true;
                break;
            }
            ParserSkipped => break,
            Skipped => {}
        }
    }
    Ok(())
}

fn item_text_to_sections(text: &str) -> Vec<Vec<&str>> {
    let mut lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }

    let mut sections: Vec<Vec<&str>> = vec![vec![]];
    for line in lines {
        match sections.last_mut() {
            Some(last) if line != "--------" => last.push(line),
            _ => sections.push(vec![]),
        }
    }
    sections.into_iter().filter(|s| !s.is_empty()).collect()
}

/// `section[0].…`, which throws when the section is empty.
fn s0<'a>(section: &[&'a str]) -> Result<&'a str> {
    section
        .first()
        .copied()
        .ok_or_else(|| ParseError::parse("empty section"))
}

/// `section[0] === x`
fn first_is(section: &[&str], x: &str) -> bool {
    section.first() == Some(&x)
}

/// The rest of `line` after `prefix`, if it starts with it.
fn after<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.strip_prefix(prefix)
}

fn find_info<'a>(infos: Option<&Vec<&'a BaseType>>) -> Option<&'a BaseType> {
    infos.and_then(|v| v.first().copied())
}

#[expect(clippy::unwrap_used, reason = "constant pattern")]
static MARKUP_SET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<<set:.+?>>").unwrap());

#[expect(clippy::unwrap_used, reason = "constant pattern")]
static MARKUP_COND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<(if:.+?|elif:.+?|else)>\{(.+?)\}").unwrap());

fn markup_condition_parser(text: &str) -> String {
    // ignores state set by <<set:__>>
    // always evaluates first condition to true <if:__>{...}
    let text = MARKUP_SET.replace_all(text, "");
    MARKUP_COND
        .replace_all(&text, |c: &regex::Captures| {
            if c[1].starts_with("if:") {
                c[2].to_string()
            } else {
                String::new()
            }
        })
        .into_owned()
}

fn parse_name_plate(d: &GameData, section: &[&str]) -> Result<(ParsedItem, Scratch)> {
    let cs = &d.cs;
    let mut lines = section.iter().copied();

    let line = lines.next();
    if !line.is_some_and(|l| l.starts_with(cs.ITEM_CLASS.as_str())) {
        return Err(ParseError::parse("not an item"));
    }

    let mut line = lines.next();
    let mut rarity_text: Option<&str> = None;
    if let Some(rest) = line.and_then(|l| after(l, &cs.RARITY)) {
        rarity_text = Some(rest);
        line = lines.next();
    }

    let name = match line {
        Some(l) => markup_condition_parser(l),
        None => return Err(ParseError::parse("not an item")),
    };

    let base_type = lines.next().map(markup_condition_parser);

    let mut item = ParsedItem::default();
    let scratch = Scratch {
        name,
        base_type,
        info_variants: Vec::new(),
    };

    if let Some(rarity) = rarity_text {
        let r = rarity;
        let rarities = [
            (&cs.RARITY_NORMAL, ItemRarity::Normal),
            (&cs.RARITY_QUEST, ItemRarity::Normal),
            (&cs.RARITY_MAGIC, ItemRarity::Magic),
            (&cs.RARITY_RARE, ItemRarity::Rare),
            (&cs.RARITY_UNIQUE, ItemRarity::Unique),
        ];
        let categories = [
            (&cs.RARITY_CURRENCY, ItemCategory::Currency),
            (&cs.RARITY_DIVCARD, ItemCategory::DivinationCard),
            (&cs.RARITY_GEM, ItemCategory::Gem),
        ];
        if let Some((_, c)) = categories.iter().find(|(s, _)| s.as_str() == r) {
            item.category = Some((*c).into());
        } else if let Some((_, ra)) = rarities.iter().find(|(s, _)| s.as_str() == r) {
            item.rarity = Some(*ra);
        }
    }
    Ok((item, scratch))
}

fn normalize_name(d: &GameData, item: &mut ParsedItem, scratch: &mut Scratch) {
    let cs = &d.cs;
    if item.rarity == Some(ItemRarity::Magic) {
        if let Some(base_type) = magic_name::magic_basetype(&d.items, &scratch.name) {
            scratch.name = base_type;
        }
    }

    if is(&item.category, ItemCategory::MetamorphSample) {
        let name = scratch.name.clone();
        if cs.METAMORPH_BRAIN.is_match(&name) {
            scratch.name = "Metamorph Brain".into();
        } else if cs.METAMORPH_EYE.is_match(&name) {
            scratch.name = "Metamorph Eye".into();
        } else if cs.METAMORPH_LUNG.is_match(&name) {
            scratch.name = "Metamorph Lung".into();
        } else if cs.METAMORPH_HEART.is_match(&name) {
            scratch.name = "Metamorph Heart".into();
        } else if cs.METAMORPH_LIVER.is_match(&name) {
            scratch.name = "Metamorph Liver".into();
        }
    }
}

fn find_in_database(d: &GameData, item: &mut ParsedItem, scratch: &mut Scratch) -> Result<()> {
    let base_or_name = scratch.base_type.clone().unwrap_or_else(|| scratch.name.clone());
    let mut info: Option<Vec<&BaseType>> = if is(&item.category, ItemCategory::DivinationCard) {
        d.items.by_translated("DIVINATION_CARD", &scratch.name)
    } else if is(&item.category, ItemCategory::CapturedBeast) {
        d.items.by_translated("CAPTURED_BEAST", &base_or_name)
    } else if is(&item.category, ItemCategory::Gem) || is(&item.category, ItemCategory::MetamorphSample) {
        d.items.by_translated(
            if is(&item.category, ItemCategory::Gem) {
                "GEM"
            } else {
                "ITEM"
            },
            &scratch.name,
        )
    } else if is(&item.category, ItemCategory::Voidstone) {
        d.items.by_ref("ITEM", "Charged Compass")
    } else if item.rarity == Some(ItemRarity::Unique) && !item.status.is_unidentified {
        d.items.by_translated("UNIQUE", &scratch.name)
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
        _ => return Err(ParseError::Unknown),
    };
    if infos.first().is_some_and(|i| i.unique.is_some()) {
        let Some(base_name) = scratch.base_type.as_deref() else {
            return Err(ParseError::Unknown);
        };
        let base_types = d.items.by_translated("ITEM", base_name);
        let Some(base_ref) = find_info(base_types.as_ref()).map(|b| b.ref_name.clone()) else {
            return Err(ParseError::Unknown);
        };
        infos.retain(|i| i.unique.as_ref().is_some_and(|u| u.base == base_ref));
    }
    // only a check for "no result" before the unique filter above; with no
    // variants left it crashes on the missing `info`, which is a parse error
    if infos.is_empty() {
        return Err(ParseError::parse("no variants of the unique left"));
    }
    scratch.info_variants = infos.drain(..).cloned().collect();
    // choose 1st variant, correct one will be picked at the end of parsing
    item.info = scratch.info_variants.first().cloned().unwrap_or_default();
    // same for every variant
    if item.category.is_none() {
        if let Some(c) = &item.info.craftable {
            item.category = Some(c.category.clone());
            // Viscera have no "Rarity:" line, and without a rarity no modifier is read
            if item.rarity.is_none() && c.category == Category::Known(ItemCategory::Viscera) {
                item.rarity = Some(ItemRarity::Normal);
            }
        } else if let Some(u) = &item.info.unique {
            let base = d
                .items
                .by_ref("ITEM", &u.base)
                .ok_or_else(|| ParseError::parse("unique base missing"))?;
            let craftable = base
                .first()
                .and_then(|b| b.craftable.as_ref())
                .ok_or_else(|| ParseError::parse("base has no category"))?;
            item.category = Some(craftable.category.clone());
        }
    }
    Ok(())
}

fn parse_map_tier(d: &GameData, item: &mut ParsedItem, scratch: &mut Scratch) {
    let text = match scratch.base_type.as_deref() {
        Some(b) if !b.is_empty() => b.to_string(),
        _ => scratch.name.clone(),
    };
    let Some(m) = d.cs.MAP_TIER.captures(&text) else {
        return;
    };

    item.map.tier = js::int(&m[1]);
    let matched = m[0].to_string();

    match scratch.base_type.as_deref() {
        Some(b) if !b.is_empty() => scratch.base_type = Some(b.replacen(&matched, "", 1)),
        _ => scratch.name = scratch.name.replacen(&matched, "", 1),
    }
}

fn parse_area_prop_nested(d: &GameData, line: &str, item: &mut ParsedItem) -> bool {
    let cs = &d.cs;
    if let Some(v) = after(line, &cs.MAP_ITEM_QUANTITY) {
        item.map.item_quantity = js::parse_int(v);
        true
    } else if let Some(v) = after(line, &cs.MAP_ITEM_RARITY) {
        item.map.item_rarity = js::parse_int(v);
        true
    } else if let Some(v) = after(line, &cs.MAP_MONSTER_PACK_SIZE) {
        item.map.pack_size = js::parse_int(v);
        true
    } else {
        false
    }
}

fn parse_map(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
    let cs = &d.cs;
    if !is(&item.category, ItemCategory::Map) {
        return Ok(ParserSkipped);
    }

    let mut result = Skipped;

    for line in section {
        if parse_area_prop_nested(d, line, item) {
            result = Parsed;
        } else if let Some(v) = after(line, &cs.MAP_MORE_MAPS) {
            item.map.more_maps = js::parse_int(v);
            result = Parsed;
        } else if let Some(v) = after(line, &cs.MAP_MORE_CURRENCY) {
            item.map.more_currency = js::parse_int(v);
            result = Parsed;
        } else if let Some(v) = after(line, &cs.MAP_MORE_DIVINATION_CARDS) {
            item.map.more_div_cards = js::parse_int(v);
            result = Parsed;
        } else if line.starts_with(&cs.MAP_AREA) {
            // the area name is read from the section's first line, not the matching line
            let area_name = s0(section)?.get(cs.MAP_AREA.len()..).unwrap_or("");
            let area = d
                .items
                .by_translated("AREA", area_name)
                .ok_or_else(|| ParseError::parse("Unknown Area name."))?;
            item.map.area = area.into_iter().next().cloned();
            result = Parsed;
        } else if let Some(m) = cs.MAP_COMPLETION_REWARD.captures(line) {
            let reward = d
                .items
                .by_translated("UNIQUE", &m[1])
                .ok_or_else(|| ParseError::parse("Unknown Unique Item."))?;
            item.map.completion_reward = reward.into_iter().next().cloned();
            result = Parsed;
        }
    }

    Ok(result)
}

fn parse_fractured(_: &GameData, item: &mut ParsedItem) {
    if item.new_mods.iter().any(|m| m.info.type_ == ModifierType::Fractured) {
        item.mod_marks.is_fractured = true;
    }
}

fn pick_correct_variant(d: &GameData, item: &mut ParsedItem, scratch: &mut Scratch) -> Result<()> {
    let at = pick_variant_index(&scratch.info_variants, item)?.unwrap_or(0);
    item.info = scratch.info_variants.get(at).cloned().unwrap_or_default();
    if let Some(unique) = &item.info.unique {
        let base_variants = d
            .items
            .by_ref("ITEM", &unique.base)
            .ok_or_else(|| ParseError::parse("unique base missing"))?;
        let at = pick_variant_index(&base_variants, item)?.unwrap_or(0);
        item.unique_base = base_variants.get(at).copied().cloned();
    }
    Ok(())
}

/// Index of the variant whose conditions the item meets.
fn pick_variant_index<B: Borrow<BaseType>>(variants: &[B], item: &ParsedItem) -> Result<Option<usize>> {
    if variants.len() <= 1 {
        return Ok((!variants.is_empty()).then_some(0));
    }

    let truthy = |v: Option<i64>| v.is_some_and(|v| v != 0);

    'variant: for (at, variant) in variants.iter().enumerate() {
        let cond = variant
            .borrow()
            .disc
            .as_ref()
            .ok_or_else(|| ParseError::parse("variant without conditions"))?;

        if cond.prop_ar.is_some() && !truthy(item.armour.ar) {
            continue;
        }
        if cond.prop_ev.is_some() && !truthy(item.armour.ev) {
            continue;
        }
        if cond.prop_es.is_some() && !truthy(item.armour.es) {
            continue;
        }

        if let Some(tier) = &cond.map_tier {
            let Some(map_tier) = item.map.tier.filter(|t| *t != 0) else {
                continue;
            };
            match tier.as_str() {
                "W" if map_tier > 5 => continue,
                "Y" if !(6..=10).contains(&map_tier) => continue,
                "R" if map_tier < 11 => continue,
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

        return Ok(Some(at));
    }

    // it may happen that we don't find correct variant
    // i.e. corrupted implicit on Two-Stone Ring
    Ok(None)
}

fn parse_influence(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    let cs = &d.cs;
    if section.len() <= 2 {
        let count_before = item.influences.len();

        for line in section {
            let l = *line;
            let influences = [
                (&cs.INFLUENCE_CRUSADER, ItemInfluence::Crusader),
                (&cs.INFLUENCE_ELDER, ItemInfluence::Elder),
                (&cs.INFLUENCE_SHAPER, ItemInfluence::Shaper),
                (&cs.INFLUENCE_HUNTER, ItemInfluence::Hunter),
                (&cs.INFLUENCE_REDEEMER, ItemInfluence::Redeemer),
                (&cs.INFLUENCE_WARLORD, ItemInfluence::Warlord),
            ];
            if let Some((_, i)) = influences.iter().find(|(s, _)| l == s.as_str()) {
                item.influences.push(*i);
            }
        }

        if count_before < item.influences.len() {
            return Parsed;
        }
    }
    Skipped
}

fn parse_corrupted(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if first_is(section, &d.cs.CORRUPTED) {
        item.status.is_corrupted = true;
        return Parsed;
    } else if first_is(section, &d.cs.UNMODIFIABLE) {
        item.status.is_corrupted = true;
        item.status.is_unmodifiable = true;
        return Parsed;
    }
    Skipped
}

fn parse_foil(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if item.rarity != Some(ItemRarity::Unique) {
        return ParserSkipped;
    }
    if first_is(section, &d.cs.FOIL_UNIQUE) {
        item.looks.is_foil = true;
        return Parsed;
    }
    Skipped
}

fn parse_unidentified(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if first_is(section, &d.cs.UNIDENTIFIED) {
        item.status.is_unidentified = true;
        return Parsed;
    }
    Skipped
}

fn parse_item_level(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    let prefix = if item.info.ref_name == "Filled Coffin" {
        &d.cs.CORPSE_LEVEL
    } else {
        &d.cs.ITEM_LEVEL
    };

    for line in section {
        if let Some(v) = after(line, prefix) {
            item.item_level = js::int(v);
            return Parsed;
        }
    }
    Skipped
}

fn parse_talisman_tier(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
    if let Some(v) = after(s0(section)?, &d.cs.TALISMAN_TIER) {
        item.talisman_tier = js::int(v);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_vaal_gem(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if !is(&item.category, ItemCategory::Gem) {
        return ParserSkipped;
    }

    if let [only] = section {
        if let Some(gem_info) = d.items.by_translated("GEM", only) {
            item.gem.vaal = gem_info.into_iter().next().cloned();
            return Parsed;
        }
    }
    Skipped
}

fn parse_gem(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if !is(&item.category, ItemCategory::Gem) {
        return ParserSkipped;
    }
    if let Some(v) = section.get(1).and_then(|l| after(l, &d.cs.GEM_LEVEL)) {
        // "Level: 20 (Max)"
        item.gem.level = js::parse_int(v);

        parse_quality_nested(d, section, item);

        return Parsed;
    }
    Skipped
}

fn parse_imbued_gem(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if !is(&item.category, ItemCategory::Gem) {
        return ParserSkipped;
    }

    if let [only] = section {
        let Some((stat, matcher)) = d.stats.by_match_str(only) else {
            return Skipped;
        };

        item.new_mods.push(ParsedModifier {
            info: ModifierInfo::new(ModifierType::Imbued),
            stats: vec![ParsedStat {
                stat: Arc::new(stat.clone()),
                translation: Arc::new(matcher.clone()),
                roll: None,
            }],
        });
        item.gem.imbued = true;

        return Parsed;
    }
    Skipped
}

fn parse_stack_size(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
    if item.rarity != Some(ItemRarity::Normal)
        && !is(&item.category, ItemCategory::Currency)
        && !is(&item.category, ItemCategory::DivinationCard)
    {
        return Ok(ParserSkipped);
    }
    if let Some(v) = after(s0(section)?, &d.cs.STACK_SIZE) {
        // Portal Scroll "Stack Size: 2[localized separator]448/40"
        let digits: String = v.chars().filter(|c| c.is_ascii_digit() || *c == '/').collect();
        let mut parts = digits.split('/').map(js::int);
        item.stack_size = Some(StackSize {
            value: parts.next().flatten(),
            max: parts.next().flatten(),
        });

        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_sockets(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
    if let Some(v) = after(s0(section)?, &d.cs.SOCKETS) {
        let sockets = v.trim_end();

        let mut result = Sockets {
            white: i64::try_from(sockets.matches('W').count()).unwrap_or(i64::MAX),
            linked: None,
        };

        let pattern: String = sockets
            .chars()
            .map(|c| if c == ' ' || c == '-' { c } else { '#' })
            .collect();
        if pattern == "#-#-#-#-#-#" {
            result.linked = Some(6);
        } else if pattern == "# #-#-#-#-#" || pattern == "#-#-#-#-# #" || pattern == "#-#-#-#-#" {
            result.linked = Some(5);
        }
        item.sockets = Some(result);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_quality_nested(d: &GameData, section: &[&str], item: &mut ParsedItem) -> bool {
    for line in section {
        if let Some(v) = after(line, &d.cs.QUALITY) {
            // "Quality: +20% (augmented)"
            item.quality = js::parse_int(v);
            return true;
        }
    }
    false
}

fn parse_memory_strands_nested(d: &GameData, section: &[&str], item: &mut ParsedItem) -> bool {
    for line in section {
        if let Some(v) = after(line, &d.cs.MEMORY_STRANDS) {
            item.memory_strands = js::parse_int(v);
            return true;
        }
    }
    false
}

fn parse_armour(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    let cs = &d.cs;
    let mut result = Skipped;

    for line in section {
        if let Some(v) = after(line, &cs.ARMOUR) {
            item.armour.ar = js::parse_int(v);
        } else if let Some(v) = after(line, &cs.EVASION) {
            item.armour.ev = js::parse_int(v);
        } else if let Some(v) = after(line, &cs.ENERGY_SHIELD) {
            item.armour.es = js::parse_int(v);
        } else if let Some(v) = after(line, &cs.TAG_WARD) {
            item.armour.ward = js::parse_int(v);
        } else if let Some(v) = after(line, &cs.BLOCK_CHANCE) {
            item.armour.block = js::parse_int(v);
        } else {
            continue;
        }
        result = Parsed;
    }

    if result == Parsed {
        parse_quality_nested(d, section, item);
        parse_memory_strands_nested(d, section, item);
    }

    result
}

/// `str.split('-').map(parseInt)` averaged.
fn range_avg(text: &str) -> f64 {
    let values: Vec<f64> = text
        .split('-')
        .map(|s| js::parse_int(s).map_or(f64::NAN, |n| n as f64))
        .collect();
    roll_or_minmax_avg(&values)
}

fn parse_weapon(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    let cs = &d.cs;
    let mut result = Skipped;

    for line in section {
        if let Some(v) = after(line, &cs.CRIT_CHANCE) {
            item.weapon.crit = Some(js::parse_float(v));
        } else if let Some(v) = after(line, &cs.ATTACK_SPEED) {
            item.weapon.attack_speed = Some(js::parse_float(v));
        } else if let Some(v) = after(line, &cs.PHYSICAL_DAMAGE) {
            item.weapon.physical = Some(range_avg(v));
        } else if let Some(v) = after(line, &cs.ELEMENTAL_DAMAGE) {
            item.weapon.elemental = Some(v.split(", ").map(range_avg).fold(0.0, |sum, x| sum + x));
        } else {
            continue;
        }
        result = Parsed;
    }

    if result == Parsed {
        parse_quality_nested(d, section, item);
        parse_memory_strands_nested(d, section, item);
    }

    result
}

fn parse_accessory(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
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
                    item.quality = Some(found.roll.as_ref().ok_or_else(|| ParseError::parse("no roll"))?.value as i64);
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

fn pseudo_mod(stat: &crate::data::Stat, matcher: &crate::data::StatMatcher, type_: ModifierType) -> ParsedModifier {
    ParsedModifier {
        info: ModifierInfo::new(type_),
        stats: vec![ParsedStat {
            stat: Arc::new(stat.clone()),
            translation: Arc::new(matcher.clone()),
            roll: None,
        }],
    }
}

fn parse_logbook_area(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
    if item.info.ref_name != "Expedition Logbook" {
        return Ok(ParserSkipped);
    }
    if section.len() < 3 {
        return Ok(Skipped);
    }

    // skip Logbook Area line, parse Faction
    let Some((faction, faction_matcher)) = section.get(1).and_then(|l| d.stats.by_match_str(l)) else {
        return Ok(Skipped);
    };
    if !faction.ref_.starts_with("Has Logbook Faction:") {
        return Ok(Skipped);
    }

    let mut area_mods = vec![pseudo_mod(faction, faction_matcher, ModifierType::Pseudo)];

    let (mod_type, lines) = parse_mod_type(section.get(2..).unwrap_or_default())?;
    for line in &lines {
        // Area contains an Expedition Boss (#)
        if let Some((stat, matcher)) = d.stats.by_match_str(line) {
            if stat.better == 0 {
                area_mods.push(pseudo_mod(stat, matcher, mod_type));
            }
        }
    }

    item.heist
        .logbook_area_mods
        .get_or_insert_with(Vec::new)
        .push(area_mods);

    Ok(Parsed)
}

fn parse_mercenary(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
    let cs = &d.cs;
    if item.info.ref_name != "Mercenary Warrant" {
        return Ok(ParserSkipped);
    }

    for line in section {
        if let Some(v) = after(line, &cs.MERCENARY_LEVEL) {
            item.item_level = js::int(v);
        } else if let Some(v) = after(line, &cs.MERCENARY_BUILD) {
            let mut build = d
                .items
                .by_translated("MERCENARY_BUILD", v)
                .ok_or_else(|| ParseError::parse("Unknown Mercenary Build."))?;
            if let Some(ref_name) = build
                .first()
                .and_then(|b| b.mercenary_build.as_ref())
                .and_then(|m| m.as_str())
            {
                build = d
                    .items
                    .by_ref("MERCENARY_BUILD", ref_name)
                    .ok_or_else(|| ParseError::parse("missing build"))?;
            }
            item.gem.mercenary_build = build.into_iter().next().cloned();
        }
    }

    if item.gem.mercenary_build.is_some() {
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_mercenary_gems(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
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

    for line in section.iter().skip(1) {
        let support = try_parse_translation(
            d,
            &StatString {
                string: (*line).to_string(),
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
                    .ok_or_else(|| ParseError::parse("mercenary stat without mercenary info"))?;
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

    item.gem.mercenary_skills.get_or_insert_with(Vec::new).push(group);

    Ok(Parsed)
}

fn parse_modifiers(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
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
                item.mod_marks.is_veiled = true;
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

fn parse_mirrored(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if first_is(section, &d.cs.MIRRORED) {
        item.origin.is_mirrored = true;
        return Parsed;
    }
    Skipped
}

fn parse_split(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if matches!(section, [only] if *only == d.cs.SPLIT) {
        item.origin.is_split = true;
        return Parsed;
    }
    Skipped
}

fn parse_flask(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if !is(&item.category, ItemCategory::Flask) {
        return ParserSkipped;
    }

    // the purpose of this parser is to "consume" flask buffs
    // so they are not recognized as modifiers
    if section.iter().any(|l| d.cs.FLASK_CHARGES.is_match(l)) {
        parse_quality_nested(d, section, item);
        return Parsed;
    }
    Skipped
}

fn parse_tincture(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if !is(&item.category, ItemCategory::Tincture) {
        return ParserSkipped;
    }

    if parse_quality_nested(d, section, item) {
        return Parsed;
    }
    Skipped
}

fn parse_sentinel_charge(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if !is(&item.category, ItemCategory::Sentinel) {
        return ParserSkipped;
    }

    if let [only] = section {
        if let Some(v) = after(only, &d.cs.SENTINEL_CHARGE) {
            item.sentinel_charge = js::parse_int(v);
            return Parsed;
        }
    }
    Skipped
}

fn parse_scrying_orb(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
    if item.info.ref_name != "Scrying Orb" {
        return Ok(ParserSkipped);
    }

    if let [only] = section {
        if let Some(area_name) = after(only, &d.cs.MAP_AREA) {
            let area = d
                .items
                .by_translated("AREA", area_name)
                .ok_or_else(|| ParseError::parse("Unknown Area name."))?;
            item.map.area = area.into_iter().next().cloned();
            return Ok(Parsed);
        }
    }
    Ok(Skipped)
}

fn parse_eldritch_item(d: &GameData, section: &mut Vec<&str>) {
    // these don't have a dedicated section and are appended to last-ish one,
    // it can be Explicit Mods section, Corrupted, or Mirrored line.
    while let Some(last) = section.last() {
        if *last == d.cs.ITEM_EATER.as_str() || *last == d.cs.ITEM_EXARCH.as_str() {
            section.pop();
        } else {
            break;
        }
    }
}

fn parse_synthesised(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if matches!(section, [only] if *only == d.cs.SECTION_SYNTHESISED) {
        item.origin.is_synthesised = true;
        return Parsed;
    }

    Skipped
}

/// The name of a synthesised item carries a prefix that the data does not have.
fn strip_synthesised_name(d: &GameData, item: &mut ParsedItem, scratch: &mut Scratch) -> Result<()> {
    if !item.origin.is_synthesised {
        return Ok(());
    }
    let strip = |text: &str| -> Result<String> {
        let m =
            d.cs.ITEM_SYNTHESISED
                .captures(text)
                .ok_or_else(|| ParseError::parse("not a synthesised name"))?;
        Ok(m[1].to_string())
    };
    match scratch.base_type.as_deref() {
        Some(b) if !b.is_empty() => scratch.base_type = Some(strip(b)?),
        _ => scratch.name = strip(&scratch.name)?,
    }
    Ok(())
}

fn parse_superior(d: &GameData, item: &mut ParsedItem, scratch: &mut Scratch) {
    let applies = match item.rarity {
        Some(ItemRarity::Normal) => true,
        Some(ItemRarity::Magic | ItemRarity::Rare | ItemRarity::Unique) => item.status.is_unidentified,
        None => false,
    };
    if applies {
        if let Some(m) = d.cs.ITEM_SUPERIOR.captures(&scratch.name) {
            scratch.name = m[1].to_string();
        }
    }
}

fn parse_foulborn(d: &GameData, item: &mut ParsedItem, scratch: &mut Scratch) {
    if item.rarity != Some(ItemRarity::Unique) || item.status.is_unidentified {
        return;
    }

    if let Some(m) = d.cs.FOULBORN_NAME.captures(&scratch.name) {
        scratch.name = m[1].to_string();
        item.mod_marks.is_foulborn = true;
    }
}

fn parse_vestigial(d: &GameData, item: &mut ParsedItem, scratch: &mut Scratch) {
    let Some(base_type) = scratch.base_type.as_deref().filter(|b| !b.is_empty()) else {
        return;
    };
    if item.rarity != Some(ItemRarity::Unique) {
        return;
    }

    if let Some(m) = d.cs.VESTIGIAL_NAME.captures(base_type) {
        scratch.base_type = Some(m[1].to_string());
        item.looks.is_vestigial = true;
    }
}

fn parse_category_by_help_text(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    if first_is(section, &d.cs.BEAST_HELP) {
        item.category = Some(ItemCategory::CapturedBeast.into());
        return Parsed;
    } else if first_is(section, &d.cs.METAMORPH_HELP) {
        item.category = Some(ItemCategory::MetamorphSample.into());
        return Parsed;
    } else if first_is(section, &d.cs.VOIDSTONE_HELP) {
        item.category = Some(ItemCategory::Voidstone.into());
        return Parsed;
    }

    Skipped
}

fn parse_heist_contract(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    let cs = &d.cs;
    if !is(&item.category, ItemCategory::HeistContract) {
        return ParserSkipped;
    }

    if !parse_area_level_nested(d, section, item) {
        return Skipped;
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
            contract.job_level = js::int(&m["level"]);
            continue;
        }

        if let Some(m) = cs.HEIST_CONTRACT_TARGET.captures(line) {
            if m[1] == cs.HEIST_TARGET_PRICELESS {
                contract.target_value = Some("Priceless");
            }
        }
    }

    item.heist.contract = Some(contract);
    Parsed
}

fn parse_heist_blueprint(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    let cs = &d.cs;
    if !is(&item.category, ItemCategory::HeistBlueprint) {
        return ParserSkipped;
    }

    if !parse_area_level_nested(d, section, item) {
        return Skipped;
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
            blueprint.wings_revealed = parts.next().and_then(js::parse_int);
            blueprint.wings_total = parts.next().and_then(js::parse_int);
        }
    }

    item.heist.blueprint = Some(blueprint);
    Parsed
}

fn parse_chart(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
    if !is(&item.category, ItemCategory::Chart) {
        return Ok(ParserSkipped);
    }

    if !parse_area_level_nested(d, section, item) {
        return Ok(Skipped);
    }

    let area = d
        .items
        .by_translated("AREA", s0(section)?)
        .ok_or_else(|| ParseError::parse("Unknown Area name."))?;
    item.map.area = area.into_iter().next().cloned();

    for line in section {
        if parse_area_prop_nested(d, line, item) {
            // line parsed
        } else if let Some(v) = after(line, &d.cs.CHART_SULPHUR) {
            item.map.chart_sulphur = js::parse_int(v);
        }
    }

    Ok(Parsed)
}

fn parse_area_level_nested(d: &GameData, section: &[&str], item: &mut ParsedItem) -> bool {
    for line in section {
        if let Some(v) = after(line, &d.cs.AREA_LEVEL) {
            item.map.area_level = js::int(v);
            return true;
        }
    }
    false
}

fn parse_area_level(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    let r = item.info.ref_name.as_str();
    if r != "Chronicle of Atzoatl" && r != "Expedition Logbook" && r != "Forbidden Tome" {
        return ParserSkipped;
    }

    if !parse_area_level_nested(d, section, item) {
        return Skipped;
    }

    Parsed
}

fn parse_atzoatl_rooms(d: &GameData, section: &[&str], item: &mut ParsedItem) -> SectionResult {
    let cs = &d.cs;
    if item.info.ref_name != "Chronicle of Atzoatl" {
        return ParserSkipped;
    }
    if !first_is(section, &cs.INCURSION_OPEN) {
        return Skipped;
    }

    let mut state = INCURSION_OPEN;
    for line in section.iter().skip(1) {
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
                    stat: Arc::new(stat.clone()),
                    translation: Arc::new(crate::data::StatMatcher {
                        string,
                        ..Default::default()
                    }),
                    roll: Some(Roll {
                        unscalable: true,
                        generation: None,
                        dp: false,
                        value: state as f64,
                        min: state as f64,
                        max: state as f64,
                    }),
                }],
            });
        } else {
            item.unknown_modifiers.push(UnknownModifier {
                text: line.to_string(),
                type_: ModifierType::Pseudo,
            });
        }
    }

    Parsed
}

fn parse_filled_coffin(d: &GameData, section: &[&str], item: &mut ParsedItem) -> Result<SectionResult> {
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
        match name.as_deref().and_then(|n| d.stats.by_match_str(n)) {
            Some((stat, matcher)) => modifier.stats.push(ParsedStat {
                stat: Arc::new(stat.clone()),
                translation: Arc::new(matcher.clone()),
                roll: None,
            }),
            None => item.unknown_modifiers.push(UnknownModifier {
                text: name.unwrap_or_default(),
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

fn augment_modifiers(_: &GameData, item: &mut ParsedItem) {
    let rarity = item.rarity;
    let corrupted = item.status.is_corrupted;
    let mirrored = item.origin.is_mirrored;
    let ring_or_amulet = is(&item.category, ItemCategory::Ring) || is(&item.category, ItemCategory::Amulet);
    let synthesised = item.origin.is_synthesised;

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
}

/// Deprecated, but the filters and price lookups still read `stats_by_type`.
fn transform_to_legacy_modifiers(_: &GameData, item: &mut ParsedItem) {
    item.stats_by_type = sum_stats_by_mod_type(&item.new_mods);
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
        (item.armour.ar, info.ar, &calc_q20::ARMOUR),
        (item.armour.ev, info.ev, &calc_q20::EVASION),
        (item.armour.es, info.es, &calc_q20::ENERGY_SHIELD),
        (item.armour.ward, info.ward, &calc_q20::WARD),
    ];
    let hit = candidates
        .iter()
        .find_map(|&(total, bounds, stats)| match (total, bounds) {
            (Some(total), Some(bounds)) if total != 0 => Some((total as f64, bounds, stats)),
            _ => None,
        });
    if let Some((total, bounds, stats)) = hit {
        item.base_percentile = Some(calc_q20::calc_prop_percentile(total, bounds, stats, item)?);
    }
    Ok(())
}

fn calc_disenchant_dust(_: &GameData, item: &mut ParsedItem) {
    let Some(disenchant_value) = item
        .info
        .unique
        .as_ref()
        .and_then(|u| u.disenchant_value)
        .filter(|v| *v != 0.0 && !v.is_nan())
    else {
        return;
    };

    let mut increase_by_factors = 0.0;

    // +50% per Influence Type
    increase_by_factors = (item.influences.len() as f64).mul_add(50.0, increase_by_factors);

    // +2% per 1% Item Quality
    if let Some(quality) = item.quality {
        increase_by_factors = (quality as f64).mul_add(2.0, increase_by_factors);
    }

    // +50% per Corruption Implicit
    if item.status.is_corrupted {
        for m in &item.new_mods {
            if m.info.mechanic == Some(ModifierMechanic::Corruption) {
                increase_by_factors += 50.0;
            }
        }
    }

    let factors_multi = (increase_by_factors + 100.0) / 100.0;
    let Some(ilvl) = item.item_level else {
        return;
    };
    let clamp = |lo: i64, hi: i64| ilvl.clamp(lo, hi) as f64;
    let term1 = 50.0; // ilvl 46 and below
    let term2 = 2.0 * (clamp(46, 68) - 46.0); // ilvl 47 to 68
    let term3 = (3.0 * (clamp(46, 68) - 46.0) / 11.0).floor(); // ilvl 47 to 68
    let term4 = 25.0 * (clamp(68, 84) - 68.0); // ilvl 69 to 84
    let total_multi = 5.0 * (term1 + term2 + term3 + term4) * factors_multi;

    item.dust_equivalent = Some((disenchant_value * total_multi).floor() as i64);
}
