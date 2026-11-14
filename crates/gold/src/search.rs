//! Runs the trade or bulk search for the active preset.
use crate::app::App;
use crate::session::{Outcome, Row};
use anyhow::Result;
use gold_core::parser::item::ParsedItem;
use gold_core::price_check::filters::presets::FilterPreset;
use gold_core::price_check::ninja::display_rounding;
use gold_core::price_check::trade::bulk::{default_have, exec_bulk_search, BulkSearch};
use gold_core::price_check::trade::request::create_trade_request;

const SHOW_ROWS: usize = 10;

/// `apiToSatisfySearch`: with no stat filter on, items with a trade tag go to the bulk exchange.
pub fn uses_bulk(item: &ParsedItem, preset: &FilterPreset) -> bool {
    !preset.stats.iter().any(|s| !s.disabled) && item.info.trade_tag.is_some()
}

pub async fn run(app: &App, item: &ParsedItem, preset: &FilterPreset) -> Result<Vec<Outcome>> {
    let filters = &preset.filters;
    let league = &filters.trade.league;

    if uses_bulk(item, preset) {
        let want = item.info.trade_tag.as_deref().unwrap_or_default();
        let have = default_have(&item.info.ref_name);
        let account = app.account_name();
        let first = exec_bulk_search(&app.trade, want, filters, &have, account).await?;

        let mut outcomes = Vec::new();
        for (tag, found) in have.iter().zip(first) {
            let search = match found {
                Some(s) => s,
                // that currency has to be asked for on its own
                None => match exec_bulk_search(&app.trade, want, filters, &[*tag], account)
                    .await?
                    .into_iter()
                    .next()
                    .flatten()
                {
                    Some(s) => s,
                    None => continue,
                },
            };
            outcomes.push(bulk_outcome(&search, league));
        }
        return Ok(outcomes);
    }

    let request = create_trade_request(&app.data, filters, &preset.stats)?;
    let found = app.trade.search(&request, league).await?;
    let rows = fetch_rows(app, &found.id, found.result.get(..SHOW_ROWS).unwrap_or(&found.result)).await?;
    let pending = found.result.get(SHOW_ROWS..).unwrap_or_default().to_vec();

    Ok(vec![Outcome {
        fetch_id: Some(found.id.clone()),
        pending,
        total: found.total,
        rows,
        url: format!(
            "https://www.pathofexile.com/trade/search/{}/{}",
            url_part(league),
            found.id
        ),
        heading: None,
    }])
}

/// Fetches the listings for these result ids (at most 10 per request).
pub async fn fetch_rows(app: &App, query_id: &str, ids: &[String]) -> Result<Vec<Row>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let listings = app.trade.fetch(query_id, ids, app.account_name()).await?;
    Ok(listings
        .iter()
        .map(|l| {
            let mut detail = vec![l.relative_date.clone()];
            if let Some(stack) = l.stack_size {
                detail.push(format!("stack {stack}"));
            }
            if l.corrupted == Some(true) {
                detail.push("corrupted".into());
            }
            Row {
                price: format!("{} {}", display_rounding(l.price_amount, false), l.price_currency),
                detail: detail.join(" · "),
                status: l.account_status,
                who: if l.ign.is_empty() {
                    l.account_name.clone()
                } else {
                    l.ign.clone()
                },
                mine: l.is_mine,
            }
        })
        .collect())
}

/// How many more listings one "show more" fetches.
pub const MORE_ROWS: usize = SHOW_ROWS;

fn bulk_outcome(search: &BulkSearch, league: &str) -> Outcome {
    let rows = search
        .listed
        .iter()
        .take(SHOW_ROWS)
        .map(|l| Row {
            price: format!(
                "{} {}",
                display_rounding(l.exchange_amount / l.item_amount, false),
                search.have_tag
            ),
            detail: format!("stock {} · {}", l.stock, l.relative_date),
            status: l.account_status,
            who: if l.ign.is_empty() {
                l.account_name.clone()
            } else {
                l.ign.clone()
            },
            mine: l.is_mine,
        })
        .collect();
    Outcome {
        total: search.total,
        rows,
        url: format!(
            "https://www.pathofexile.com/trade/exchange/{}/{}",
            url_part(league),
            search.query_id
        ),
        heading: Some(format!("in {}", search.have_tag)),
        fetch_id: None,
        pending: vec![],
    }
}

fn url_part(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::url_part;

    #[test]
    fn percent_encodes_utf8_bytes() {
        assert_eq!(url_part("Hardcore Covens"), "Hardcore%20Covens");
        assert_eq!(url_part("標"), "%E6%A8%99");
    }
}
