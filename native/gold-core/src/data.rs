//! Game data (`items.ndjson`, `stats.ndjson`) and the lookups the parser uses.
//!
//! Records are found through hash indexes built in memory and binary-searched, so duplicate
//! keys always resolve to the same record.
use crate::client_strings::ClientStrings;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
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
#[allow(clippy::large_enum_variant)]
pub enum StatOrGroup {
    Group(StatGroup),
    Stat(Stat),
}

impl StatOrGroup {
    fn stats(&self) -> &[Stat] {
        match self {
            StatOrGroup::Group(g) => &g.stats,
            StatOrGroup::Stat(s) => std::slice::from_ref(s),
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

/// 32-bit FNV-1a over the UTF-8 bytes.
pub fn fnv1a32(s: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for b in s.bytes() {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

/// Binary search: returns whichever equal entry the
/// midpoint probing hits first, so duplicates resolve identically.
fn binary_search(index: &[(u32, u32)], value: u32) -> Option<u32> {
    let mut left: i64 = 0;
    let mut right: i64 = index.len() as i64 - 1;
    while left <= right {
        let mid = (left + right) / 2;
        let (hash, line) = index[mid as usize];
        if hash < value {
            left = mid + 1;
        } else if hash > value {
            right = mid - 1;
        } else {
            return Some(line);
        }
    }
    None
}

pub struct ItemDb {
    items: Vec<BaseType>,
    by_name: Vec<(u32, u32)>,
    by_ref: Vec<(u32, u32)>,
}

impl ItemDb {
    pub fn parse(ndjson: &str) -> Result<Self> {
        let mut items = Vec::new();
        for (n, line) in ndjson.lines().enumerate() {
            items.push(serde_json::from_str::<BaseType>(line).with_context(|| format!("items.ndjson line {}", n + 1))?);
        }

        let mut seen = std::collections::HashSet::new();
        let (mut by_name, mut by_ref) = (Vec::new(), Vec::new());
        for (n, item) in items.iter().enumerate() {
            if seen.insert(format!("{}::{}", item.namespace, item.ref_name)) {
                by_name.push((fnv1a32(&format!("{}::{}", item.namespace, item.name)), n as u32));
                by_ref.push((fnv1a32(&format!("{}::{}", item.namespace, item.ref_name)), n as u32));
            }
        }
        by_name.sort_by_key(|e| e.0);
        by_ref.sort_by_key(|e| e.0);
        Ok(ItemDb { items, by_name, by_ref })
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn find(&self, index: &[(u32, u32)], ns: &str, name: &str, by_ref: bool) -> Option<Vec<BaseType>> {
        let start = binary_search(index, fnv1a32(&format!("{ns}::{name}")))?;
        let mut out = Vec::new();
        for record in &self.items[start as usize..] {
            let key = if by_ref { &record.ref_name } else { &record.name };
            if record.namespace == ns && key == name {
                out.push(record.clone());
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
    pub fn by_translated(&self, ns: &str, name: &str) -> Option<Vec<BaseType>> {
        self.find(&self.by_name, ns, name, false)
    }

    /// `ITEM_BY_REF`
    pub fn by_ref(&self, ns: &str, ref_name: &str) -> Option<Vec<BaseType>> {
        self.find(&self.by_ref, ns, ref_name, true)
    }
}

pub struct StatDb {
    lines: Vec<StatOrGroup>,
    by_ref: Vec<(u32, u32)>,
    by_matcher: Vec<(u32, u32)>,
}

impl StatDb {
    pub fn parse(ndjson: &str) -> Result<Self> {
        let mut lines = Vec::new();
        for (n, line) in ndjson.lines().enumerate() {
            lines.push(
                serde_json::from_str::<StatOrGroup>(line).with_context(|| format!("stats.ndjson line {}", n + 1))?,
            );
        }

        let (mut by_ref, mut by_matcher) = (Vec::new(), Vec::new());
        for (n, line) in lines.iter().enumerate() {
            for stat in line.stats() {
                by_ref.push((fnv1a32(&stat.ref_), n as u32));
                for m in &stat.matchers {
                    by_matcher.push((fnv1a32(m.advanced.as_deref().unwrap_or(&m.string)), n as u32));
                }
            }
        }
        by_ref.sort_by_key(|e| e.0);
        by_matcher.sort_by_key(|e| e.0);
        Ok(StatDb {
            lines,
            by_ref,
            by_matcher,
        })
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// `STAT_BY_REF_V2`
    pub fn by_ref(&self, ref_: &str) -> Option<StatOrGroup> {
        let line = binary_search(&self.by_ref, fnv1a32(ref_))?;
        Some(self.lines[line as usize].clone())
    }

    /// `STAT_BY_MATCH_STR_V2`. Returns a copy: callers may edit its trade ids.
    pub fn by_match_str_v2(&self, match_str: &str) -> Option<StatOrGroup> {
        let line = binary_search(&self.by_matcher, fnv1a32(match_str))?;
        let found = &self.lines[line as usize];
        let matches = |m: &StatMatcher| m.string == match_str || m.advanced.as_deref() == Some(match_str);
        found
            .stats()
            .iter()
            .any(|s| s.matchers.iter().any(matches))
            .then(|| found.clone())
    }

    /// `STAT_BY_MATCH_STR`: a single stat and the matcher that matched.
    pub fn by_match_str(&self, match_str: &str) -> Option<(Stat, StatMatcher)> {
        let matches = |m: &StatMatcher| m.string == match_str || m.advanced.as_deref() == Some(match_str);
        let stat = match self.by_match_str_v2(match_str)? {
            StatOrGroup::Group(g) => {
                let mut stats: Vec<Stat> = g.stats.into_iter().filter(|s| s.matchers.iter().any(matches)).collect();
                if stats.len() != 1 {
                    return None;
                }
                stats.remove(0)
            }
            StatOrGroup::Stat(s) => s,
        };
        let matcher = stat.matchers.iter().find(|m| matches(m))?.clone();
        Some((stat, matcher))
    }
}

pub struct GameData {
    pub cs: ClientStrings,
    pub items: ItemDb,
    pub stats: StatDb,
}

impl GameData {
    /// Loads `<data_dir>/<lang>/{client_strings.js,items.ndjson,stats.ndjson}`.
    pub fn load(data_dir: &Path, lang: &str) -> Result<Self> {
        let dir = data_dir.join(lang);
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name)).with_context(|| format!("reading {}/{name}", dir.display()))
        };
        Ok(GameData {
            cs: ClientStrings::parse(&read("client_strings.js")?)?,
            items: ItemDb::parse(&read("items.ndjson")?)?,
            stats: StatDb::parse(&read("stats.ndjson")?)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn data_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")
    }

    #[test]
    fn fnv1a_known_values() {
        assert_eq!(fnv1a32(""), 0x811c_9dc5);
        assert_eq!(fnv1a32("a"), 0xe40c_292c);
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
        let data = GameData::load(&data_dir(), "en").unwrap();
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
