//! Game data (`items.ndjson`, `stats.ndjson`) and the lookups the parser uses.
//!
//! Records are found through hash maps built in memory; the first record of a duplicate key wins.
use crate::client_strings::ClientStrings;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StatMatcher {
    pub string: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advanced: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negate: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StatTrade {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<bool>,
    pub ids: BTreeMap<String, Vec<String>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StatMercenary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical: Option<String>,
    #[serde(default, rename = "syntheticFamily", skip_serializing_if = "Option::is_none")]
    pub synthetic_family: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `better`: -1 = negative roll is better, 1 = positive roll is better, 0 = not comparable.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Stat {
    #[serde(rename = "ref")]
    pub ref_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dp: Option<bool>,
    pub matchers: Vec<StatMatcher>,
    pub better: i32,
    #[serde(default, rename = "modFamily", skip_serializing_if = "Option::is_none")]
    pub mod_family: Option<Vec<String>>,
    #[serde(default, rename = "fromAreaMods", skip_serializing_if = "Option::is_none")]
    pub from_area_mods: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anointments: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mercenary: Option<StatMercenary>,
    #[serde(default, rename = "jewelleryQuality", skip_serializing_if = "Option::is_none")]
    pub jewellery_quality: Option<Value>,
    pub trade: StatTrade,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "strat", rename_all = "kebab-case")]
pub enum StatGroupResolver {
    Select { test: Vec<Option<String>> },
    TrivialMerge,
    PercentMerge { kind: Vec<String> },
    FlagMerge { kind: Vec<String> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatGroup {
    pub resolve: StatGroupResolver,
    pub stats: Vec<Stat>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StatOrGroup {
    Group(StatGroup),
    Stat(Box<Stat>),
}

impl StatOrGroup {
    fn stats(&self) -> &[Stat] {
        match self {
            Self::Group(g) => &g.stats,
            Self::Stat(s) => std::slice::from_ref(&**s),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RefOnly {
    #[serde(rename = "ref")]
    pub ref_: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Disc {
    #[serde(default, rename = "propAR", skip_serializing_if = "Option::is_none")]
    pub prop_ar: Option<bool>,
    #[serde(default, rename = "propEV", skip_serializing_if = "Option::is_none")]
    pub prop_ev: Option<bool>,
    #[serde(default, rename = "propES", skip_serializing_if = "Option::is_none")]
    pub prop_es: Option<bool>,
    #[serde(default, rename = "hasImplicit", skip_serializing_if = "Option::is_none")]
    pub has_implicit: Option<RefOnly>,
    #[serde(default, rename = "hasExplicit", skip_serializing_if = "Option::is_none")]
    pub has_explicit: Option<RefOnly>,
    #[serde(default, rename = "sectionText", skip_serializing_if = "Option::is_none")]
    pub section_text: Option<String>,
    #[serde(default, rename = "mapTier", skip_serializing_if = "Option::is_none")]
    pub map_tier: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Craftable {
    pub category: crate::parser::meta::Category,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrupted: Option<bool>,
    #[serde(default, rename = "uniqueOnly", skip_serializing_if = "Option::is_none")]
    pub unique_only: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UniqueInfo {
    pub base: String,
    #[serde(default, rename = "fixedStats", skip_serializing_if = "Option::is_none")]
    pub fixed_stats: Option<Vec<String>>,
    #[serde(default, rename = "disenchantValue", skip_serializing_if = "Option::is_none")]
    pub disenchant_value: Option<f64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ArmourInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ar: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ev: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub es: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ward: Option<[f64; 2]>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BaseType {
    pub name: String,
    #[serde(rename = "refName")]
    pub ref_name: String,
    pub namespace: String,
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<f64>,
    #[serde(default, rename = "tradeTag", skip_serializing_if = "Option::is_none")]
    pub trade_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchangeable: Option<bool>,
    #[serde(default, rename = "tradeDisc", skip_serializing_if = "Option::is_none")]
    pub trade_disc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disc: Option<Disc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub craftable: Option<Craftable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique: Option<UniqueInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gem: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armour: Option<ArmourInfo>,
    /// Either the `refName` of another build, or the build itself.
    #[serde(default, rename = "mercenaryBuild", skip_serializing_if = "Option::is_none")]
    pub mercenary_build: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Index of the first record per key: duplicates resolve to the earliest one.
type Index = HashMap<String, usize>;

fn insert_first(index: &mut Index, key: String, line: usize) {
    index.entry(key).or_insert(line);
}

pub struct ItemDb {
    items: Vec<BaseType>,
    by_name: Index,
    by_ref: Index,
}

impl ItemDb {
    pub fn parse(ndjson: &str) -> Result<Self> {
        let mut items = Vec::new();
        for (n, line) in ndjson.lines().enumerate() {
            items.push(serde_json::from_str::<BaseType>(line).with_context(|| format!("items.ndjson line {}", n + 1))?);
        }

        // only the first record of each `refName` is indexed; later ones are found by scanning on
        let mut seen = HashSet::new();
        let (mut by_name, mut by_ref) = (Index::new(), Index::new());
        for (n, item) in items.iter().enumerate() {
            if seen.insert((item.namespace.as_str(), item.ref_name.as_str())) {
                insert_first(&mut by_name, format!("{}::{}", item.namespace, item.name), n);
                insert_first(&mut by_ref, format!("{}::{}", item.namespace, item.ref_name), n);
            }
        }
        Ok(Self { items, by_name, by_ref })
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The record found in `index` and the ones right after it with the same key, up to the
    /// first that is neither a disc variant nor a unique.
    fn find(&self, index: &Index, ns: &str, name: &str, by_ref: bool) -> Option<Vec<&BaseType>> {
        let start = *index.get(&format!("{ns}::{name}"))?;
        let mut out = Vec::new();
        for record in self.items.get(start..)? {
            let key = if by_ref { &record.ref_name } else { &record.name };
            if record.namespace == ns && key == name {
                out.push(record);
                if record.disc.is_none() && record.unique.is_none() {
                    break;
                }
            } else {
                break;
            }
        }
        Some(out)
    }

    /// `ITEM_BY_TRANSLATED`
    #[must_use]
    pub fn by_translated(&self, ns: &str, name: &str) -> Option<Vec<&BaseType>> {
        self.find(&self.by_name, ns, name, false)
    }

    /// Every unique built on the base `base_ref`, one record per name, in data order.
    #[must_use]
    pub fn uniques_of_base(&self, base_ref: &str) -> Vec<&BaseType> {
        let mut seen = HashSet::new();
        self.items
            .iter()
            .filter(|i| i.namespace == "UNIQUE" && i.unique.as_ref().is_some_and(|u| u.base == base_ref))
            .filter(|i| seen.insert(i.ref_name.as_str()))
            .collect()
    }

    /// `ITEM_BY_REF`
    #[must_use]
    pub fn by_ref(&self, ns: &str, ref_name: &str) -> Option<Vec<&BaseType>> {
        self.find(&self.by_ref, ns, ref_name, true)
    }
}

pub struct StatDb {
    lines: Vec<StatOrGroup>,
    by_ref: Index,
    by_matcher: Index,
}

impl StatDb {
    pub fn parse(ndjson: &str) -> Result<Self> {
        Self::parse_with_extra(ndjson, "")
    }

    /// `extra` (see `data_update`) holds either whole stats, appended after the shipped ones, or
    /// `{"patch": <ref>, "ids": {<type>: [<id>]}}` lines that give a shipped stat trade ids for
    /// modifier types it lacks.
    pub fn parse_with_extra(ndjson: &str, extra: &str) -> Result<Self> {
        let mut lines = Vec::new();
        for (n, line) in ndjson.lines().enumerate() {
            lines.push(
                serde_json::from_str::<StatOrGroup>(line).with_context(|| format!("stats.ndjson line {}", n + 1))?,
            );
        }
        let mut plain: HashMap<String, usize> = HashMap::new();
        for (n, line) in lines.iter().enumerate() {
            if let StatOrGroup::Stat(s) = line {
                plain.insert(s.ref_.clone(), n);
            }
        }
        for (n, line) in extra.lines().enumerate().filter(|(_, l)| !l.is_empty()) {
            let context = || format!("stats.extra.ndjson line {}", n + 1);
            let value: Value = serde_json::from_str(line).with_context(context)?;
            let Some(target) = value.get("patch").and_then(Value::as_str) else {
                lines.push(serde_json::from_value(value).with_context(context)?);
                continue;
            };
            let ids: BTreeMap<String, Vec<String>> =
                serde_json::from_value(value.get("ids").cloned().unwrap_or_default()).with_context(context)?;
            if let Some(StatOrGroup::Stat(stat)) = plain.get(target).and_then(|&at| lines.get_mut(at)) {
                for (kind, list) in ids {
                    stat.trade.ids.entry(kind).or_insert(list);
                }
            }
        }

        let (mut by_ref, mut by_matcher) = (Index::new(), Index::new());
        for (n, line) in lines.iter().enumerate() {
            for stat in line.stats() {
                insert_first(&mut by_ref, stat.ref_.clone(), n);
                for m in &stat.matchers {
                    insert_first(
                        &mut by_matcher,
                        m.advanced.as_deref().unwrap_or(&m.string).to_owned(),
                        n,
                    );
                }
            }
        }
        Ok(Self {
            lines,
            by_ref,
            by_matcher,
        })
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.lines.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// `STAT_BY_REF_V2`
    #[must_use]
    pub fn by_ref(&self, ref_: &str) -> Option<&StatOrGroup> {
        self.lines.get(*self.by_ref.get(ref_)?)
    }

    /// `STAT_BY_MATCH_STR_V2`
    #[must_use]
    pub fn by_match_str_v2(&self, match_str: &str) -> Option<&StatOrGroup> {
        let found = self.lines.get(*self.by_matcher.get(match_str)?)?;
        let is_match = |m: &StatMatcher| m.string == match_str || m.advanced.as_deref() == Some(match_str);
        found
            .stats()
            .iter()
            .any(|s| s.matchers.iter().any(is_match))
            .then_some(found)
    }

    /// `STAT_BY_MATCH_STR`: a single stat and the matcher that matched.
    #[must_use]
    pub fn by_match_str(&self, match_str: &str) -> Option<(&Stat, &StatMatcher)> {
        let is_match = |m: &StatMatcher| m.string == match_str || m.advanced.as_deref() == Some(match_str);
        let stat = match self.by_match_str_v2(match_str)? {
            StatOrGroup::Group(g) => {
                let mut found = g.stats.iter().filter(|s| s.matchers.iter().any(is_match));
                let stat = found.next()?;
                if found.next().is_some() {
                    return None;
                }
                stat
            }
            StatOrGroup::Stat(s) => s,
        };
        let matcher = stat.matchers.iter().find(|m| is_match(m))?;
        Some((stat, matcher))
    }
}

pub struct GameData {
    pub cs: ClientStrings,
    pub items: ItemDb,
    pub stats: StatDb,
}

impl GameData {
    /// Loads `<data_dir>/en/{client_strings.json,items.ndjson,stats.ndjson}`.
    pub fn load(data_dir: &Path) -> Result<Self> {
        Self::load_with_extra(data_dir, None)
    }

    /// Like `load`, plus the items in `<extra_dir>/en/items.extra.ndjson` (see
    /// `data_update`) when that file exists. Shipped items come first, so they win on a clash.
    pub fn load_with_extra(data_dir: &Path, extra_dir: Option<&Path>) -> Result<Self> {
        let dir = data_dir.join("en");
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name)).with_context(|| format!("reading {}/{name}", dir.display()))
        };
        let mut items = read("items.ndjson")?;
        if let Some(extra) = extra_dir.map(|d| d.join("en").join(EXTRA_ITEMS)) {
            if let Ok(text) = std::fs::read_to_string(&extra) {
                if !items.ends_with('\n') {
                    items.push('\n');
                }
                items.push_str(&text);
            }
        }
        let stats_extra = extra_dir
            .and_then(|d| std::fs::read_to_string(d.join("en").join(EXTRA_STATS)).ok())
            .unwrap_or_default();
        Ok(Self {
            cs: ClientStrings::parse(&read("client_strings.json")?)?,
            items: ItemDb::parse(&items)?,
            stats: StatDb::parse_with_extra(&read("stats.ndjson")?, &stats_extra)?,
        })
    }
}

/// File name of the generated extra items inside `<extra_dir>/en/`.
pub const EXTRA_ITEMS: &str = "items.extra.ndjson";
/// File name of the generated extra stats (`StatDb::parse_with_extra`).
pub const EXTRA_STATS: &str = "stats.extra.ndjson";

#[cfg(test)]
#[expect(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]
mod tests {
    use super::*;

    pub fn data_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    fn normalise(v: Value) -> Value {
        match v {
            Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
            Value::Array(a) => Value::Array(a.into_iter().map(normalise).collect()),
            Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalise(v))).collect()),
            other => other,
        }
    }

    /// The typed model must lose nothing: re-serializing every record gives the original JSON.
    #[test]
    fn data_round_trips_losslessly() {
        let dir = data_dir().join("en");
        for line in std::fs::read_to_string(dir.join("items.ndjson")).unwrap().lines() {
            let original: Value = serde_json::from_str(line).unwrap();
            let typed: BaseType = serde_json::from_str(line).unwrap();
            assert_eq!(
                normalise(serde_json::to_value(&typed).unwrap()),
                normalise(original),
                "{line}"
            );
        }
        for line in std::fs::read_to_string(dir.join("stats.ndjson")).unwrap().lines() {
            let original: Value = serde_json::from_str(line).unwrap();
            let typed: StatOrGroup = serde_json::from_str(line).unwrap();
            assert_eq!(
                normalise(serde_json::to_value(&typed).unwrap()),
                normalise(original),
                "{line}"
            );
        }
    }

    #[test]
    fn lookups_work() {
        let data = GameData::load(&data_dir()).unwrap();
        assert!(data.items.len() > 1000 && data.stats.len() > 1000);
        let divine = data.items.by_translated("ITEM", "Divine Orb").unwrap();
        assert_eq!(divine[0].ref_name, "Divine Orb");
        assert!(data.items.by_translated("ITEM", "Not A Real Item").is_none());
        let (stat, matcher) = data
            .stats
            .by_match_str("# Added Passive Skills are Jewel Sockets")
            .unwrap();
        assert_eq!(stat.ref_, "# Added Passive Skills are Jewel Sockets");
        assert_eq!(matcher.string, "# Added Passive Skills are Jewel Sockets");
    }
}
