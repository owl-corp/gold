//! Search and fetch on the trade site.
use super::bulk::BulkRequest;
use super::limits::{prevent_queue_creation, Rules};
use super::request::TradeRequest;
use super::Account;
use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(300);
const TRADE_HOST: &str = "https://www.pathofexile.com";

#[derive(Clone, Debug, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub result: Vec<String>,
    pub total: u64,
    #[serde(default)]
    pub inexact: Option<bool>,
}

#[derive(Deserialize)]
struct ApiError {
    code: i64,
    message: String,
}

#[derive(Deserialize)]
struct Price {
    amount: f64,
    currency: String,
}

#[derive(Deserialize)]
struct Listing {
    indexed: String,
    #[serde(default)]
    price: Option<Price>,
    #[serde(default)]
    fee: Option<f64>,
    account: Account,
}

#[derive(Deserialize)]
struct Property {
    values: Vec<(String, f64)>,
    #[serde(rename = "type", default)]
    type_: Option<i64>,
}

#[derive(Deserialize)]
struct FetchedItem {
    #[serde(default)]
    ilvl: Option<f64>,
    #[serde(rename = "stackSize", default)]
    stack_size: Option<f64>,
    #[serde(default)]
    corrupted: Option<bool>,
    #[serde(default)]
    properties: Option<Vec<Property>>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Deserialize)]
struct FetchResult {
    id: String,
    item: FetchedItem,
    listing: Listing,
}

/// One listing, as the overlay shows it.
#[derive(Clone, Debug)]
pub struct PricingResult {
    pub id: String,
    pub item_level: Option<String>,
    pub stack_size: Option<f64>,
    pub corrupted: Option<bool>,
    pub quality: Option<String>,
    pub level: Option<String>,
    pub relative_date: String,
    pub price_amount: f64,
    pub price_currency: String,
    pub is_mine: bool,
    pub has_note: bool,
    pub has_fee: bool,
    pub account_name: String,
    /// "online", "afk" or "offline"
    pub account_status: &'static str,
    pub ign: String,
}

pub struct TradeClient {
    http: crate::http::Client,
    search: Rules,
    exchange: Rules,
    fetch: Rules,
    latency: Duration,
    cache: Mutex<HashMap<String, (Instant, Value)>>,
}

impl TradeClient {
    #[must_use]
    pub fn new(http: crate::http::Client, api_latency: Duration) -> Self {
        Self {
            http,
            search: Rules::default(),
            exchange: Rules::default(),
            fetch: Rules::default(),
            latency: api_latency,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn cached(&self, key: &str) -> Option<Value> {
        let mut cache = self.cache.lock();
        cache.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
        cache.get(key).map(|(_, v)| v.clone())
    }

    fn remember(&self, key: String, value: Value) {
        self.cache.lock().insert(key, (Instant::now(), value));
    }

    /// Reads the server's rate limit headers and adopts them.
    fn adjust(&self, rules: &Rules, headers: &reqwest::header::HeaderMap) {
        let Some(rule_names) = headers.get("x-rate-limit-rules").and_then(|v| v.to_str().ok()) else {
            return;
        };
        let (mut limits, mut states) = (Vec::new(), Vec::new());
        for rule in rule_names.split(',') {
            let get = |suffix: &str| {
                headers
                    .get(format!("x-rate-limit-{rule}{suffix}"))
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string()
            };
            limits.push(get(""));
            states.push(get("-state"));
        }
        rules.adjust(&limits.join(","), &states.join(","), self.latency);
    }

    /// POSTs a search body to `api/trade/<kind>/<league>`, with caching, rate limits and
    /// the API's error reply turned into an error. The caller checks the queue first.
    async fn post<B: Serialize + Hash>(&self, kind: &str, rules: &Rules, league: &str, body: &B) -> Result<Value> {
        let key = cache_key(kind, league, body);
        if let Some(hit) = self.cached(&key) {
            return Ok(hit);
        }

        rules.wait().await;

        let mut url = reqwest::Url::parse(TRADE_HOST)?;
        url.path_segments_mut()
            .map_err(|()| anyhow!("bad url"))?
            .extend(["api", "trade", kind, league]);
        let response = self
            .http
            .post(url)
            .header("Accept", "application/json")
            .json(body)
            .send()
            .await?;
        self.adjust(rules, response.headers());

        let status = response.status();
        let data: Value = response
            .json()
            .await
            .map_err(|e| anyhow!("trade {kind} failed (HTTP {status}): {e}"))?;
        check_api_error(&data)?;
        self.remember(key, data.clone());
        Ok(data)
    }

    /// Runs a search and returns the ids of the cheapest listings.
    pub async fn search(&self, body: &TradeRequest, league: &str) -> Result<SearchResult> {
        if self.cached(&cache_key("search", league, body)).is_none() {
            prevent_queue_creation(&[(&self.search, 1), (&self.fetch, 1)])?;
        }
        Ok(serde_json::from_value(
            self.post("search", &self.search, league, body).await?,
        )?)
    }

    /// Runs a bulk exchange search (the "Bulk Item Exchange" of the trade site).
    pub async fn exchange(&self, body: &BulkRequest, league: &str) -> Result<Value> {
        if self.cached(&cache_key("exchange", league, body)).is_none() {
            prevent_queue_creation(&[(&self.exchange, 1)])?;
        }
        self.post("exchange", &self.exchange, league, body).await
    }

    /// Fetches the listings behind up to 10 result ids.
    pub async fn fetch(&self, query_id: &str, ids: &[String], my_account: Option<&str>) -> Result<Vec<PricingResult>> {
        let key = format!("fetch:{}", ids.join(","));
        let raw: Vec<Value> = if let Some(Value::Array(a)) = self.cached(&key) {
            a
        } else {
            self.fetch.wait().await;

            let url = format!("{TRADE_HOST}/api/trade/fetch/{}?query={query_id}", ids.join(","));
            let response = self.http.get(url).send().await?;
            self.adjust(&self.fetch, response.headers());

            let status = response.status();
            let data: Value = response
                .json()
                .await
                .map_err(|e| anyhow!("trade fetch failed (HTTP {status}): {e}"))?;
            check_api_error(&data)?;
            let results: Vec<Value> = data
                .get("result")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|r| !r.is_null())
                .collect();
            self.remember(key, Value::Array(results.clone()));
            results
        };

        // one odd listing must not fail the whole page
        Ok(raw
            .into_iter()
            .filter_map(|r| Some(to_pricing_result(serde_json::from_value(r).ok()?, my_account)))
            .collect())
    }
}

/// Identifies a search by what it asks for: the hash of the typed request.
fn cache_key(kind: &str, league: &str, body: &impl Hash) -> String {
    let mut hasher = DefaultHasher::new();
    body.hash(&mut hasher);
    format!("{kind}:{league}:{:016x}", hasher.finish())
}

/// The API answers errors with `{"error": {"code": .., "message": ..}}`.
fn check_api_error(data: &Value) -> Result<()> {
    if let Some(err) = data.get("error").filter(|e| !e.is_null()) {
        let err: ApiError = serde_json::from_value(err.clone())?;
        bail!("{} (code {})", err.message, err.code);
    }
    Ok(())
}

fn to_pricing_result(result: FetchResult, my_account: Option<&str>) -> PricingResult {
    let FetchResult { id, item, listing } = result;

    let prop = |ty: i64| {
        item.properties
            .as_ref()
            .and_then(|ps| ps.iter().find(|p| p.type_ == Some(ty)))
            .and_then(|p| p.values.first())
            .map(|v| v.0.clone())
    };

    let status = if listing.fee.is_some() {
        "online"
    } else {
        listing.account.status()
    };

    PricingResult {
        id,
        item_level: prop(78).or_else(|| Some(item.ilvl.map_or_else(|| "undefined".into(), |v| v.to_string()))),
        stack_size: item.stack_size,
        corrupted: item.corrupted,
        quality: prop(6),
        level: prop(5),
        relative_date: relative_date(&listing.indexed),
        price_amount: listing.price.as_ref().map_or(0.0, |p| p.amount),
        price_currency: listing
            .price
            .as_ref()
            .map_or_else(|| "no price".into(), |p| p.currency.clone()),
        is_mine: Some(listing.account.name.as_str()) == my_account,
        has_note: item.note.is_some(),
        has_fee: listing.fee.is_some(),
        ign: listing.account.last_character_name,
        account_name: listing.account.name,
        account_status: status,
    }
}

/// "5 min. ago", like luxon's short relative time.
#[must_use]
pub fn relative_date(iso: &str) -> String {
    use time::format_description::well_known::Rfc3339;
    let Ok(then) = time::OffsetDateTime::parse(iso, &Rfc3339) else {
        return String::new();
    };
    relative_from_seconds((time::OffsetDateTime::now_utc() - then).whole_seconds())
}

fn relative_from_seconds(secs: i64) -> String {
    const UNITS: [(&str, i64); 5] = [
        ("yr.", 365 * 86400),
        ("mo.", 30 * 86400),
        ("day", 86400),
        ("hr.", 3600),
        ("min.", 60),
    ];
    let secs = secs.max(0);
    let (name, len) = UNITS.into_iter().find(|&(_, len)| secs >= len).unwrap_or(("sec.", 1));
    let n = secs / len;
    let name = if name == "day" && n != 1 { "days" } else { name };
    format!("{n} {name} ago")
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn relative_times() {
        assert_eq!(relative_from_seconds(5), "5 sec. ago");
        assert_eq!(relative_from_seconds(150), "2 min. ago");
        assert_eq!(relative_from_seconds(7300), "2 hr. ago");
        assert_eq!(relative_from_seconds(86400), "1 day ago");
        assert_eq!(relative_from_seconds(3 * 86400), "3 days ago");
    }

    #[test]
    fn parses_a_fetched_listing() {
        let json = serde_json::json!({
          "id": "abc",
          "item": { "ilvl": 84, "corrupted": true, "properties": [{"values": [["+20%", 1]], "type": 6}] },
          "listing": {
            "indexed": "2024-01-01T00:00:00Z",
            "price": { "amount": 3, "currency": "divine", "type": "~price" },
            "account": { "name": "Someone", "lastCharacterName": "Char", "online": { "status": "afk" } }
          }
        });
        let r = to_pricing_result(serde_json::from_value(json).unwrap(), Some("Someone"));
        assert_eq!((r.price_amount, r.price_currency.as_str()), (3.0, "divine"));
        assert_eq!(r.quality.as_deref(), Some("+20%"));
        assert_eq!(r.account_status, "afk");
        assert!(r.is_mine);
        assert_eq!(r.item_level.as_deref(), Some("84"));
    }
}
