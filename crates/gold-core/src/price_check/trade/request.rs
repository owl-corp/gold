//! The trade site search request.
//!
//! Not ported yet: "not"/"one of" filter groups.
use super::category_to_trade_id;
use crate::data::{GameData, StatOrGroup};
use crate::price_check::filters::{ItemFilters, StatFilter, INTERNAL_TRADE_IDS};
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::hash::{Hash, Hasher};

/// A JSON number the way JavaScript would print it: integers without a fraction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Num(pub f64);

impl Serialize for Num {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let x = self.0;
        if x.is_finite() && crate::js::eq(x, x.trunc()) && x.abs() < 1e15 {
            s.serialize_i64(x as i64)
        } else {
            s.serialize_f64(x)
        }
    }
}

impl Eq for Num {}

impl Hash for Num {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

/// `{ "min": .., "max": .. }`, either side optional. `option` is only used by stat entries.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct Range {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<Num>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Num>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<Num>,
}

impl Range {
    fn new(min: Option<f64>, max: Option<f64>) -> Self {
        Self {
            min: min.map(Num),
            max: max.map(Num),
            option: None,
        }
    }
}

/// `{ "option": .. }`
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
pub struct Opt {
    pub option: String,
}

impl Opt {
    fn new(option: impl Into<String>) -> Self {
        Self { option: option.into() }
    }

    /// The trade site's yes/no options are the strings "true" and "false".
    fn flag(value: bool) -> Self {
        Self::new(value.to_string())
    }
}

/// `{ "filters": .. }`, the wrapper every filter group has.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct Group<T> {
    pub filters: T,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct TradeFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collapse: Option<Opt>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct TypeFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Opt>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct MiscFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrupted: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fractured_item: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem_imbued: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foulborn_item: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vestigial: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirrored: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identified: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem_level: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ilvl: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_size: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_level: Option<Range>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct SocketColours {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<Num>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct SocketFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<SocketColours>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct MapFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_tier: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_level: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_iiq: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_iir: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_packsize: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_sulphur: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_blighted: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_uberblighted: Option<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_completion_reward: Option<Opt>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct ArmourFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_defence_percentile: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ar: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ev: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub es: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ward: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<Range>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct WeaponFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dps: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdps: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edps: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crit: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aps: Option<Range>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct HeistFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_agility: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_brute_force: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_counter_thaumaturgy: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_deception: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_demolition: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_engineering: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_lockpicking: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_perception: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_trap_disarmament: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_wings: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_max_wings: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_objective_value: Option<Opt>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct SentinelFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentinel_durability: Option<Range>,
}

/// The `query.filters` object: only the groups that were touched are present.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, Hash)]
pub struct QueryFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_filters: Option<Group<TradeFilters>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_filters: Option<Group<TypeFilters>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub misc_filters: Option<Group<MiscFilters>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_filters: Option<Group<SocketFilters>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_filters: Option<Group<MapFilters>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub armour_filters: Option<Group<ArmourFilters>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon_filters: Option<Group<WeaponFilters>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_filters: Option<Group<HeistFilters>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentinel_filters: Option<Group<SentinelFilters>>,
}

impl QueryFilters {
    fn trade(&mut self) -> &mut TradeFilters {
        &mut self.trade_filters.get_or_insert_default().filters
    }
    fn types(&mut self) -> &mut TypeFilters {
        &mut self.type_filters.get_or_insert_default().filters
    }
    fn misc(&mut self) -> &mut MiscFilters {
        &mut self.misc_filters.get_or_insert_default().filters
    }
    fn socket(&mut self) -> &mut SocketFilters {
        &mut self.socket_filters.get_or_insert_default().filters
    }
    fn map(&mut self) -> &mut MapFilters {
        &mut self.map_filters.get_or_insert_default().filters
    }
    fn armour(&mut self) -> &mut ArmourFilters {
        &mut self.armour_filters.get_or_insert_default().filters
    }
    fn weapon(&mut self) -> &mut WeaponFilters {
        &mut self.weapon_filters.get_or_insert_default().filters
    }
    fn heist(&mut self) -> &mut HeistFilters {
        &mut self.heist_filters.get_or_insert_default().filters
    }
    fn sentinel(&mut self) -> &mut SentinelFilters {
        &mut self.sentinel_filters.get_or_insert_default().filters
    }

    /// The range a property-style trade id (armour, dps, ...) writes into, and the default for
    /// a missing minimum. The range exists (possibly empty) even when nothing is written to it.
    fn internal_range(&mut self, id: &str) -> Option<(&mut Range, Option<f64>)> {
        let range = match id {
            "item.base_percentile" => self.armour().base_defence_percentile.get_or_insert_default(),
            "item.memory_strands" => self.misc().memory_level.get_or_insert_default(),
            "item.armour" => self.armour().ar.get_or_insert_default(),
            "item.evasion_rating" => self.armour().ev.get_or_insert_default(),
            "item.energy_shield" => self.armour().es.get_or_insert_default(),
            "item.ward" => self.armour().ward.get_or_insert_default(),
            "item.block" => self.armour().block.get_or_insert_default(),
            "item.total_dps" => self.weapon().dps.get_or_insert_default(),
            "item.physical_dps" => self.weapon().pdps.get_or_insert_default(),
            "item.elemental_dps" => self.weapon().edps.get_or_insert_default(),
            "item.crit" => self.weapon().crit.get_or_insert_default(),
            "item.aps" => self.weapon().aps.get_or_insert_default(),
            "item.map_item_quantity" => self.map().map_iiq.get_or_insert_default(),
            "item.map_item_rarity" => self.map().map_iir.get_or_insert_default(),
            "item.map_pack_size" => self.map().map_packsize.get_or_insert_default(),
            "item.map.chart_sulphur" => self.map().chart_sulphur.get_or_insert_default(),
            "item.heist_wings_revealed" => self.heist().heist_wings.get_or_insert_default(),
            "item.heist_wings_total" => self.heist().heist_max_wings.get_or_insert_default(),
            "item.heist_job_agility" => return Some((self.heist().heist_agility.get_or_insert_default(), Some(1.0))),
            "item.heist_job_bruteforce" => {
                return Some((self.heist().heist_brute_force.get_or_insert_default(), Some(1.0)))
            }
            "item.heist_job_counterthaumaturgy" => {
                return Some((
                    self.heist().heist_counter_thaumaturgy.get_or_insert_default(),
                    Some(1.0),
                ))
            }
            "item.heist_job_deception" => {
                return Some((self.heist().heist_deception.get_or_insert_default(), Some(1.0)))
            }
            "item.heist_job_demolition" => {
                return Some((self.heist().heist_demolition.get_or_insert_default(), Some(1.0)))
            }
            "item.heist_job_engineering" => {
                return Some((self.heist().heist_engineering.get_or_insert_default(), Some(1.0)))
            }
            "item.heist_job_lockpicking" => {
                return Some((self.heist().heist_lockpicking.get_or_insert_default(), Some(1.0)))
            }
            "item.heist_job_perception" => {
                return Some((self.heist().heist_perception.get_or_insert_default(), Some(1.0)))
            }
            "item.heist_job_trapdisarmament" => {
                return Some((self.heist().heist_trap_disarmament.get_or_insert_default(), Some(1.0)))
            }
            _ => return None,
        };
        Some((range, None))
    }
}

/// A name or base type: plain, or with the discriminator of a variant.
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum NameQuery {
    Plain(String),
    Variant { discriminator: String, option: String },
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
pub struct Status {
    pub option: &'static str,
}

/// One stat entry of a group.
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
pub struct StatQuery {
    pub id: String,
    pub value: Range,
    pub disabled: bool,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum GroupKind {
    And,
    Count,
    Not,
}

/// A stat group: "and" (all), "count" (at least `value` of them) or "not" (none).
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
pub struct StatGroup {
    #[serde(rename = "type")]
    pub kind: GroupKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    pub filters: Vec<StatQuery>,
}

impl StatGroup {
    const fn and(filters: Vec<StatQuery>) -> Self {
        Self {
            kind: GroupKind::And,
            value: None,
            disabled: None,
            filters,
        }
    }

    const fn count(value: Range, disabled: bool, filters: Vec<StatQuery>) -> Self {
        Self {
            kind: GroupKind::Count,
            value: Some(value),
            disabled: Some(disabled),
            filters,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
pub struct Query {
    pub status: Status,
    pub stats: Vec<StatGroup>,
    pub filters: QueryFilters,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<NameQuery>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub base_type: Option<NameQuery>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
pub struct Sort {
    pub price: &'static str,
}

/// The body sent to `api/trade/search/<league>`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
pub struct TradeRequest {
    pub query: Query,
    pub sort: Sort,
}

fn name_to_query(name: &str, discriminator: Option<&str>) -> NameQuery {
    match discriminator {
        Some(d) if !d.is_empty() => NameQuery::Variant {
            discriminator: d.to_string(),
            option: name.to_string(),
        },
        _ => NameQuery::Plain(name.to_string()),
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

/// One entry of a stat group, from a trade id and the filter's numbers.
fn trade_id_to_query(id: &str, stat: &StatFilter) -> StatQuery {
    let mut roll = stat.roll.as_ref();

    let div: f64 = if id.starts_with("{div_by_100}") { 100.0 } else { 1.0 };
    if id.starts_with("{empty}")
        || (id.starts_with("{empty_if_100}") && roll.is_some_and(|r| crate::js::eq(r.value, 100.0)))
    {
        roll = None;
    }
    let id = if id.starts_with('{') {
        id.split_once('}').map_or(id, |(_, rest)| rest)
    } else {
        id
    };

    let (min, max) = min_max(roll, div);
    let mut value = Range::new(min, max);
    value.option = stat.option.as_ref().map(|o| Num(o.value));
    StatQuery {
        id: id.to_string(),
        value,
        disabled: stat.disabled,
    }
}

fn every_trade_id_to_query(stat: &StatFilter) -> Vec<StatQuery> {
    stat.trade_id.iter().map(|id| trade_id_to_query(id, stat)).collect()
}

/// A stat with several trade ids matches if any one of them does.
fn append_and_filter(stat: &StatFilter, and_group: &mut Vec<StatQuery>, all_groups: &mut Vec<StatGroup>) {
    if let [id] = stat.trade_id.as_slice() {
        and_group.push(trade_id_to_query(id, stat));
    } else {
        all_groups.push(StatGroup::count(
            Range::new(Some(1.0), None),
            stat.disabled,
            every_trade_id_to_query(stat),
        ));
    }
}

/// An entry that must be present exactly once.
fn exactly_one(id: &str, disabled: bool) -> StatQuery {
    StatQuery {
        id: id.to_string(),
        value: Range::new(Some(1.0), Some(1.0)),
        disabled,
    }
}

/// The item-level filters (flags, ranges, map and socket options) of the query.
fn apply_item_filters(
    q_filters: &mut QueryFilters,
    filters: &ItemFilters,
    active: &crate::price_check::filters::Search,
) -> Result<()> {
    if filters.foil.as_ref().is_some_and(|f| !f.disabled) {
        q_filters.types().rarity = Some(Opt::new("uniquefoil"));
    } else if let Some(r) = &filters.rarity {
        q_filters.types().rarity = Some(Opt::new(if r.disabled { "nonunique" } else { r.value }));
    }

    if let Some(category) = &active.category {
        let id = category
            .known()
            .and_then(category_to_trade_id)
            .ok_or_else(|| anyhow!("Invalid category: {category:?}"))?;
        q_filters.types().category = Some(Opt::new(id));
    }

    if let Some(c) = &filters.corrupted {
        if !c.value || c.exact == Some(true) {
            q_filters.misc().corrupted = Some(Opt::flag(c.value));
        }
    }
    if filters.fractured.as_ref().is_some_and(|f| !f.value) {
        q_filters.misc().fractured_item = Some(Opt::flag(false));
    }
    if filters.imbued_gem.as_ref().is_some_and(|f| f.disabled) {
        q_filters.misc().gem_imbued = Some(Opt::flag(false));
    }
    if filters.split.as_ref().is_some_and(|f| f.disabled) {
        q_filters.misc().split = Some(Opt::flag(false));
    }
    if filters.foulborn.as_ref().is_some_and(|f| !f.value) {
        q_filters.misc().foulborn_item = Some(Opt::flag(false));
    }
    if filters.vestigial.as_ref().is_some_and(|f| !f.value) {
        q_filters.misc().vestigial = Some(Opt::flag(false));
    }
    if filters.mirrored.as_ref().is_some_and(|f| f.disabled) {
        q_filters.misc().mirrored = Some(Opt::flag(false));
    }

    if let Some(f) = filters.gem_level.as_ref().filter(|f| !f.disabled) {
        q_filters.misc().gem_level = Some(Range::new(Some(f.value as f64), None));
    }
    if let Some(f) = filters.quality.as_ref().filter(|f| !f.disabled) {
        q_filters.misc().quality = Some(Range::new(Some(f.value as f64), None));
    }
    if let Some(f) = filters.item_level.as_ref().filter(|f| !f.disabled) {
        q_filters.misc().ilvl = Some(Range::new(
            Some(f.value as f64),
            f.max.filter(|m| *m != 0).map(|m| m as f64),
        ));
    }
    if let Some(f) = filters.stack_size.as_ref().filter(|f| !f.disabled) {
        q_filters.misc().stack_size = Some(Range::new(Some(f.value as f64), None));
    }
    if let Some(f) = filters.linked_sockets.as_ref().filter(|f| !f.disabled) {
        q_filters.socket().links = Some(Range::new(Some(f.value as f64), None));
    }
    if let Some(f) = filters.white_sockets.as_ref().filter(|f| !f.disabled) {
        q_filters.socket().sockets = Some(SocketColours {
            w: Some(Num(f.value as f64)),
        });
    }
    if let Some(f) = filters.map_tier.as_ref().filter(|f| !f.disabled) {
        q_filters.map().map_tier = Some(Range::new(Some(f.value as f64), Some(f.value as f64)));
    }

    if let Some(b) = &filters.map_blighted {
        match b.value.as_str() {
            Some("Blighted") => q_filters.map().map_blighted = Some(Opt::flag(true)),
            Some("Blight-ravaged") => q_filters.map().map_uberblighted = Some(Opt::flag(true)),
            _ if b.value == serde_json::Value::Bool(false) => {
                q_filters.map().map_blighted = Some(Opt::flag(false));
                q_filters.map().map_uberblighted = Some(Opt::flag(false));
            }
            _ => {}
        }
    }
    if let Some(r) = &filters.map_completion_reward {
        q_filters.map().map_completion_reward = Some(Opt::new(r.name_trade.as_str()));
    }
    if filters.unidentified.as_ref().is_some_and(|f| !f.disabled) {
        q_filters.misc().identified = Some(Opt::flag(false));
    }
    if let Some(f) = filters.area_level.as_ref().filter(|f| !f.disabled) {
        q_filters.map().area_level = Some(Range::new(
            Some(f.value as f64),
            f.max.filter(|m| *m != 0).map(|m| m as f64),
        ));
    }
    if let Some(f) = filters.sentinel_charge.as_ref().filter(|f| !f.disabled) {
        q_filters.sentinel().sentinel_durability = Some(Range::new(Some(f.value as f64), None));
    }
    Ok(())
}

/// Property filters (`item.*` ids) and the empty-modifier count groups.
fn apply_property_stats(
    data: &GameData,
    q_filters: &mut QueryFilters,
    stats: &[StatFilter],
    extra_groups: &mut Vec<StatGroup>,
) -> Result<()> {
    for stat in stats {
        let first = stat.trade_id.first().map_or("", String::as_str);
        if !first.starts_with("item.") {
            continue;
        }

        if first == "item.has_empty_modifier" {
            let which = stat.option.as_ref().map_or(0, |o| o.value as usize).min(2);
            let pick = |names: [&'static str; 3]| names.get(which).copied().unwrap_or_default();
            let crafted = pseudo_id(
                data,
                pick([
                    "# Crafted Modifiers",
                    "# Crafted Prefix Modifiers",
                    "# Crafted Suffix Modifiers",
                ]),
            )?;
            let empty = pseudo_id(
                data,
                pick([
                    "# Empty Modifiers",
                    "# Empty Prefix Modifiers",
                    "# Empty Suffix Modifiers",
                ]),
            )?;
            let total = pseudo_id(data, "# Modifiers")?;
            let at_least = |id: &str, min: f64| StatQuery {
                id: id.to_string(),
                value: Range::new(Some(min), None),
                disabled: stat.disabled,
            };

            for (other, min) in [(&crafted, 1.0), (&total, 6.0)] {
                extra_groups.push(StatGroup::count(
                    Range::new(Some(1.0), Some(1.0)),
                    stat.disabled,
                    vec![exactly_one(&empty, stat.disabled), at_least(other, min)],
                ));
            }
        }

        if stat.disabled {
            continue;
        }

        if first == "item.heist_target_priceless" {
            q_filters.heist().heist_objective_value = Some(Opt::new("priceless"));
        } else if let Some((range, default_min)) = q_filters.internal_range(first) {
            let roll = stat
                .roll
                .as_ref()
                .ok_or_else(|| anyhow!("property filter without a roll"))?;
            range.min = roll.min.or(default_min).map(Num);
            range.max = roll.max.map(Num);
        }
    }
    Ok(())
}

/// Stats that are real modifiers on the trade site, with the veiled and influence ones.
fn real_stats(data: &GameData, filters: &ItemFilters, stats: &[StatFilter]) -> Result<Vec<StatFilter>> {
    let mut real: Vec<StatFilter> = stats
        .iter()
        .filter(|s| !INTERNAL_TRADE_IDS.contains(&s.trade_id.first().map_or("", String::as_str)))
        .cloned()
        .collect();

    if let Some(veiled) = &filters.veiled {
        for stat_ref in &veiled.stat_refs {
            let found = data
                .stats
                .by_ref(stat_ref)
                .ok_or_else(|| anyhow!("unknown veiled stat {stat_ref}"))?;
            let db_stats = match found {
                StatOrGroup::Group(g) => g.stats.as_slice(),
                StatOrGroup::Stat(s) => std::slice::from_ref(&**s),
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
    Ok(real)
}

pub fn create_trade_request(data: &GameData, filters: &ItemFilters, stats: &[StatFilter]) -> Result<TradeRequest> {
    let trade = &filters.trade;
    let mut q_filters = QueryFilters::default();
    // the count groups of the "empty modifier" filters, after the main "and" group
    let mut extra_groups: Vec<StatGroup> = Vec::new();

    if let Some(currency) = &trade.currency {
        q_filters.trade().price = Some(Opt::new(currency.as_str()));
    }
    if trade.collapse_listings == "api" && trade.collapse_merchant {
        q_filters.trade().collapse = Some(Opt::flag(true));
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
    let name = active
        .name_trade
        .as_deref()
        .or(active.name.as_deref())
        .map(|name| name_to_query(name, disc));
    let base_type = active
        .base_type_trade
        .as_deref()
        .or(active.base_type.as_deref())
        .map(|base| name_to_query(base, disc));

    apply_item_filters(&mut q_filters, filters, active)?;

    apply_property_stats(data, &mut q_filters, stats, &mut extra_groups)?;

    let real = real_stats(data, filters, stats)?;

    let mut and_filters: Vec<StatQuery> = Vec::new();
    let mut not_filters: Vec<StatQuery> = Vec::new();
    let mut groups: Vec<StatGroup> = Vec::new();

    for stat in &real {
        if stat.not == Some(true) {
            not_filters.extend(every_trade_id_to_query(stat));
        } else {
            append_and_filter(stat, &mut and_filters, &mut groups);
        }
    }

    let mut stat_groups = vec![StatGroup::and(and_filters)];
    stat_groups.extend(extra_groups);
    stat_groups.extend(groups);
    if !not_filters.is_empty() {
        stat_groups.push(StatGroup {
            kind: GroupKind::Not,
            value: None,
            disabled: None,
            filters: not_filters,
        });
    }

    Ok(TradeRequest {
        query: Query {
            status: Status { option: "securable" },
            stats: stat_groups,
            filters: q_filters,
            name,
            base_type,
        },
        sort: Sort { price: "asc" },
    })
}

const fn bare_filter(disabled: bool, trade_id: Vec<String>) -> StatFilter {
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
