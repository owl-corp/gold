//! Trade leagues.
use anyhow::{bail, Result};
use serde::Deserialize;

/// Names of the permanent leagues.
pub const PERMANENT_SC: [&str; 2] = ["Standard", "標準模式"];
const PERMANENT_HC: [&str; 2] = ["Hardcore", "專家模式"];

#[derive(Deserialize)]
struct ApiLeague {
    id: String,
    #[serde(default)]
    event: bool,
    #[serde(default)]
    rules: Vec<ApiRule>,
}

#[derive(Deserialize)]
struct ApiRule {
    id: String,
}

/// The leagues worth trading in, in the API's order. Index 0 is Standard and
/// index 1 is the current challenge league.
pub async fn load(client: &reqwest::Client) -> Result<Vec<String>> {
    let response = client
        .get("https://www.pathofexile.com/api/leagues?type=main&realm=pc")
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("leagues request failed: HTTP {}", response.status());
    }
    let leagues: Vec<ApiLeague> = response.json().await?;
    Ok(select_leagues(leagues))
}

fn select_leagues(leagues: Vec<ApiLeague>) -> Vec<String> {
    leagues
        .into_iter()
        .filter(|l| {
            !PERMANENT_HC.contains(&l.id.as_str())
                && !l
                    .rules
                    .iter()
                    .any(|r| r.id == "NoParties" || (r.id == "HardMode" && !l.event))
        })
        .map(|l| l.id)
        .collect()
}

/// The league to use when none is configured, or the configured one is gone.
pub fn default_league(leagues: &[String]) -> Option<&str> {
    // the second entry is the challenge league, if there is one
    leagues.get(1).or_else(|| leagues.first()).map(String::as_str)
}

/// Private leagues have no public prices.
pub fn is_private(id: &str) -> bool {
    if id.contains("Ruthless") {
        return true;
    }
    // "(PL12345)" suffix
    id.strip_suffix(')')
        .and_then(|rest| rest.rsplit_once("(PL"))
        .is_some_and(|(_, digits)| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_and_private_leagues() {
        let json = r#"[
          {"id":"Standard","rules":[]},{"id":"Hardcore","rules":[{"id":"Hardcore"}]},
          {"id":"Solo Self-Found","rules":[{"id":"NoParties"}]},
          {"id":"Ruthless","rules":[{"id":"HardMode"}]},
          {"id":"Covens","rules":[]},{"id":"Hardcore Covens","rules":[{"id":"Hardcore"}]}]"#;
        let leagues = select_leagues(serde_json::from_str(json).unwrap());
        assert_eq!(leagues, ["Standard", "Covens", "Hardcore Covens"]);
        assert_eq!(default_league(&leagues), Some("Covens"));
        assert!(is_private("My League (PL12345)"));
        assert!(is_private("Ruthless Covens"));
        assert!(!is_private("Covens"));
    }
}
