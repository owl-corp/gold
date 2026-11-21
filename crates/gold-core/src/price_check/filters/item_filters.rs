//! Item-level search filters.
use super::{
    Corrupted, DisabledOnly, FilterNumeric, HideableToggle, InfluenceFilter, ItemFilters, MapCompletionReward, Rarity,
    Search, TradeFilters, Unidentified, Value, Veiled,
};
use crate::data::{BaseType, GameData};
use crate::leagues::PERMANENT_SC;
use crate::parser::calc_q20::get_prop_quality;
use crate::parser::item::{ItemRarity, ParsedItem};
use crate::parser::meta::{is, is_jewellery, known, ItemCategory};
use crate::parser::modifiers::ModifierType;
use crate::price_check::trade::category_to_trade_id;
use crate::price_check::{ceil_to_bracket, floor_to_bracket};

#[derive(Clone, Debug)]
pub struct CreateOptions {
    pub league: String,
    pub currency: Option<String>,
    /// "app" or "api"
    pub collapse_listings: &'static str,
    pub activate_stock_filter: bool,
    pub exact: bool,
}

fn t(_opts: &CreateOptions, info: &BaseType) -> String {
    info.name.clone()
}

fn cat(item: &ParsedItem) -> Option<ItemCategory> {
    known(&item.category)
}

/// Categories whose items are "consumable" crafting bases rather than equipment.
const fn is_consumable_craftable(c: Option<ItemCategory>) -> bool {
    use ItemCategory::{Chart, ExpeditionLogbook, HeistBlueprint, HeistContract, Invitation, Map, MemoryLine};
    matches!(
        c,
        Some(Map | HeistBlueprint | HeistContract | Invitation | MemoryLine | ExpeditionLogbook | Chart)
    )
}

const fn is_magic_only_or_unique(c: Option<ItemCategory>) -> bool {
    use ItemCategory::{Charm, Flask, SanctumRelic, Tincture, Voidstone};
    matches!(c, Some(Flask | Tincture | Voidstone | SanctumRelic | Charm))
}

fn exact_search(info: &BaseType, opts: &CreateOptions) -> Search {
    Search {
        base_type: Some(info.name.clone()),
        base_type_trade: Some(t(opts, info)),
        ..Default::default()
    }
}

fn area_sub(area: &BaseType, disc: &str) -> Box<Search> {
    Box::new(Search {
        base_type: Some(area.name.clone()),
        base_type_trade: area.trade_disc.clone(),
        discriminator_trade: Some(disc.to_string()),
        disabled: Some(false),
        ..Default::default()
    })
}

/// Search and level filters of divination cards, currency and the like.
fn apply_currency_like(item: &ParsedItem, opts: &CreateOptions, filters: &mut ItemFilters) {
    let info = &item.info;
    filters.search_exact = exact_search(info, opts);
    let area_level = item.map.area_level;
    match info.ref_name.as_str() {
        "Chronicle of Atzoatl" => {
            filters.area_level =
                area_level.map(|l| FilterNumeric::new(floor_to_bracket(l, &[1, 68, 73, 75, 78, 80]), false));
        }
        "Forbidden Tome" => {
            filters.area_level = area_level.map(|l| {
                let mut f = FilterNumeric::new(l, false);
                if l < 83 {
                    f.max = Some(l);
                }
                f
            });
        }
        _ => {
            // Incubators, Wombgifts
            if let Some(ilvl) = item.item_level.filter(|l| *l != 0) {
                filters.item_level = Some(FilterNumeric::new(ilvl, false));
            }
        }
    }
}

/// Search and filters of a map.
fn apply_map_search(
    data: &GameData,
    item: &ParsedItem,
    opts: &CreateOptions,
    filters: &mut ItemFilters,
) -> anyhow::Result<()> {
    let info = &item.info;
    let blighted = info
        .area
        .as_ref()
        .and_then(|a| a.get("blighted"))
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    if blighted {
        let map = data.items.by_ref("ITEM", "Map").and_then(|m| m.into_iter().next());
        let area = item
            .map
            .area
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("blighted map without area"))?;
        filters.search_exact = Search {
            base_type: Some(info.name.clone()),
            base_type_trade: map.as_ref().map(|m| t(opts, m)),
            discriminator_trade: Some("map".into()),
            sub: Some(area_sub(area, info.trade_disc.as_deref().unwrap_or_default())),
            ..Default::default()
        };
    } else if item.rarity == Some(ItemRarity::Unique) && item.unique_base.is_some() {
        filters.search_exact = Search {
            name: Some(info.name.clone()),
            name_trade: Some(t(opts, info)),
            base_type_trade: item.unique_base.as_ref().map(|b| t(opts, b)),
            ..Default::default()
        };
    } else {
        filters.search_exact = exact_search(info, opts);
    }

    if info.ref_name == "Map" || item.unique_base.as_ref().is_some_and(|b| b.ref_name == "Map") {
        filters.search_exact.discriminator_trade = Some("map".into());
    }

    match info.ref_name.as_str() {
        "Blighted Map" => {
            filters.map_blighted = Some(Value {
                value: "Blighted".into(),
            });
        }
        "Blight-ravaged Map" => {
            filters.map_blighted = Some(Value {
                value: "Blight-ravaged".into(),
            });
        }
        "Map" => filters.map_blighted = Some(Value { value: false.into() }),
        _ => {}
    }

    if let Some(reward) = &item.map.completion_reward {
        filters.map_completion_reward = Some(MapCompletionReward {
            name: reward.name.clone(),
            name_trade: t(opts, reward),
        });
    }

    if let Some(tier) = item.map.tier.filter(|t| *t != 0) {
        filters.map_tier = Some(FilterNumeric::new(tier, false));
    }
    Ok(())
}

/// The search an item kind needs (maps, charts, uniques, plain bases).
fn apply_item_search(
    data: &GameData,
    item: &ParsedItem,
    opts: &CreateOptions,
    filters: &mut ItemFilters,
) -> anyhow::Result<()> {
    let info = &item.info;
    if is(&item.category, ItemCategory::Map) {
        apply_map_search(data, item, opts, filters)?;
    } else if info.ref_name == "Expedition Logbook" {
        filters.search_exact = exact_search(info, opts);
        filters.area_level = item
            .map
            .area_level
            .map(|l| FilterNumeric::new(floor_to_bracket(l, &[1, 68, 73, 78, 81, 83]), false));
    } else if is(&item.category, ItemCategory::Chart) {
        let area = item
            .map
            .area
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("chart without area"))?;
        filters.search_exact = exact_search(info, opts);
        filters.search_relaxed = Some(Search {
            category: item.category.clone(),
            disabled: Some(false),
            sub: Some(area_sub(area, info.trade_disc.as_deref().unwrap_or_default())),
            ..Default::default()
        });
    } else if item.rarity == Some(ItemRarity::Unique) && item.unique_base.is_some() {
        filters.search_exact = Search {
            name: Some(info.name.clone()),
            name_trade: Some(t(opts, info)),
            base_type_trade: item.unique_base.as_ref().map(|b| t(opts, b)),
            ..Default::default()
        };
    } else {
        filters.search_exact = exact_search(info, opts);
        if let Some(c) = cat(item).filter(|c| category_to_trade_id(*c).is_some()) {
            use ItemCategory::{Charm, ClusterJewel, Graft, HeistBlueprint, HeistContract, Idol, SanctumRelic};
            let disabled = match c {
                ClusterJewel | Idol | Graft | HeistBlueprint => true,
                SanctumRelic | Charm | HeistContract => false,
                _ => opts.exact,
            };
            filters.search_relaxed = Some(Search {
                category: item.category.clone(),
                disabled: Some(disabled),
                ..Default::default()
            });
        }
    }

    Ok(())
}

/// Quality, sockets, corruption and rarity filters.
fn apply_quality_corruption_rarity(item: &ParsedItem, opts: &CreateOptions, filters: &mut ItemFilters) {
    let info = &item.info;
    if let Some(charge) = item.sentinel_charge {
        filters.sentinel_charge = Some(FilterNumeric::new(charge, false));
    }

    if let Some(quality) = item.quality.filter(|q| *q >= 20) {
        if !cat(item).is_some_and(is_jewellery) {
            let flask_like = is(&item.category, ItemCategory::Flask) || is(&item.category, ItemCategory::Tincture);
            if flask_like || get_prop_quality(item) == 0 || opts.exact {
                // for Weapons & Armour
                filters.quality = Some(FilterNumeric::new(quality, quality <= 20));
            }
        }
    }

    if let Some(linked) = item.sockets.as_ref().and_then(|s| s.linked) {
        filters.linked_sockets = Some(FilterNumeric::new(linked, false));
    }

    let for_adorned_jewel = item.rarity == Some(ItemRarity::Magic)
        // `item.isCorrupted` is not required: let the buyer corrupt
        && (is(&item.category, ItemCategory::Jewel) || is(&item.category, ItemCategory::AbyssJewel));

    if !item.status.is_unmodifiable && (info.craftable.is_some() || item.rarity == Some(ItemRarity::Unique)) {
        filters.corrupted = Some(Corrupted {
            value: item.status.is_corrupted,
            exact: Some(for_adorned_jewel),
        });
    }

    if for_adorned_jewel {
        filters.rarity = Some(Rarity {
            value: "magic",
            disabled: false,
        });
    } else if opts.exact
        && item.rarity == Some(ItemRarity::Magic)
        && !is_consumable_craftable(cat(item))
        && !is_magic_only_or_unique(cat(item))
    {
        filters.rarity = Some(Rarity {
            value: "magic",
            disabled: true,
        });
    } else if info.craftable.is_some() && item.rarity == Some(ItemRarity::Magic) {
        // a magic item with the wanted mods is worth more than a full rare with them: search magic only
        filters.rarity = Some(Rarity {
            value: "magic",
            disabled: false,
        });
    } else if info.craftable.is_some() && matches!(item.rarity, Some(ItemRarity::Normal | ItemRarity::Rare)) {
        filters.rarity = Some(Rarity {
            value: "nonunique",
            disabled: false,
        });
    }
}

/// Mirrored, split, fractured, foil and influence filters.
fn apply_toggles(item: &ParsedItem, opts: &CreateOptions, filters: &mut ItemFilters) {
    let info = &item.info;
    if item.origin.is_mirrored {
        filters.mirrored = Some(HideableToggle {
            disabled: false,
            hidden: false,
        });
    } else if info.craftable.is_some() && !item.status.is_corrupted {
        filters.mirrored = Some(HideableToggle {
            disabled: true,
            hidden: true,
        });
    }

    if item.origin.is_split {
        filters.split = Some(HideableToggle {
            disabled: false,
            hidden: false,
        });
    } else if (!PERMANENT_SC.contains(&opts.league.as_str()) || opts.exact)
        && info.craftable.is_some()
        && !item.status.is_corrupted
        && !item.origin.is_mirrored
        && !item.origin.is_synthesised
        && !item.mod_marks.is_fractured
        && item.influences.is_empty()
    {
        filters.split = Some(HideableToggle {
            disabled: true,
            hidden: true,
        });
    }

    if !item.mod_marks.is_fractured
        && (info.craftable.is_some() && !item.status.is_corrupted && !item.origin.is_mirrored)
    {
        filters.fractured = Some(Value { value: false });
    }

    if item.looks.is_foil {
        filters.foil = Some(DisabledOnly { disabled: false });
    }

    if !item.influences.is_empty() && item.influences.len() <= 2 {
        filters.influences = Some(
            item.influences
                .iter()
                .map(|i| InfluenceFilter {
                    value: *i,
                    disabled: !opts.exact,
                })
                .collect(),
        );
    }
}

/// Item level filter.
fn apply_item_level(item: &ParsedItem, opts: &CreateOptions, filters: &mut ItemFilters) {
    let info = &item.info;
    if let Some(ilvl) = item.item_level.filter(|l| *l != 0) {
        use ItemCategory::{
            Charm, Chart, ClusterJewel, Flask, HeistBlueprint, HeistContract, Idol, Jewel, Map, MemoryLine,
            SanctumRelic, Tincture,
        };
        let excluded = matches!(
            cat(item),
            Some(Map | Jewel | HeistBlueprint | HeistContract | Chart | MemoryLine | SanctumRelic | Charm | Idol)
        ) || info.ref_name == "Expedition Logbook";
        if item.rarity != Some(ItemRarity::Unique) && !excluded {
            if is(&item.category, ClusterJewel) {
                filters.item_level = Some(FilterNumeric {
                    value: floor_to_bracket(ilvl, &[1, 50, 68, 75, 84]),
                    max: Some(ceil_to_bracket(ilvl, &[100, 74, 67, 49])),
                    disabled: !opts.exact,
                });
            } else {
                // TODO limit level by item type
                let always_off = is(&item.category, Flask) || is(&item.category, Tincture);
                filters.item_level = Some(FilterNumeric::new(ilvl.min(86), !opts.exact || always_off));
            }
        }

        if item.rarity == Some(ItemRarity::Unique) {
            if item.status.is_unidentified && info.ref_name == "Watcher's Eye" {
                filters.item_level = Some(FilterNumeric::new(ilvl, false));
            }

            if ilvl >= 75
                && [
                    "Agnerod",
                    "Agnerod East",
                    "Agnerod North",
                    "Agnerod South",
                    "Agnerod West",
                ]
                .contains(&info.ref_name.as_str())
            {
                // https://pathofexile.gamepedia.com/The_Vinktar_Square
                let normalized = if ilvl >= 82 {
                    82
                } else if ilvl >= 80 {
                    80
                } else if ilvl >= 78 {
                    78
                } else {
                    75
                };
                filters.item_level = Some(FilterNumeric::new(normalized, false));
            }
        }
    }
}

/// Filters with nothing set but the trade options.
fn blank_filters(opts: &CreateOptions) -> ItemFilters {
    ItemFilters {
        search_exact: Search::default(),
        search_relaxed: None,
        rarity: None,
        linked_sockets: None,
        white_sockets: None,
        corrupted: None,
        fractured: None,
        imbued_gem: None,
        mirrored: None,
        split: None,
        foil: None,
        foulborn: None,
        vestigial: None,
        influences: None,
        quality: None,
        gem_level: None,
        map_tier: None,
        map_blighted: None,
        map_completion_reward: None,
        item_level: None,
        stack_size: None,
        unidentified: None,
        veiled: None,
        area_level: None,
        sentinel_charge: None,
        trade: TradeFilters {
            currency: opts.currency.clone(),
            league: opts.league.clone(),
            collapse_listings: opts.collapse_listings,
            collapse_merchant: false,
        },
    }
}

pub fn create_filters(data: &GameData, item: &ParsedItem, opts: &CreateOptions) -> anyhow::Result<ItemFilters> {
    use ItemCategory::{Chart, HeistBlueprint, HeistContract};
    let info = &item.info;
    let mut filters = blank_filters(opts);

    if (info.craftable.is_none() || is_consumable_craftable(cat(item))) && item.rarity != Some(ItemRarity::Unique) {
        if opts.currency.is_none() {
            filters.trade.currency = Some("chaos_divine".into());
        }
        if info.ref_name != "Mercenary Warrant" {
            filters.trade.collapse_merchant = true;
        }
    }

    if is(&item.category, ItemCategory::Gem) {
        return create_gem_filters(data, item, filters, opts);
    }
    if is(&item.category, ItemCategory::CapturedBeast) {
        filters.search_exact = exact_search(info, opts);
        return Ok(filters);
    }

    if item.stack_size.is_some() || info.trade_tag.is_some() || info.exchangeable == Some(true) {
        let size = item
            .stack_size
            .as_ref()
            .and_then(|s| s.value)
            .filter(|v| *v != 0)
            .unwrap_or(1);
        let has_stack = item.stack_size.as_ref().is_some_and(|s| s.value.is_some_and(|v| v > 1));
        filters.stack_size = Some(FilterNumeric::new(size, !(has_stack && opts.activate_stock_filter)));
    }
    if is(&item.category, ItemCategory::Invitation) {
        filters.search_exact = exact_search(info, opts);
        return Ok(filters);
    }
    if is(&item.category, ItemCategory::MetamorphSample) {
        filters.search_exact = exact_search(info, opts);
        filters.item_level = item.item_level.map(|l| FilterNumeric::new(l, false));
        return Ok(filters);
    }
    if info.ref_name == "Scrying Orb" {
        let area = item
            .map
            .area
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("scrying orb without area"))?;
        let mut search = exact_search(info, opts);
        search.sub = Some(area_sub(area, info.trade_disc.as_deref().unwrap_or_default()));
        filters.search_exact = search;
        return Ok(filters);
    }
    if is(&item.category, ItemCategory::DivinationCard)
        || is(&item.category, ItemCategory::Currency)
        || info.ref_name == "Charged Compass"
    {
        apply_currency_like(item, opts, &mut filters);
        return Ok(filters);
    }

    apply_item_search(data, item, opts, &mut filters)?;
    apply_quality_corruption_rarity(item, opts, &mut filters);
    apply_toggles(item, opts, &mut filters);
    apply_item_level(item, opts, &mut filters);

    if item.status.is_unidentified {
        filters.unidentified = Some(Unidentified {
            value: true,
            disabled: item.rarity != Some(ItemRarity::Unique),
        });
    }

    if item.mod_marks.is_veiled {
        filters.veiled = Some(Veiled {
            stat_refs: item
                .stats_by_type
                .iter()
                .filter(|c| c.type_ == ModifierType::Veiled)
                .map(|c| c.stat.ref_.clone())
                .collect(),
            disabled: false,
        });

        if item.rarity != Some(ItemRarity::Unique) {
            if let Some(level) = filters.item_level.as_mut() {
                level.disabled = false;
            }
        }
    }

    if item.rarity == Some(ItemRarity::Unique) {
        filters.foulborn = Some(Value {
            value: item.mod_marks.is_foulborn,
        });
        filters.vestigial = Some(Value {
            value: item.looks.is_vestigial,
        });
    }

    if matches!(cat(item), Some(HeistContract | HeistBlueprint | Chart)) && item.rarity != Some(ItemRarity::Unique) {
        filters.area_level = item.map.area_level.map(|l| FilterNumeric::new(l, false));
    }

    Ok(filters)
}

fn create_gem_filters(
    data: &GameData,
    item: &ParsedItem,
    mut filters: ItemFilters,
    opts: &CreateOptions,
) -> anyhow::Result<ItemFilters> {
    let gem = item.info.gem.as_ref();
    let gem_flag = |k: &str| gem.and_then(|g| g.get(k)).and_then(serde_json::Value::as_bool) == Some(true);
    let max_level = gem.and_then(|g| g.get("maxLevel")).and_then(serde_json::Value::as_i64);
    let transfigured = gem_flag("transfigured");

    if transfigured {
        let normal = gem
            .and_then(|g| g.get("normalVariant"))
            .and_then(|v| v.as_str())
            .and_then(|r| data.items.by_ref("GEM", r))
            .and_then(|v| v.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("transfigured gem without normal variant"))?;
        filters.search_exact = Search {
            base_type: Some(item.info.name.clone()),
            base_type_trade: Some(t(opts, normal)),
            discriminator_trade: item.info.trade_disc.clone(),
            ..Default::default()
        };
    } else {
        filters.search_exact = exact_search(&item.info, opts);
    }

    if let Some(vaal) = &item.gem.vaal {
        filters.search_exact.sub = Some(Box::new(Search {
            base_type: Some(vaal.name.clone()),
            base_type_trade: Some(t(opts, vaal)),
            discriminator_trade: item.info.trade_disc.clone(),
            disabled: Some(false),
            ..Default::default()
        }));
    }

    filters.corrupted = Some(Corrupted {
        value: item.status.is_corrupted,
        exact: None,
    });

    if let Some(gem_level) = item.gem.level {
        if !item.gem.imbued && item.status.is_corrupted && gem_level >= 20 {
            filters.imbued_gem = Some(DisabledOnly { disabled: true });
        }

        filters.gem_level = Some(FilterNumeric::new(gem_level, max_level.is_some_and(|m| gem_level < m)));
    }

    if let Some(quality) = item.quality.filter(|q| *q != 0) {
        filters.quality = Some(if max_level == Some(1) {
            FilterNumeric::new(quality, false)
        } else if quality < 20 {
            // Below 20% search 2% lower so near-misses still match.
            FilterNumeric::new((quality - 2).max(0), false)
        } else {
            FilterNumeric::new(quality, false)
        });
    }

    Ok(filters)
}
