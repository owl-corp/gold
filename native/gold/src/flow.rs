//! The price check from copy to results, and the overlay's actions.
use crate::app::App;
use crate::session::{SearchState, Session};
use crate::ui::Ui;
use crate::view::{self, Placement};
use crate::{clipboard, hypr, search};
use anyhow::{bail, Result};
use gold_core::parser::item::{ItemRarity, ParsedItem};
use gold_core::parser::meta::{known, ItemCategory};
use gold_core::parser::parse_clipboard;
use gold_core::price_check::filters::presets::{create_presets, FilterPreset, PresetOptions, Unsupported};
use gold_core::price_check::trade::category_to_trade_id;
use serde_json::Value;
use std::sync::Arc;

/// Runs `f` on the current session if it is still the one for `epoch`.
fn update<R>(app: &App, epoch: u64, f: impl FnOnce(&mut Session) -> R) -> Option<R> {
    let mut guard = app.session.lock().unwrap();
    guard.as_mut().filter(|s| s.epoch == epoch).map(f)
}

/// Shows the session in the overlay, unless the overlay moved on to something else.
/// The check and the send happen under the session lock, so a close or a newer check can't
/// slip in between and a stale view can't reach the overlay after a newer one.
fn show(app: &App, ui: &Ui, epoch: u64) {
    let guard = app.session.lock().unwrap();
    if let Some(s) = guard.as_ref().filter(|s| s.epoch == epoch && app.is_current(epoch)) {
        ui.show(view::render(s));
    }
}

/// Shows a view that isn't part of a session, if `epoch` is still current.
fn show_standalone(app: &App, ui: &Ui, epoch: u64, view: Value) {
    let _guard = app.session.lock().unwrap();
    if app.is_current(epoch) {
        ui.show(view);
    }
}

/// Hides the overlay and invalidates everything still in flight for it.
/// Returns whether there was an overlay to close.
pub fn close(app: &App, ui: &Ui) -> bool {
    let guard = app.session.lock().unwrap();
    app.next_epoch();
    ui.hide();
    guard.is_some()
}

/// The overlay closed itself (Esc): invalidate everything still in flight for it.
pub fn closed_by_overlay(app: &App) {
    let _guard = app.session.lock().unwrap();
    app.next_epoch();
}

pub async fn price_check(app: &Arc<App>, ui: &Ui) -> Result<()> {
    if ui.clients() == 0 {
        bail!("no overlay connected (is the Quickshell overlay running?)");
    }
    if hypr::active_class().await? != hypr::POE_CLASS {
        bail!("Path of Exile is not the focused window");
    }
    // The epoch moves only once the copy worked: a copy that fails or is ignored
    // (a second press while one runs) must not invalidate the overlay already showing.
    let text = clipboard::copy_item().await?;
    let epoch = app.next_epoch();

    let cursor = hypr::cursor_pos().await?;
    let monitor = hypr::monitors().await?.into_iter().find(|m| m.focused);
    let (mx, my, screen) = monitor.map(|m| (m.x, m.y, m.name)).unwrap_or((0, 0, String::new()));
    let place = Placement {
        screen,
        x: cursor.x - mx,
        y: cursor.y - my,
    };

    let item = match parse_clipboard(&app.data, &text) {
        Ok(item) => item,
        Err(e) => {
            let mut lines = vec![e.code().to_string()];
            lines.extend(text.lines().take(12).map(String::from));
            show_standalone(app, ui, epoch, view::error_view(&place, "Item not recognised", lines));
            return Ok(());
        }
    };

    let (presets, active, note) = match app.league().await {
        Err(e) => (vec![], 0, Some(format!("No league: {e:#}"))),
        Ok(league) => {
            let opts = PresetOptions {
                league: league.clone(),
                merchant_only: false,
                currency: None,
                collapse_listings: "api",
                activate_stock_filter: false,
                search_stat_range: 10.0,
                use_en: false,
            };
            match create_presets(&app.data, &item, &opts) {
                Ok(p) => {
                    let active = p.presets.iter().position(|x| x.id == p.active).unwrap_or(0);
                    (p.presets, active, None)
                }
                Err(e) if e.is::<Unsupported>() => (vec![], 0, Some(e.to_string())),
                Err(e) => (vec![], 0, Some(format!("Can't build a search: {e:#}"))),
            }
        }
    };

    let auto = presets.get(active).is_some_and(|p| search_on_open(&item, p));
    {
        let mut guard = app.session.lock().unwrap();
        if !app.is_current(epoch) {
            // closed, or replaced by a newer check, while this one was still working
            return Ok(());
        }
        *guard = Some(Session {
            epoch,
            item,
            place,
            presets,
            active,
            ninja: None,
            search: SearchState::Idle,
            search_gen: 0,
            note,
        });
    }
    show(app, ui, epoch);

    let (a, u) = (app.clone(), ui.clone());
    tokio::spawn(async move {
        let ninja = a.ninja().await.map_err(|e| format!("{e:#}"));
        update(&a, epoch, |s| s.ninja = Some(ninja));
        show(&a, &u, epoch);
    });

    if auto {
        start_search(app, ui, epoch);
    }
    Ok(())
}

/// Whether the search starts by itself when the overlay opens (`smartInitialSearch`).
fn search_on_open(item: &ParsedItem, preset: &FilterPreset) -> bool {
    use ItemCategory::*;
    let cat = known(&item.category);
    item.rarity == Some(ItemRarity::Unique)
        || preset.id == "filters.preset_bulk"
        || item.map_completion_reward.is_some()
        || matches!(cat, Some(HeistContract | HeistBlueprint | SanctumRelic | Charm | Idol))
        || (cat.and_then(category_to_trade_id).is_none() && item.info.ref_name != "Mercenary Warrant")
        || item.is_unidentified
        || item.is_veiled == Some(true)
}

fn start_search(app: &Arc<App>, ui: &Ui, epoch: u64) {
    let snapshot = {
        let mut guard = app.session.lock().unwrap();
        let Some(s) = guard.as_mut().filter(|s| s.epoch == epoch) else {
            return;
        };
        let Some(preset) = s.preset().cloned() else { return };
        if matches!(s.search, SearchState::Loading) {
            return;
        }
        s.search = SearchState::Loading;
        s.search_gen += 1;
        (s.item.clone(), preset, s.search_gen)
    };
    show(app, ui, epoch);

    let (app, ui) = (app.clone(), ui.clone());
    tokio::spawn(async move {
        let (item, preset, generation) = snapshot;
        let result = search::run(&app, &item, &preset).await;
        // only the latest search for the current filters may write its result
        update(&app, epoch, |s| {
            if s.search_gen == generation {
                s.search = match result {
                    Ok(groups) => SearchState::Done(groups),
                    Err(e) => SearchState::Failed(format!("{e:#}")),
                }
            }
        });
        show(&app, &ui, epoch);
    });
}

fn number(v: &Value) -> Option<f64> {
    v.as_f64()
}

/// An action from the overlay: toggle or edit a filter, switch preset, search.
pub async fn handle_action(app: &Arc<App>, ui: &Ui, action: Value) {
    // actions carry the epoch of the view they were clicked in; a click on a view that
    // has since been replaced must not touch the new item's filters
    let Some(epoch) = action["epoch"].as_u64() else { return };
    let matches_session = app.session.lock().unwrap().as_ref().is_some_and(|s| s.epoch == epoch);
    if !matches_session || !app.is_current(epoch) {
        return;
    }
    let index = action["index"].as_u64().map(|i| i as usize);

    match action["type"].as_str().unwrap_or("") {
        "toggle" => {
            update(app, epoch, |s| {
                if let Some(f) = s.preset_mut().and_then(|p| index.and_then(|i| p.stats.get_mut(i))) {
                    f.disabled = !f.disabled;
                    s.search = SearchState::Idle;
                    s.search_gen += 1;
                }
            });
            show(app, ui, epoch);
        }
        "set" => {
            update(app, epoch, |s| {
                if let Some(f) = s.preset_mut().and_then(|p| index.and_then(|i| p.stats.get_mut(i))) {
                    if let Some(roll) = f.roll.as_mut() {
                        roll.min = number(&action["min"]);
                        roll.max = number(&action["max"]);
                        f.disabled = false;
                        s.search = SearchState::Idle;
                        s.search_gen += 1;
                    }
                }
            });
            show(app, ui, epoch);
        }
        "preset" => {
            update(app, epoch, |s| {
                if let Some(i) = index.filter(|i| *i < s.presets.len()) {
                    s.active = i;
                    s.search = SearchState::Idle;
                    s.search_gen += 1;
                }
            });
            show(app, ui, epoch);
        }
        "search" => start_search(app, ui, epoch),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gold_core::data::GameData;

    fn data() -> GameData {
        GameData::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"),
            "en",
        )
        .unwrap()
    }

    /// Whether the search starts by itself for this item text.
    fn auto_searches(text: &str) -> bool {
        let data = data();
        let item = parse_clipboard(&data, text).unwrap();
        let opts = PresetOptions {
            league: "Covens".into(),
            merchant_only: false,
            currency: None,
            collapse_listings: "api",
            activate_stock_filter: false,
            search_stat_range: 10.0,
            use_en: false,
        };
        let presets = create_presets(&data, &item, &opts).unwrap();
        let active = presets.presets.iter().find(|p| p.id == presets.active).unwrap();
        search_on_open(&item, active)
    }

    #[test]
    fn uniques_and_currency_search_on_open_but_rares_wait_for_the_button() {
        let currency = "Item Class: Stackable Currency\nRarity: Currency\nDivine Orb\n--------\nStack Size: 3/10\n";
        let unique = "Item Class: Belts\nRarity: Unique\nHeadhunter\nLeather Belt\n--------\nItem Level: 84\n";
        let rare = "Item Class: Belts\nRarity: Rare\nGhoul Twine\nStudded Belt\n--------\nItem Level: 39\n--------\n\
                    { Prefix Modifier \"Rotund\" (Tier: 4) \u{2014} Life }\n+86(85-99) to maximum Life\n";
        assert!(auto_searches(currency));
        assert!(auto_searches(unique));
        assert!(!auto_searches(rare));
    }
}
