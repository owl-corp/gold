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
fn show(app: &App, ui: &Ui, epoch: u64) {
    let rendered = {
        let guard = app.session.lock().unwrap();
        guard.as_ref().filter(|s| s.epoch == epoch).map(view::render)
    };
    if let Some(v) = rendered {
        if app.is_current(epoch) {
            ui.show(v);
        }
    }
}

pub async fn price_check(app: &Arc<App>, ui: &Ui) -> Result<()> {
    if ui.clients() == 0 {
        bail!("no overlay connected (is the Quickshell overlay running?)");
    }
    if hypr::active_class().await? != hypr::POE_CLASS {
        bail!("Path of Exile is not the focused window");
    }
    let epoch = app.next_epoch();
    let text = clipboard::copy_item().await?;

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
            ui.show(view::error_view(&place, "Item not recognised", lines));
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
    *app.session.lock().unwrap() = Some(Session {
        epoch,
        item,
        place,
        presets,
        active,
        ninja: None,
        search: SearchState::Idle,
        note,
    });
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
        s.search = SearchState::Loading;
        (s.item.clone(), preset)
    };
    show(app, ui, epoch);

    let (app, ui) = (app.clone(), ui.clone());
    tokio::spawn(async move {
        let (item, preset) = snapshot;
        let result = search::run(&app, &item, &preset).await;
        update(&app, epoch, |s| {
            s.search = match result {
                Ok(groups) => SearchState::Done(groups),
                Err(e) => SearchState::Failed(format!("{e:#}")),
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
    let Some(epoch) = app.session.lock().unwrap().as_ref().map(|s| s.epoch) else {
        return;
    };
    if !app.is_current(epoch) {
        return;
    }
    let index = action["index"].as_u64().map(|i| i as usize);

    match action["type"].as_str().unwrap_or("") {
        "toggle" => {
            update(app, epoch, |s| {
                if let Some(f) = s.preset_mut().and_then(|p| index.and_then(|i| p.stats.get_mut(i))) {
                    f.disabled = !f.disabled;
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
                }
            });
            show(app, ui, epoch);
        }
        "search" => start_search(app, ui, epoch),
        _ => {}
    }
}
