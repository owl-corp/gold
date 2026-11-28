//! What the overlay shows. The QML renders this JSON as it is and keeps no state.
//! The structs below are that contract: field names are what `GoldOverlay.qml` reads.
use crate::session::{Row, SearchState, Session};
use crate::ui::TradeKey;
use gold_core::parser::item::{ItemRarity, ParsedItem};
use gold_core::price_check::details::{get_details_id, is_valuable_basetype, DetailsId};
use gold_core::price_check::filters::util::round_roll;
use gold_core::price_check::filters::FilterTag;
use gold_core::price_check::ninja::{auto_currency, delta_from_graph, display_rounding, Currency, NinjaDb};
use gold_core::price_check::related;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Placement {
    pub screen: String,
    pub x: i32,
    pub y: i32,
}

/// What `show` sends: a plain message or the overlay of a session.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum View {
    Error(ErrorView),
    Session(Box<SessionView>),
}

#[derive(Debug, Serialize)]
pub struct ErrorView {
    title: String,
    lines: Vec<String>,
    #[serde(flatten)]
    place: Placement,
}

pub fn error_view(p: &Placement, title: &str, lines: Vec<String>) -> View {
    View::Error(ErrorView {
        title: title.to_string(),
        lines,
        place: p.clone(),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    epoch: u64,
    title: String,
    subtitle: String,
    rarity: &'static str,
    price: Price,
    presets: Vec<PresetView>,
    filters: Vec<FilterView>,
    hidden_count: usize,
    show_hidden: bool,
    trade_filters: Option<TradeFiltersView>,
    search_quality: Option<i64>,
    trade: TradeView,
    lines: Vec<String>,
    /// Names to choose from when the item is an unidentified unique.
    candidates: Vec<Candidate>,
    #[serde(flatten)]
    place: Placement,
}

#[derive(Debug, Serialize)]
pub struct Candidate {
    name: String,
    icon: String,
}

/// The poe.ninja price: either a `status` message or the amount with its extras.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    average: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    groups: Option<Vec<PriceGroup>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trend: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trend_text: Option<String>,
}

impl Price {
    fn status(status: impl Into<String>) -> Self {
        Self {
            status: Some(status.into()),
            ..Self::default()
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PriceGroup {
    #[serde(skip_serializing_if = "Option::is_none")]
    heading: Option<&'static str>,
    rows: Vec<PriceRow>,
}

#[derive(Debug, Serialize)]
pub struct PriceRow {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    chaos: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    current: bool,
}

#[derive(Debug, Serialize)]
pub struct PresetView {
    index: usize,
    name: &'static str,
    active: bool,
}

#[derive(Debug, Serialize)]
pub struct FilterView {
    index: usize,
    text: String,
    tag: FilterTag,
    on: bool,
    not: bool,
    min: Option<f64>,
    max: Option<f64>,
    editable: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeFiltersView {
    currency: Option<String>,
    toggles: Vec<TradeToggle>,
}

/// An item-level filter the overlay can switch on or off.
#[derive(Debug, Serialize)]
pub struct TradeToggle {
    key: TradeKey,
    text: String,
    on: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum TradeView {
    Idle,
    Loading,
    Failed { message: String },
    Done { groups: Vec<ResultGroup> },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultGroup {
    heading: Option<String>,
    total: u64,
    url: String,
    rows: Vec<RowView>,
    more: bool,
    fetching_more: bool,
}

#[derive(Debug, Serialize)]
pub struct RowView {
    price: String,
    detail: String,
    status: &'static str,
    who: String,
    mine: bool,
    count: u32,
}

const fn rarity_name(item: &ParsedItem) -> &'static str {
    match item.rarity {
        Some(ItemRarity::Normal) => "normal",
        Some(ItemRarity::Magic) => "magic",
        Some(ItemRarity::Rare) => "rare",
        Some(ItemRarity::Unique) => "unique",
        None => "currency",
    }
}

/// The whole overlay for a session.
pub fn render(s: &Session) -> View {
    let item = &s.item;

    let (title, subtitle) = if item.info.ref_name == item.info.name && item.rarity != Some(ItemRarity::Unique) {
        (item.info.name.clone(), String::new())
    } else {
        (
            item.info.name.clone(),
            item.unique_base.as_ref().map(|b| b.name.clone()).unwrap_or_default(),
        )
    };

    let price = match &s.ninja {
        None => Price::status("Loading prices…"),
        Some(Err(e)) => Price::status(format!("Prices unavailable: {e}")),
        Some(Ok(db)) => ninja_price(item, db),
    };

    let filters = filter_views(s);

    let hidden_count = s.preset().map_or(0, |p| {
        p.stats.iter().filter(|f| f.hidden.is_some() && f.disabled).count()
    });
    let trade_filters = s.preset().map(|p| {
        let f = &p.filters;
        let mut toggles = Vec::new();
        if let Some(r) = f.search_relaxed.as_ref().filter(|r| r.sub.is_none()) {
            let any = r.disabled != Some(true);
            toggles.push(TradeToggle {
                key: TradeKey::AnyBase,
                text: if any { "any base" } else { "this base only" }.into(),
                on: any,
            });
        }
        if let Some(r) = f.rarity.as_ref().filter(|r| r.value == "magic") {
            toggles.push(TradeToggle {
                key: TradeKey::MagicOnly,
                text: "magic only".into(),
                on: !r.disabled,
            });
        }
        if let Some(l) = &f.item_level {
            let text = l.max.filter(|m| *m != 0).map_or_else(
                || format!("ilvl ≥ {}", l.value),
                |max| format!("ilvl {}–{max}", l.value),
            );
            toggles.push(TradeToggle {
                key: TradeKey::ItemLevel,
                text,
                on: !l.disabled,
            });
        }
        for (i, inf) in f.influences.iter().flatten().enumerate() {
            toggles.push(TradeToggle {
                key: TradeKey::Influence(i),
                text: format!("{:?} influence", inf.value),
                on: !inf.disabled,
            });
        }
        TradeFiltersView {
            currency: f.trade.currency.clone(),
            toggles,
        }
    });

    // the quality minimum the search uses (gems search below their own quality)
    let quality = s
        .preset()
        .and_then(|p| p.filters.quality.as_ref())
        .filter(|q| !q.disabled)
        .map(|q| q.value);

    let presets = s
        .presets
        .iter()
        .enumerate()
        .map(|(index, p)| PresetView {
            index,
            name: preset_name(p.id),
            active: index == s.active,
        })
        .collect();

    let trade = trade_view(s);

    View::Session(Box::new(SessionView {
        epoch: s.epoch,
        title,
        subtitle,
        rarity: rarity_name(item),
        price,
        presets,
        filters,
        hidden_count,
        show_hidden: s.show_hidden,
        trade_filters,
        search_quality: quality,
        trade,
        lines: s
            .note
            .iter()
            .cloned()
            .chain(item.unknown_modifiers.iter().map(|m| format!("? {}", m.text)))
            .collect(),
        candidates: candidate_views(s),
        place: s.place.clone(),
    }))
}

fn candidate_views(s: &Session) -> Vec<Candidate> {
    s.candidates
        .iter()
        .map(|c| Candidate {
            name: c.name.clone(),
            icon: c.icon.clone(),
        })
        .collect()
}

/// The stat filter rows of the active preset.
fn filter_views(s: &Session) -> Vec<FilterView> {
    s.preset()
        .map(|p| {
            p.stats
                .iter()
                .enumerate()
                // hidden filters that are switched on still shape the search, so they stay visible
                .filter(|(_, f)| s.show_hidden || f.hidden.is_none() || !f.disabled)
                .map(|(index, f)| {
                    let value = f.roll.as_ref().map(|r| r.value);
                    let text = value.map_or_else(
                        || f.text.clone(),
                        |v| {
                            f.text.replace(
                                '#',
                                &display_rounding(round_roll(v, f.roll.as_ref().is_some_and(|r| r.dp)), false),
                            )
                        },
                    );
                    FilterView {
                        index,
                        text,
                        tag: f.tag,
                        on: !f.disabled,
                        not: f.not.unwrap_or(false),
                        min: f.roll.as_ref().and_then(|r| r.min),
                        max: f.roll.as_ref().and_then(|r| r.max),
                        editable: f.roll.is_some(),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The trade search state as the overlay shows it.
fn trade_view(s: &Session) -> TradeView {
    match &s.search {
        SearchState::Idle => TradeView::Idle,
        SearchState::Loading => TradeView::Loading,
        SearchState::Failed(message) => TradeView::Failed {
            message: message.clone(),
        },
        SearchState::Done(groups) => TradeView::Done {
            groups: groups
                .iter()
                .map(|g| ResultGroup {
                    heading: g.heading.clone(),
                    total: g.total,
                    url: g.url.clone(),
                    rows: grouped_rows(&g.rows),
                    more: g.fetch_id.is_some() && !g.pending.is_empty(),
                    fetching_more: s.fetching_more,
                })
                .collect(),
        },
    }
}

/// Amount and unit text for a chaos value, in divines when that reads better.
fn fmt_price(rate: Option<f64>, chaos: f64, is_divine: bool) -> (String, &'static str) {
    let v = auto_currency(chaos, if is_divine { None } else { rate });
    match v.currency {
        Currency::Chaos if is_divine => (display_rounding(chaos, false), "c"),
        // under 0.1c rounding shows 0 or a coarse 0.1: say how many a chaos buys
        Currency::Chaos if v.min > 0.0 && v.min < 0.1 => (display_rounding(1.0 / v.min, false), "per c"),
        Currency::Chaos => (display_rounding(v.min, false), "c"),
        Currency::Div => (display_rounding(v.min, false), "div"),
    }
}

/// Name prefixes and a suffix of the items ranked together with this one.
fn same_set(id: &DetailsId) -> Option<(Vec<String>, &'static str)> {
    if id.ns != "ITEM" {
        return None;
    }
    for suffix in [" Delirium Orb", " Catalyst"] {
        if id.name.ends_with(suffix) {
            return Some((vec![String::new()], suffix));
        }
    }
    // the tierless essences (Delirium, Horror, Hysteria, Insanity) are as dear as Deafening
    let tier = match id.name.split_once(" Essence of ") {
        Some((tier, _)) => tier,
        None if id.name.starts_with("Essence of ") => "Deafening",
        None => return None,
    };
    let mut prefixes = vec![format!("{tier} Essence of ")];
    if tier == "Deafening" {
        prefixes.push("Essence of ".to_string());
    }
    Some((prefixes, ""))
}

/// Related items, or else all of a small category, not only this item.
fn price_groups(
    db: &NinjaDb,
    id: &DetailsId,
    found: &gold_core::price_check::ninja::Price,
    rate: Option<f64>,
) -> Option<Vec<PriceGroup>> {
    let fmt = |chaos: f64, is_divine: bool| fmt_price(rate, chaos, is_divine);
    let row = |name: String, p: Option<(f64, String)>, current: bool| match p {
        Some((chaos, url)) => {
            let (amount, unit) = fmt(chaos, name == "Divine Orb");
            PriceRow {
                name,
                chaos: Some(chaos),
                amount: Some(amount),
                unit: Some(unit),
                url: Some(url),
                current,
            }
        }
        None => PriceRow {
            name,
            chaos: None,
            amount: None,
            unit: None,
            url: None,
            current,
        },
    };
    // an essence is ranked against every essence of its tier, a Delirium Orb or Catalyst against all of its kind
    if let Some((prefixes, suffix)) = same_set(id) {
        if let Some(all) = db.section_prices(&id.ns, &id.name, id.variant.as_deref()) {
            let mut rows: Vec<PriceRow> = all
                .iter()
                .filter(|p| prefixes.iter().any(|x| p.info.name.starts_with(x)) && p.info.name.ends_with(suffix))
                .map(|p| {
                    let current = p.info.name == found.info.name && p.info.variant == found.info.variant;
                    row(p.info.name.clone(), Some((p.info.chaos, p.url.clone())), current)
                })
                .collect();
            if rows.len() > 1 {
                sort_by_price(&mut rows);
                return Some(vec![PriceGroup { heading: None, rows }]);
            }
        }
    }
    let curated = related::find_group(&id.ns, &id.name, id.variant.as_deref()).map(|g| {
        let rows = |ids: &[String], heading: &'static str| {
            let keys: Vec<_> = ids.iter().filter_map(|i| related::parse_key(i)).collect();
            let mut rows: Vec<PriceRow> = keys
                .iter()
                .map(|k| {
                    // the variant tells apart entries that share a name
                    let dup = keys.iter().filter(|o| o.name == k.name).count() > 1;
                    let name = match k.variant {
                        Some(v) if dup => format!("{} ({v})", k.name),
                        _ => k.name.to_string(),
                    };
                    let p = db.find(k.ns, k.name, k.variant).map(|p| (p.info.chaos, p.url));
                    let current = k.ns == id.ns && k.name == id.name && k.variant == id.variant.as_deref();
                    row(name, p, current)
                })
                .collect();
            sort_by_price(&mut rows);
            (!rows.is_empty()).then_some(PriceGroup {
                heading: Some(heading),
                rows,
            })
        };
        [rows(&g.query, "Related"), rows(&g.items, "Drops / used for")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
    });
    let is_omen = id.ns == "ITEM" && id.name.starts_with("Omen of ");
    curated.filter(|g| !g.is_empty()).or_else(|| {
        let all = db.section_prices(&id.ns, &id.name, id.variant.as_deref())?;
        (all.len() > 1 && (all.len() <= SMALL_CATEGORY || is_omen)).then(|| {
            let mut rows: Vec<PriceRow> = all
                .iter()
                .map(|p| {
                    let name = match p.info.variant.as_deref() {
                        Some(v) if !v.is_empty() => format!("{}, {v}", p.info.name),
                        _ => p.info.name.clone(),
                    };
                    let current = p.info.name == found.info.name && p.info.variant == found.info.variant;
                    row(name, Some((p.info.chaos, p.url.clone())), current)
                })
                .collect();
            sort_by_price(&mut rows);
            vec![PriceGroup { heading: None, rows }]
        })
    })
}

/// Listings from the same seller at the same price are one row with a count.
fn grouped_rows(rows: &[Row]) -> Vec<RowView> {
    let mut groups: Vec<(&Row, u32)> = Vec::new();
    for r in rows {
        match groups
            .iter_mut()
            .find(|(g, _)| g.who == r.who && g.price == r.price && g.status == r.status)
        {
            Some((_, n)) => *n += 1,
            None => groups.push((r, 1)),
        }
    }
    groups
        .into_iter()
        .map(|(r, count)| RowView {
            price: r.price.clone(),
            detail: r.detail.clone(),
            status: r.status,
            who: r.who.clone(),
            mine: r.mine,
            count,
        })
        .collect()
}

fn preset_name(id: &str) -> &'static str {
    match id {
        "filters.preset_pseudo" => "Similar",
        "filters.preset_exact" => "Exact",
        "filters.preset_bulk" => "Bulk",
        "filters.preset_base_item" => "Base item",
        _ => "Search",
    }
}

/// Most valuable first; entries without a price go last. Stable, so ties keep their order.
fn sort_by_price(rows: &mut [PriceRow]) {
    let chaos = |r: &PriceRow| r.chaos.unwrap_or(f64::NEG_INFINITY);
    rows.sort_by(|a, b| chaos(b).total_cmp(&chaos(a)));
}

/// Sections of poe.ninja with at most this many entries are shown whole.
const SMALL_CATEGORY: usize = 10;

fn ninja_price(item: &ParsedItem, db: &NinjaDb) -> Price {
    if is_valuable_basetype(item) && item.rarity != Some(ItemRarity::Unique) {
        // Equipment is priced by its mods on the trade site, not by poe.ninja.
        return Price::status("Priced by its mods on the trade site");
    }
    let Some(id) = get_details_id(item) else {
        return Price::status("No price data for this item");
    };
    let Some(found) = db.find(&id.ns, &id.name, id.variant.as_deref()) else {
        return Price::status("Not listed on poe.ninja");
    };

    let rate = db.divine_rate();
    let fmt = |chaos: f64, is_divine: bool| fmt_price(rate, chaos, is_divine);
    let (amount, unit) = fmt(found.info.chaos, item.info.ref_name == "Divine Orb");
    let mut price = Price {
        amount: Some(amount),
        unit: Some(unit),
        url: Some(found.url.clone()),
        ..Price::default()
    };
    if id.ns == "ITEM" && id.name.ends_with(" Thread") {
        if let Some(avg) = db.average_chaos_by_suffix("ITEM", " Thread") {
            let v = auto_currency(avg, rate);
            let (n, u) = match v.currency {
                Currency::Chaos => (display_rounding(v.min, false), "c"),
                Currency::Div => (display_rounding(v.min, false), "div"),
            };
            price.average = Some(format!("all threads avg {n} {u}"));
        }
    }
    price.groups = price_groups(db, &id, &found, rate);
    if let Some(trend) = delta_from_graph(&found.info.graph) {
        price.trend = Some(trend.forecast);
        price.trend_text = Some(trend.text);
    }
    price
}

#[cfg(test)]
#[expect(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests may panic on failure")]
mod tests {
    use super::*;
    use crate::session::Session;
    use gold_core::data::GameData;
    use gold_core::parser::parse_clipboard;
    use std::sync::Arc;

    const NINJA: &str = r#"{"currencyOverviews":[{"type":"Currency","lines":[
        {"name":"Chaos Orb","chaos":1,"graph":[null,null,null,null,null,null,-37.23]},
        {"name":"Divine Orb","chaos":150.5,"graph":[null,null,null,null,null,null,3.2]}]}],"itemOverviews":[]}"#;

    fn view_of(text: &str) -> serde_json::Value {
        let data = GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")).unwrap();
        let item = parse_clipboard(&data, text).unwrap();
        let db = Ok(Arc::new(NinjaDb::from_json("Covens", NINJA.as_bytes()).unwrap()));
        let view = render(&Session {
            epoch: 1,
            item,
            place: Placement {
                screen: "DP-1".into(),
                x: 10,
                y: 20,
            },
            presets: vec![],
            active: 0,
            ninja: Some(db),
            search: SearchState::Idle,
            search_gen: 0,
            note: None,
            show_hidden: false,
            fetching_more: false,
            candidates: vec![],
        });
        serde_json::to_value(view).unwrap()
    }

    #[test]
    fn small_ninja_category_lists_every_entry() {
        let v = view_of("Item Class: Stackable Currency\nRarity: Currency\nDivine Orb\n--------\nStack Size: 3/10\n");
        let groups = v["price"]["groups"].as_array().unwrap();
        let all = groups[0]["rows"].as_array().unwrap();
        assert_eq!(all.len(), 2);
        // most valuable first
        assert_eq!(all[0]["name"], "Divine Orb");
        assert_eq!(all[0]["current"], true);
        assert_eq!(all[1]["name"], "Chaos Orb");
    }

    #[test]
    fn filter_rows_keep_their_index_and_hide_hidden_disabled_filters() {
        use gold_core::price_check::filters::presets::{create_presets, PresetOptions};
        let data = GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")).unwrap();
        let text = "Item Class: Belts\nRarity: Rare\nGhoul Twine\nStudded Belt\n--------\nItem Level: 39\n--------\n\
                    { Prefix Modifier \"Rotund\" (Tier: 4) \u{2014} Life }\n+86(85-99) to maximum Life\n\
                    { Suffix Modifier \"of the Salamander\" (Tier: 7) \u{2014} Elemental, Fire, Resistance }\n+16(12-17)% to Fire Resistance\n";
        let item = parse_clipboard(&data, text).unwrap();
        let opts = PresetOptions {
            league: "Covens".into(),
            currency: None,
            collapse_listings: "api",
            activate_stock_filter: false,
        };
        let presets = create_presets(&data, &item, &opts).unwrap().presets;
        let stats = presets[0].stats.clone();
        let v = serde_json::to_value(render(&Session {
            epoch: 7,
            item,
            place: Placement {
                screen: "DP-1".into(),
                x: 0,
                y: 0,
            },
            presets,
            active: 0,
            ninja: None,
            search: SearchState::Idle,
            search_gen: 0,
            note: None,
            show_hidden: false,
            fetching_more: false,
            candidates: vec![],
        }))
        .unwrap();

        assert_eq!(v["epoch"], 7);
        let rows = v["filters"].as_array().unwrap();
        // every row's `index` points at the same filter in the full list the daemon edits
        for row in rows {
            let f = &stats[row["index"].as_u64().unwrap() as usize];
            assert!(
                f.hidden.is_none() || !f.disabled,
                "hidden and disabled filters must not be shown"
            );
        }
        let life = rows
            .iter()
            .find(|r| r["text"].as_str().unwrap().contains("total maximum Life"))
            .unwrap();
        assert_eq!(life["on"], true);
        assert_eq!(life["editable"], true);
    }

    #[test]
    fn same_seller_and_price_collapse_into_one_counted_row() {
        let row = |who: &str, price: &str| Row {
            price: price.into(),
            detail: String::new(),
            status: "online",
            who: who.into(),
            mine: false,
        };
        let rows = [
            row("a", "1 divine"),
            row("b", "1 divine"),
            row("a", "1 divine"),
            row("a", "2 divine"),
        ];
        let g = grouped_rows(&rows);
        let counts: Vec<_> = g.iter().map(|r| (r.who.as_str(), r.count)).collect();
        assert_eq!(counts, [("a", 2), ("b", 1), ("a", 1)]);
    }

    #[test]
    fn chaos_orb_is_one_chaos() {
        let v = view_of("Item Class: Stackable Currency\nRarity: Currency\nChaos Orb\n--------\nStack Size: 5/20\n");
        assert_eq!(v["price"]["amount"], "1");
        assert_eq!(v["price"]["unit"], "c");
    }

    #[test]
    fn divine_orb_shows_its_chaos_value() {
        let v = view_of("Item Class: Stackable Currency\nRarity: Currency\nDivine Orb\n--------\nStack Size: 1/10\n");
        assert_eq!(v["price"]["unit"], "c");
        assert_eq!(v["price"]["amount"], "151");
    }
}
