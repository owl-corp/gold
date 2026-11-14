//! One price check: the item, its filters, and the state of its searches.
//! The overlay only renders `view::render(session)` and sends actions back.
use crate::view::Placement;
use gold_core::parser::item::ParsedItem;
use gold_core::price_check::filters::presets::FilterPreset;
use gold_core::price_check::ninja::NinjaDb;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Row {
    pub price: String,
    pub detail: String,
    pub status: &'static str,
    pub who: String,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub total: u64,
    pub rows: Vec<Row>,
    pub url: String,
    /// e.g. "in divine"; set only for bulk exchange searches
    pub heading: Option<String>,
    /// For trade searches: the query id and the result ids not fetched yet ("show more").
    pub fetch_id: Option<String>,
    pub pending: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum SearchState {
    /// The user has not asked for a search yet (rares start like this).
    Idle,
    Loading,
    Done(Vec<Outcome>),
    Failed(String),
}

pub struct Session {
    pub epoch: u64,
    pub item: ParsedItem,
    pub place: Placement,
    pub presets: Vec<FilterPreset>,
    pub active: usize,
    pub ninja: Option<Result<Arc<NinjaDb>, String>>,
    pub search: SearchState,
    /// Bumped whenever the filters change or a search starts, so a late result can tell it is stale.
    pub search_gen: u64,
    /// Why there are no filters, e.g. an item kind that is not supported yet.
    pub note: Option<String>,
    /// Show the hidden filters that are switched off.
    pub show_hidden: bool,
    /// A "show more" fetch is running.
    pub fetching_more: bool,
}

impl Session {
    pub fn preset(&self) -> Option<&FilterPreset> {
        self.presets.get(self.active)
    }

    pub fn preset_mut(&mut self) -> Option<&mut FilterPreset> {
        self.presets.get_mut(self.active)
    }
}
