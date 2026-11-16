//! State shared by every command: game data, HTTP client, league and cached prices.
use crate::config::Config;
use crate::session::Session;
use anyhow::{Context, Result};
use gold_core::data::GameData;
use gold_core::leagues;
use gold_core::price_check::filters::presets::PresetOptions;
use gold_core::price_check::ninja::NinjaDb;
use gold_core::price_check::trade::client::TradeClient;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct App {
    pub data: Arc<GameData>,
    pub client: gold_core::http::Client,
    pub trade: TradeClient,
    pub session: std::sync::Mutex<Option<Session>>,
    config: Config,
    league: Mutex<Option<String>>,
    ninja: Mutex<Option<Arc<NinjaDb>>>,
    /// Bumped whenever the overlay is replaced or closed, so late results are dropped.
    epoch: AtomicU64,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        let dir = config.data_dir();
        let data = GameData::load_with_extra(&dir, crate::config::extra_data_dir().as_deref(), "en")
            .with_context(|| format!("loading game data from {}", dir.display()))?;
        let contact = config.contact.clone().unwrap_or_else(|| "unset".into());
        let client = gold_core::http::client(&contact, config.poesessid.as_deref())?;
        let trade = TradeClient::new(client.clone(), std::time::Duration::from_secs(2));
        Ok(App {
            data: Arc::new(data),
            client,
            trade,
            session: std::sync::Mutex::new(None),
            config,
            league: Mutex::new(None),
            ninja: Mutex::new(None),
            epoch: AtomicU64::new(0),
        })
    }

    /// Starts a new overlay generation and returns its id.
    pub fn next_epoch(&self) -> u64 {
        self.epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn is_current(&self, epoch: u64) -> bool {
        self.epoch.load(Ordering::SeqCst) == epoch
    }

    pub async fn league(&self) -> Result<String> {
        let mut cached = self.league.lock().await;
        if let Some(l) = cached.as_ref() {
            return Ok(l.clone());
        }
        let league = match &self.config.league {
            Some(l) => l.clone(),
            None => {
                let all = leagues::load(&self.client).await?;
                leagues::default_league(&all).context("no trade leagues")?.to_string()
            }
        };
        *cached = Some(league.clone());
        Ok(league)
    }

    /// Your trade account name, to mark your own listings.
    pub fn account_name(&self) -> Option<&str> {
        self.config.account_name.as_deref()
    }

    /// Search defaults from the config.
    pub fn preset_options(&self, league: String) -> PresetOptions {
        PresetOptions {
            league,
            currency: self.config.default_currency.clone(),
            collapse_listings: if self.config.collapse_listings.as_deref() == Some("app") {
                "app"
            } else {
                "api"
            },
            activate_stock_filter: self.config.activate_stock_filter,
        }
    }

    /// poe.ninja prices for the league, refreshed when they get old.
    pub async fn ninja(&self) -> Result<Arc<NinjaDb>> {
        let league = self.league().await?;
        let mut cached = self.ninja.lock().await;
        if let Some(db) = cached.as_ref().filter(|db| db.league() == league && !db.is_stale()) {
            return Ok(db.clone());
        }
        let db = Arc::new(NinjaDb::fetch(&self.client, &league).await?);
        *cached = Some(db.clone());
        Ok(db)
    }
}
