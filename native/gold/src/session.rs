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
    /// listings grouped by what they are priced in, for bulk searches
    pub heading: Option<String>,
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
    /// Why there are no filters, e.g. an item kind that is not supported yet.
    pub note: Option<String>,
}

impl Session {
    pub fn preset(&self) -> Option<&FilterPreset> {
        self.presets.get(self.active)
    }

    pub fn preset_mut(&mut self) -> Option<&mut FilterPreset> {
        self.presets.get_mut(self.active)
    }
}
