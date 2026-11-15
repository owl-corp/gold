//! Runs the trade or bulk search for the active preset.
use crate::app::App;
use crate::session::{Outcome, Row};
use anyhow::Result;
use gold_core::parser::item::ParsedItem;
use gold_core::price_check::filters::presets::FilterPreset;
use gold_core::price_check::ninja::display_rounding;
use gold_core::price_check::trade::request::create_trade_request;

const SHOW_ROWS: usize = 10;

pub async fn run(app: &App, _item: &ParsedItem, preset: &FilterPreset) -> Result<Vec<Outcome>> {
    let filters = &preset.filters;
    let league = &filters.trade.league;

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
