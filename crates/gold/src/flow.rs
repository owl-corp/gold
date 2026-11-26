//! The price check from copy to results, and the overlay's actions.
use crate::app::App;
use crate::session::{SearchState, Session};
use crate::ui::{Action, TradeKey, Ui};
use crate::view::{self, Placement, View};
use crate::{clipboard, hypr, search};
use anyhow::{bail, Result};
use gold_core::parser::item::{ItemRarity, ParsedItem};
use gold_core::parser::meta::{known, ItemCategory};
use gold_core::parser::parse_clipboard;
use gold_core::price_check::filters::presets::{create_presets, FilterPreset, Unsupported};
use gold_core::price_check::filters::ItemFilters;
use gold_core::price_check::trade::category_to_trade_id;
use std::sync::Arc;

/// Runs `f` on the current session if it is still the one for `epoch`.
fn update<R>(app: &App, epoch: u64, f: impl FnOnce(&mut Session) -> R) -> Option<R> {
    let mut guard = app.session.lock();
    guard.as_mut().filter(|s| s.epoch == epoch).map(f)
}

/// Shows the session in the overlay, unless the overlay moved on to something else.
/// The check and the send happen under the session lock, so a close or a newer check can't
/// slip in between and a stale view can't reach the overlay after a newer one.
fn show(app: &App, ui: &Ui, epoch: u64) {
    let guard = app.session.lock();
    if let Some(s) = guard.as_ref().filter(|s| s.epoch == epoch && app.is_current(epoch)) {
        ui.show(&view::render(s));
    }
}

/// Shows a view that isn't part of a session, if `epoch` is still current.
fn show_standalone(app: &App, ui: &Ui, epoch: u64, view: &View) {
    let _guard = app.session.lock();
    if app.is_current(epoch) {
        ui.show(view);
    }
}

/// Hides the overlay and invalidates everything still in flight for it.
/// Returns whether there was an overlay to close.
pub fn close(app: &App, ui: &Ui) -> bool {
    let guard = app.session.lock();
    app.next_epoch();
    ui.hide();
    guard.is_some()
}

/// The overlay closed itself (Esc): invalidate everything still in flight for it.
pub fn closed_by_overlay(app: &App) {
    let _guard = app.session.lock();
    app.next_epoch();
}

pub async fn price_check(app: &Arc<App>, ui: &Ui) -> Result<()> {
    if ui.clients() == 0 {
        bail!("no overlay connected (is the Quickshell overlay running?)");
    }
    if hypr::active_class().await? != hypr::POE_CLASS {
        bail!("Path of Exile is not the focused window");
    }
    // The epoch moves only once the copy worked: a second press while one runs must not
    // invalidate the overlay already showing.
    let text = match clipboard::copy_item().await {
        Ok(text) => text,
        Err(e) if e.is::<clipboard::Busy>() => return Err(e),
        Err(e) => {
            // the keybind hides stdout, so say what went wrong on screen too
            if let Ok(place) = placement().await {
                let epoch = app.next_epoch();
                show_standalone(
                    app,
                    ui,
                    epoch,
                    &view::error_view(&place, "Copy failed", vec![format!("{e:#}")]),
                );
            }
            return Err(e);
        }
    };
    let epoch = app.next_epoch();
    let place = placement().await?;

    let item = match parse_clipboard(&app.data, &text) {
        Ok(item) => item,
        Err(e) => {
            let mut lines = vec![e.code().to_string()];
            lines.extend(text.lines().take(12).map(String::from));
            show_standalone(app, ui, epoch, &view::error_view(&place, "Item not recognised", lines));
            return Ok(());
        }
    };

    let (presets, active, note) = match app.league().await {
        Err(e) => (vec![], 0, Some(format!("No league: {e:#}"))),
        Ok(league) => {
            let opts = app.preset_options(league);
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

    // Currency-exchange items are priced by poe.ninja only: no presets means no search at all.
    let (presets, active) = if item.info.exchangeable == Some(true) {
        (vec![], 0)
    } else {
        (presets, active)
    };
    let auto = presets.get(active).is_some_and(|p| search_on_open(&item, p));
    {
        let mut guard = app.session.lock();
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
            show_hidden: false,
            fetching_more: false,
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

/// Where the overlay goes: on the monitor under the cursor, at the cursor.
async fn placement() -> Result<Placement> {
    let cursor = hypr::cursor_pos().await?;
    let (screen, x, y) = hypr::locate(&hypr::monitors().await?, &cursor).unwrap_or((String::new(), cursor.x, cursor.y));
    Ok(Placement { screen, x, y })
}

/// Whether the search starts by itself when the overlay opens (`smartInitialSearch`).
fn search_on_open(item: &ParsedItem, preset: &FilterPreset) -> bool {
    use ItemCategory::{Charm, HeistBlueprint, HeistContract, Idol, SanctumRelic};
    let cat = known(&item.category);
    item.rarity == Some(ItemRarity::Unique)
        || preset.id == "filters.preset_bulk"
        || item.map.completion_reward.is_some()
        || matches!(cat, Some(HeistContract | HeistBlueprint | SanctumRelic | Charm | Idol))
        || (cat.and_then(category_to_trade_id).is_none() && item.info.ref_name != "Mercenary Warrant")
        || item.status.is_unidentified
        || item.mod_marks.is_veiled
}

fn start_search(app: &Arc<App>, ui: &Ui, epoch: u64) {
    let snapshot = {
        let mut guard = app.session.lock();
        let Some(s) = guard.as_mut().filter(|s| s.epoch == epoch) else {
            return;
        };
        let Some(preset) = s.preset().cloned() else { return };
        if matches!(s.search, SearchState::Loading) {
            return;
        }
        s.search = SearchState::Loading;
        s.search_gen += 1;
        let snapshot = (s.item.clone(), preset, s.search_gen);
        drop(guard);
        snapshot
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

/// Flips or cycles one trade filter. Returns whether it changed.
fn apply_trade_filter(f: &mut ItemFilters, key: TradeKey) -> bool {
    match key {
        TradeKey::MagicOnly => match &mut f.rarity {
            Some(r) if r.value == "magic" => {
                r.disabled = !r.disabled;
                true
            }
            _ => false,
        },
        TradeKey::ItemLevel => f.item_level.as_mut().map(|l| l.disabled = !l.disabled).is_some(),
        TradeKey::AnyBase => f
            .search_relaxed
            .as_mut()
            .map(|r| r.disabled = Some(r.disabled != Some(true)))
            .is_some(),
        TradeKey::Influence(i) => f
            .influences
            .as_mut()
            .and_then(|v| v.get_mut(i))
            .map(|inf| inf.disabled = !inf.disabled)
            .is_some(),
    }
}

/// Fetches the next listings of the first trade search and appends them.
fn more(app: &Arc<App>, ui: &Ui, epoch: u64) {
    let snapshot = {
        let mut guard = app.session.lock();
        let Some(s) = guard.as_mut().filter(|s| s.epoch == epoch) else {
            return;
        };
        if s.fetching_more {
            return;
        }
        let SearchState::Done(groups) = &s.search else { return };
        let Some((id, ids)) = groups.iter().find_map(|g| {
            let id = g.fetch_id.clone()?;
            (!g.pending.is_empty()).then(|| {
                (
                    id,
                    g.pending.iter().take(search::MORE_ROWS).cloned().collect::<Vec<_>>(),
                )
            })
        }) else {
            return;
        };
        s.fetching_more = true;
        let snapshot = (id, ids, s.search_gen);
        drop(guard);
        snapshot
    };
    show(app, ui, epoch);

    let (app, ui) = (app.clone(), ui.clone());
    tokio::spawn(async move {
        let (id, ids, generation) = snapshot;
        let result = search::fetch_rows(&app, &id, &ids).await;
        update(&app, epoch, |s| {
            // filters changed or a new search started meanwhile: this result belongs to the old one
            if s.search_gen != generation {
                return;
            }
            s.fetching_more = false;
            match result {
                Ok(rows) => {
                    if let SearchState::Done(groups) = &mut s.search {
                        if let Some(g) = groups.iter_mut().find(|g| g.fetch_id.as_deref() == Some(&id)) {
                            g.pending.drain(..ids.len().min(g.pending.len()));
                            g.rows.extend(rows);
                        }
                    }
                }
                Err(e) => s.search = SearchState::Failed(format!("{e:#}")),
            }
        });
        show(&app, &ui, epoch);
    });
}

/// An action from the overlay: toggle or edit a filter, switch preset, search.
pub fn handle_action(app: &Arc<App>, ui: &Ui, action: &Action) {
    // actions carry the epoch of the view they were clicked in; a click on a view that
    // has since been replaced must not touch the new item's filters
    let Some(epoch) = action.epoch() else { return };
    let matches_session = app.session.lock().as_ref().is_some_and(|s| s.epoch == epoch);
    if !matches_session || !app.is_current(epoch) {
        return;
    }

    match action {
        Action::Closed => {}
        Action::Toggle { index, .. } => {
            update(app, epoch, |s| {
                if let Some(f) = s.preset_mut().and_then(|p| p.stats.get_mut(*index)) {
                    f.disabled = !f.disabled;
                    s.search = SearchState::Idle;
                    s.search_gen += 1;
                }
            });
            show(app, ui, epoch);
        }
        Action::Set { index, min, max, .. } => {
            update(app, epoch, |s| {
                if let Some(f) = s.preset_mut().and_then(|p| p.stats.get_mut(*index)) {
                    if let Some(roll) = f.roll.as_mut() {
                        roll.min = *min;
                        roll.max = *max;
                        f.disabled = false;
                        s.search = SearchState::Idle;
                        s.search_gen += 1;
                    }
                }
            });
            show(app, ui, epoch);
        }
        Action::Preset { index, .. } => {
            update(app, epoch, |s| {
                if *index < s.presets.len() {
                    s.active = *index;
                    s.search = SearchState::Idle;
                    s.search_gen += 1;
                }
            });
            show(app, ui, epoch);
        }
        Action::Search { .. } => start_search(app, ui, epoch),
        Action::Hidden { .. } => {
            update(app, epoch, |s| s.show_hidden = !s.show_hidden);
            show(app, ui, epoch);
        }
        Action::More { .. } => more(app, ui, epoch),
        Action::Trade { key, .. } => {
            // a changed trade filter re-runs the search, if the user had run one
            let rerun = update(app, epoch, |s| {
                let had_search = !matches!(s.search, SearchState::Idle);
                let changed = s.preset_mut().is_some_and(|p| apply_trade_filter(&mut p.filters, *key));
                if changed {
                    s.search = SearchState::Idle;
                    s.search_gen += 1;
                    s.fetching_more = false;
                }
                changed && had_search
            });
            show(app, ui, epoch);
            if rerun == Some(true) {
                start_search(app, ui, epoch);
            }
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "tests may panic on failure")]
mod tests {
    use super::*;
    use gold_core::data::GameData;
    use gold_core::price_check::filters::presets::PresetOptions;

    fn data() -> GameData {
        GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")).unwrap()
    }

    /// Whether the search starts by itself for this item text.
    fn auto_searches(text: &str) -> bool {
        let data = data();
        let item = parse_clipboard(&data, text).unwrap();
        let opts = PresetOptions {
            league: "Covens".into(),
            currency: None,
            collapse_listings: "api",
            activate_stock_filter: false,
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
