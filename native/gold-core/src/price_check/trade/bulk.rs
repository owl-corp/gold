//! Bulk item exchange for currency and similar items.
use super::client::{relative_date, TradeClient};
use super::Account;
use crate::price_check::filters::ItemFilters;
use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};

const SHOW_RESULTS: usize = 20;
const API_FETCH_LIMIT: u64 = 100;

#[derive(Deserialize)]
struct Offer {
    exchange: Exchange,
    item: OfferItem,
}

#[derive(Deserialize)]
struct Exchange {
    currency: String,
    amount: f64,
}

#[derive(Deserialize)]
struct OfferItem {
    amount: f64,
    stock: f64,
}

#[derive(Deserialize)]
struct Listing {
    indexed: String,
    offers: Vec<Offer>,
    account: Account,
}

#[derive(Deserialize)]
struct FetchResult {
    id: String,
    listing: Listing,
}

#[derive(Deserialize)]
struct SearchResult {
    id: String,
    /// In the API's order (cheapest first), which decides ties after sorting.
    #[serde(default)]
    result: serde_json::Map<String, Value>,
    total: u64,
}

#[derive(Clone, Debug)]
pub struct BulkListing {
    pub id: String,
    pub relative_date: String,
    pub exchange_amount: f64,
    pub item_amount: f64,
    pub stock: f64,
    /// "online", "afk" or "offline"
    pub account_status: &'static str,
    pub is_mine: bool,
    pub account_name: String,
    pub ign: String,
}

#[derive(Clone, Debug)]
pub struct BulkSearch {
    pub query_id: String,
    pub have_tag: String,
    pub total: u64,
    pub listed: Vec<BulkListing>,
}

/// `createTradeRequest` of the bulk module. `want` is the item's trade tag.
pub fn create_bulk_request(filters: &ItemFilters, want: &str, have: &[&str]) -> Value {
    let status = if filters.trade.offline {
        "any"
    } else if filters.trade.online_in_league {
        "onlineleague"
    } else {
        "online"
    };
    let mut query = json!({ "have": have, "want": [want], "status": { "option": status } });
    if let Some(min) = filters.stack_size.as_ref().filter(|f| !f.disabled) {
        query["minimum"] = json!(min.value);
    }
    json!({ "engine": "new", "query": query, "sort": { "have": "asc" } })
}

/// The currencies to price an item in. At league start many players set wrong prices,
/// so Chaos and Divine Orbs are priced in the other one only.
pub fn default_have(item_ref_name: &str) -> Vec<&'static str> {
    match item_ref_name {
        "Chaos Orb" => vec!["divine"],
        "Divine Orb" => vec!["chaos"],
        _ => vec!["divine", "chaos"],
    }
}

/// `execBulkSearch`. An entry is None when that currency has to be loaded on its own.
pub async fn exec_bulk_search(
    client: &TradeClient,
    want: &str,
    filters: &ItemFilters,
    have: &[&str],
    my_account: Option<&str>,
) -> Result<Vec<Option<BulkSearch>>> {
    let body = create_bulk_request(filters, want, have);
    let raw = client.exchange(&body, &filters.trade.league).await?;
    let query: SearchResult = serde_json::from_value(raw)?;

    let offer = 0;
    // a listing that doesn't have the expected shape is skipped, not fatal
    let parsed: Vec<FetchResult> = query
        .result
        .values()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect();
    let results: Vec<&FetchResult> = parsed.iter().filter(|r| r.listing.offers.len() == 1).collect();

    Ok(have
        .iter()
        .map(|tag| {
            let mut of_tag: Vec<&FetchResult> = results
                .iter()
                .copied()
                .filter(|r| r.listing.offers[offer].exchange.currency == *tag)
                .collect();

            let loaded_on_demand = *tag == "chaos" && of_tag.len() < SHOW_RESULTS && query.total > API_FETCH_LIMIT;
            if loaded_on_demand {
                return None;
            }

            let ratio = |r: &FetchResult| r.listing.offers[offer].exchange.amount / r.listing.offers[offer].item.amount;
            of_tag.sort_by(|a, b| ratio(a).partial_cmp(&ratio(b)).unwrap_or(std::cmp::Ordering::Equal));
            let listed = of_tag
                .iter()
                .take(SHOW_RESULTS)
                .map(|r| to_listing(r, my_account, offer))
                .collect();

            let chaos_is_loaded = *tag == "divine"
                && of_tag.len() < results.len()
                && ((results.len() - of_tag.len()) >= SHOW_RESULTS || query.total <= API_FETCH_LIMIT);

            Some(BulkSearch {
                query_id: query.id.clone(),
                have_tag: tag.to_string(),
                // this is a best guess when making request with multiple `have` currencies
                total: if chaos_is_loaded {
                    of_tag.len() as u64
                } else {
                    query.total.saturating_sub((results.len() - of_tag.len()) as u64)
                },
                listed,
            })
        })
        .collect())
}

fn to_listing(result: &FetchResult, my_account: Option<&str>, offer: usize) -> BulkListing {
    let o = &result.listing.offers[offer];
    let account = &result.listing.account;
    BulkListing {
        id: result.id.clone(),
        relative_date: relative_date(&result.listing.indexed),
        exchange_amount: o.exchange.amount,
        item_amount: o.item.amount,
        stock: o.item.stock,
        is_mine: Some(account.name.as_str()) == my_account,
        ign: account.last_character_name.clone(),
        account_name: account.name.clone(),
        account_status: account.status(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(id: &str, currency: &str, exchange: f64, item: f64) -> Value {
        json!({ "id": id, "listing": {
            "indexed": "2024-01-01T00:00:00Z",
            "offers": [{ "exchange": { "currency": currency, "amount": exchange }, "item": { "amount": item, "stock": 100 } }],
            "account": { "name": "A", "lastCharacterName": "C" } } })
    }

    fn parse(raw: Value) -> (Vec<FetchResult>, Vec<String>) {
        let query: SearchResult = serde_json::from_value(raw).unwrap();
        let keys = query.result.keys().cloned().collect();
        let parsed = query
            .result
            .values()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
        (parsed, keys)
    }

    #[test]
    fn sorts_by_unit_price_and_splits_by_currency() {
        let (parsed, _) = parse(json!({ "id": "q", "total": 3, "result": {
            "a": listing("a", "chaos", 300.0, 2.0),
            "b": listing("b", "chaos", 100.0, 1.0),
            "c": listing("c", "divine", 1.0, 1.0),
        }}));
        let mut of_chaos: Vec<&FetchResult> = parsed
            .iter()
            .filter(|r| r.listing.offers[0].exchange.currency == "chaos")
            .collect();
        let ratio = |r: &FetchResult| r.listing.offers[0].exchange.amount / r.listing.offers[0].item.amount;
        of_chaos.sort_by(|a, b| ratio(a).partial_cmp(&ratio(b)).unwrap());
        assert_eq!(of_chaos[0].id, "b"); // 100 per item beats 150 per item
        assert_eq!(to_listing(of_chaos[0], None, 0).account_status, "offline");
    }

    #[test]
    fn keeps_the_api_order_and_skips_odd_listings() {
        let (parsed, keys) = parse(json!({ "id": "q", "total": 3, "result": {
            "z": listing("z", "chaos", 100.0, 1.0),
            "m": json!({ "id": "m" }), // not a listing
            "a": listing("a", "chaos", 100.0, 1.0),
        }}));
        // not sorted by id: ties stay in the order the API gave them
        assert_eq!(keys, ["z", "m", "a"]);
        assert_eq!(parsed.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["z", "a"]);
    }

    #[test]
    fn default_currencies() {
        assert_eq!(default_have("Chaos Orb"), ["divine"]);
        assert_eq!(default_have("Divine Orb"), ["chaos"]);
        assert_eq!(default_have("Exalted Orb"), ["divine", "chaos"]);
    }
}
