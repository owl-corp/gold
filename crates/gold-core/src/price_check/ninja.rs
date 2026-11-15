//! poe.ninja prices.
use anyhow::{bail, Result};
use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;

/// poe.ninja overview type -> (namespace in our item data, url segment on poe.ninja)
const NAMESPACE_MAP: &[(&str, &str, &str)] = &[
    ("Currency", "ITEM", "currency"),
    ("Fragment", "ITEM", "fragments"),
    ("DeliriumOrb", "ITEM", "delirium-orbs"),
    ("Scarab", "ITEM", "scarabs"),
    ("Artifact", "ITEM", "artifacts"),
    ("BaseType", "ITEM", "base-types"),
    ("Fossil", "ITEM", "fossils"),
    ("Resonator", "ITEM", "resonators"),
    ("Incubator", "ITEM", "incubators"),
    ("Oil", "ITEM", "oils"),
    ("Vial", "ITEM", "vials"),
    ("Invitation", "ITEM", "invitations"),
    ("BlightedMap", "ITEM", "blighted-maps"),
    ("BlightRavagedMap", "ITEM", "blight-ravaged-maps"),
    ("Essence", "ITEM", "essences"),
    ("Map", "ITEM", "maps"),
    ("Tattoo", "ITEM", "tattoos"),
    ("Omen", "ITEM", "omens"),
    ("Coffin", "ITEM", "coffins"),
    ("AllflameEmber", "ITEM", "allflame-embers"),
    ("DjinnCoin", "ITEM", "djinn-coins"),
    ("Astrolabe", "ITEM", "astrolabes"),
    ("Runegraft", "ITEM", "runegrafts"),
    ("Ducat", "ITEM", "ducats"),
    ("EnshroudingCrystal", "ITEM", "enshrouding-crystals"),
    ("Corpse", "ITEM", "corpses"),
    ("Thread", "ITEM", "threads"),
    ("DivinationCard", "DIVINATION_CARD", "divination-cards"),
    ("Beast", "CAPTURED_BEAST", "beasts"),
    ("UniqueJewel", "UNIQUE", "unique-jewels"),
    ("UniqueFlask", "UNIQUE", "unique-flasks"),
    ("UniqueWeapon", "UNIQUE", "unique-weapons"),
    ("UniqueArmour", "UNIQUE", "unique-armours"),
    ("UniqueAccessory", "UNIQUE", "unique-accessories"),
    ("UniqueMap", "UNIQUE", "unique-maps"),
    ("UniqueRelic", "UNIQUE", "unique-relics"),
    ("UniqueTincture", "UNIQUE", "unique-tinctures"),
    ("SkillGem", "GEM", "skill-gems"),
];

const UPDATE_INTERVAL_SECS: u64 = 31 * 60;

#[derive(Clone, Debug, Deserialize)]
pub struct DenseInfo {
    pub name: String,
    #[serde(default)]
    pub variant: Option<String>,
    pub chaos: f64,
    #[serde(default)]
    pub graph: Vec<Option<f64>>,
}

#[derive(Deserialize)]
struct Overview {
    #[serde(rename = "type")]
    type_: String,
    #[serde(default)]
    lines: Vec<DenseInfo>,
}

#[derive(Deserialize)]
struct Response {
    #[serde(rename = "currencyOverviews", default)]
    currency: Vec<Overview>,
    #[serde(rename = "itemOverviews", default)]
    items: Vec<Overview>,
}

struct Section {
    ns: &'static str,
    url: &'static str,
    lines: Vec<DenseInfo>,
}

#[derive(Clone, Debug)]
pub struct Price {
    pub info: DenseInfo,
    pub url: String,
}

pub struct NinjaDb {
    league: String,
    sections: Vec<Section>,
    fetched_at: std::time::Instant,
}

impl NinjaDb {
    pub async fn fetch(client: &reqwest::Client, league: &str) -> Result<Self> {
        let response = client
            .get("https://poe.ninja/poe1/api/economy/current/dense/overviews")
            .query(&[("league", league), ("language", "en")])
            .send()
            .await?;
        if !response.status().is_success() {
            bail!("poe.ninja request failed: HTTP {}", response.status());
        }
        Self::from_json(league, &response.bytes().await?)
    }

    pub fn from_json(league: &str, json: &[u8]) -> Result<Self> {
        let response: Response = serde_json::from_slice(json)?;
        // sections keep the order of the response: the currency overviews come first
        let sections = response
            .currency
            .into_iter()
            .chain(response.items)
            .filter_map(|o| {
                let (_, ns, url) = NAMESPACE_MAP.iter().find(|(t, _, _)| *t == o.type_)?;
                Some(Section {
                    ns,
                    url,
                    lines: o.lines,
                })
            })
            .collect();
        Ok(NinjaDb {
            league: league.to_string(),
            sections,
            fetched_at: std::time::Instant::now(),
        })
    }

    pub fn league(&self) -> &str {
        &self.league
    }

    pub fn is_stale(&self) -> bool {
        self.fetched_at.elapsed().as_secs() > UPDATE_INTERVAL_SECS
    }

    /// `findPriceByQuery`: the first entry with exactly this name and variant.
    pub fn find(&self, ns: &str, name: &str, variant: Option<&str>) -> Option<Price> {
        for section in self.sections.iter().filter(|s| s.ns == ns) {
            if let Some(info) = section
                .lines
                .iter()
                .find(|l| l.name == name && l.variant.as_deref() == variant)
            {
                let url = format!(
                    "https://poe.ninja/poe1/economy/{}/{}/{}",
                    league_to_url(&self.league),
                    section.url,
                    details_id(info)
                );
                return Some(Price {
                    info: info.clone(),
                    url,
                });
            }
        }
        None
    }

    /// Every entry of the section that `find` takes this one from, with its url.
    pub fn section_prices(&self, ns: &str, name: &str, variant: Option<&str>) -> Option<Vec<Price>> {
        let section = self.sections.iter().filter(|s| s.ns == ns).find(|s| {
            s.lines
                .iter()
                .any(|l| l.name == name && l.variant.as_deref() == variant)
        })?;
        Some(
            section
                .lines
                .iter()
                .map(|info| Price {
                    info: info.clone(),
                    url: format!(
                        "https://poe.ninja/poe1/economy/{}/{}/{}",
                        league_to_url(&self.league),
                        section.url,
                        details_id(info)
                    ),
                })
                .collect(),
        )
    }

    /// Mean chaos value of every entry in `ns` whose name ends with `suffix` (None if there is none).
    pub fn average_chaos_by_suffix(&self, ns: &str, suffix: &str) -> Option<f64> {
        let values: Vec<f64> = self
            .sections
            .iter()
            .filter(|s| s.ns == ns)
            .flat_map(|s| &s.lines)
            .filter(|l| l.name.ends_with(suffix))
            .map(|l| l.chaos)
            .collect();
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    }

    /// Chaos orbs per Divine Orb, when poe.ninja has a believable value.
    pub fn divine_rate(&self) -> Option<f64> {
        let divine = self.find("ITEM", "Divine Orb", None)?;
        (divine.info.chaos >= 30.0).then_some(divine.info.chaos)
    }
}

fn league_to_url(league: &str) -> String {
    match league {
        "Standard" => "standard".into(),
        "Hardcore" => "hardcore".into(),
        _ => {
            let mut id = league.replace("Hardcore ", "").to_lowercase();
            if league.starts_with("Hardcore ") {
                id.push_str("hc");
            }
            id
        }
    }
}

fn details_id(info: &DenseInfo) -> String {
    let text = match &info.variant {
        Some(v) if !v.is_empty() => format!("{}, {}", info.name, v),
        _ => info.name.clone(),
    };
    text.nfkd()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | ' '))
        .collect::<String>()
        .to_lowercase()
        .replace(' ', "-")
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurrencyValue {
    pub min: f64,
    pub max: f64,
    pub currency: Currency,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Currency {
    Chaos,
    Div,
}

/// `autoCurrency` for a single value: show it in divines when it is close to one or more.
pub fn auto_currency(value: f64, xchg_rate: Option<f64>) -> CurrencyValue {
    let rate = xchg_rate.unwrap_or(9999.0);
    if value > rate * 0.94 {
        if value < rate * 1.06 {
            return CurrencyValue {
                min: 1.0,
                max: 1.0,
                currency: Currency::Div,
            };
        }
        let div = value / rate;
        return CurrencyValue {
            min: div,
            max: div,
            currency: Currency::Div,
        };
    }
    CurrencyValue {
        min: value,
        max: value,
        currency: Currency::Chaos,
    }
}

/// JavaScript's `toFixed(1)` for |v| < 10: the exact decimal value, rounded half away from
/// zero. Rust's `{:.1}` rounds exact halves to even (1.25 gives 1.2, JavaScript 1.3).
fn to_fixed_1(v: f64) -> String {
    let exact = format!("{:.60}", v.abs());
    let (int, frac) = exact.split_once('.').unwrap_or((&exact, "0"));
    let digits = frac.as_bytes();
    let mut tenths = int.parse::<u64>().unwrap_or(0) * 10 + u64::from(digits[0] - b'0');
    if digits[1] >= b'5' {
        tenths += 1;
    }
    let out = format!("{}.{}", tenths / 10, tenths % 10);
    if v < 0.0 && tenths != 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// `displayRounding`
pub fn display_rounding(value: f64, fraction: bool) -> String {
    if fraction && value.abs() < 1.0 {
        if value == 0.0 {
            return "0".into();
        }
        let r = format!("1\u{200A}/\u{200A}{}", display_rounding(1.0 / value, false));
        return if r == "1\u{200A}/\u{200A}1" { "1".into() } else { r };
    }
    if value.abs() < 10.0 {
        let rounded = to_fixed_1(value);
        let trimmed = rounded.strip_suffix(".0").unwrap_or(&rounded);
        return trimmed.replace('.', "\u{200A}.\u{200A}");
    }
    format!("{}", crate::js::round(value))
}

/// The price movement shown next to a trend (`deltaFromGraph` in `PriceTrend.vue`).
#[derive(Clone, Debug, PartialEq)]
pub struct Trend {
    pub forecast: &'static str,
    pub text: String,
}

pub fn delta_from_graph(graph: &[Option<f64>]) -> Option<Trend> {
    let points: Vec<f64> = graph.iter().flatten().copied().collect();
    if points.len() < 2 {
        return None;
    }

    let mut forecast = "const";
    if points.len() == 7 {
        if points.iter().filter(|p| **p > 0.0).count() >= 4 || points[4..].iter().all(|p| *p > 0.0) {
            forecast = "up";
        } else if points.iter().filter(|p| **p < 0.0).count() >= 4 || points[4..].iter().all(|p| *p < 0.0) {
            forecast = "down";
        }
    }

    let mean = points.iter().sum::<f64>() / points.len() as f64;
    let variance = points.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (points.len() - 1) as f64;
    let change = variance.sqrt();
    Some(Trend {
        forecast,
        text: format!("{}\u{2009}%", crate::js::round(change * 2.0)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "currencyOverviews":[{"type":"Currency","lines":[
        {"name":"Divine Orb","chaos":150.5,"graph":[null,null,null,null,null,null,3.2]}]}],
      "itemOverviews":[{"type":"UniqueAccessory","lines":[
        {"name":"Mageblood","variant":"4 Flasks, Heavy Belt","chaos":7227,"graph":[]}]}]}"#;

    #[test]
    fn finds_exact_name_and_variant() {
        let db = NinjaDb::from_json("Covens", SAMPLE.as_bytes()).unwrap();
        let divine = db.find("ITEM", "Divine Orb", None).unwrap();
        assert_eq!(divine.info.chaos, 150.5);
        assert_eq!(divine.url, "https://poe.ninja/poe1/economy/covens/currency/divine-orb");
        assert_eq!(db.divine_rate(), Some(150.5));
        assert!(db.find("ITEM", "Divine Orb", Some("x")).is_none());
        let m = db.find("UNIQUE", "Mageblood", Some("4 Flasks, Heavy Belt")).unwrap();
        assert!(m.url.ends_with("/unique-accessories/mageblood-4-flasks-heavy-belt"));
        assert!(db.find("UNIQUE", "Mageblood", None).is_none());
    }

    #[test]
    fn thread_overview_is_mapped_and_averaged() {
        let json = r#"{"currencyOverviews":[],"itemOverviews":[{"type":"Thread","lines":[
            {"name":"Abyss Thread","chaos":1,"graph":[]},
            {"name":"Ultimatum Thread","chaos":3,"graph":[]}]}]}"#;
        let db = NinjaDb::from_json("Covens", json.as_bytes()).unwrap();
        assert!(db.find("ITEM", "Ultimatum Thread", None).is_some());
        assert_eq!(db.average_chaos_by_suffix("ITEM", " Thread"), Some(2.0));
    }

    #[test]
    fn rounds_halves_like_javascript() {
        assert_eq!(display_rounding(1.25, false), "1\u{200A}.\u{200A}3");
        assert_eq!(display_rounding(0.25, false), "0\u{200A}.\u{200A}3");
        assert_eq!(display_rounding(-0.04, false), "0");
        assert_eq!(display_rounding(8.75, false), "8\u{200A}.\u{200A}8");
    }

    #[test]
    fn currency_display() {
        assert_eq!(
            auto_currency(100.0, Some(150.0)),
            CurrencyValue {
                min: 100.0,
                max: 100.0,
                currency: Currency::Chaos
            }
        );
        assert_eq!(auto_currency(150.0, Some(150.0)).currency, Currency::Div);
        assert_eq!(auto_currency(150.0, Some(150.0)).min, 1.0);
        assert_eq!(auto_currency(300.0, Some(150.0)).min, 2.0);
        assert_eq!(display_rounding(2.54, false), "2\u{200A}.\u{200A}5");
        assert_eq!(display_rounding(42.4, false), "42");
        assert_eq!(display_rounding(0.5, true), "1\u{200A}/\u{200A}2");
    }

    #[test]
    fn league_urls() {
        assert_eq!(league_to_url("Standard"), "standard");
        assert_eq!(league_to_url("Hardcore Covens"), "covenshc");
        assert_eq!(league_to_url("Covens"), "covens");
    }

    #[test]
    fn trend_text() {
        let t = delta_from_graph(&[None, None, None, None, None, None, Some(-17.05)]);
        assert!(t.is_none());
        let t = delta_from_graph(&[
            Some(1.0),
            Some(2.0),
            Some(3.0),
            Some(4.0),
            Some(5.0),
            Some(6.0),
            Some(7.0),
        ])
        .unwrap();
        assert_eq!(t.forecast, "up");
    }
}
