//! The HTTP client shared by every request to GGG and poe.ninja.
use anyhow::Result;
use std::sync::Arc;

pub use reqwest::Client;

/// `contact` goes into the User-Agent, because GGG asks API clients to identify themselves.
/// `poesessid` (optional) logs the client in on pathofexile.com; the cookie is tied to
/// that domain, so it is never sent to poe.ninja.
pub fn client(contact: &str, poesessid: Option<&str>) -> Result<Client> {
    let agent = format!("gold-native/{} (contact: {contact})", env!("CARGO_PKG_VERSION"));
    let jar = Arc::new(reqwest::cookie::Jar::default());
    if let Some(id) = poesessid.map(str::trim).filter(|id| !id.is_empty()) {
        let site = reqwest::Url::parse("https://www.pathofexile.com/")?;
        jar.add_cookie_str(
            &format!("POESESSID={id}; Domain=www.pathofexile.com; Path=/; Secure"),
            &site,
        );
    }
    Ok(Client::builder().user_agent(agent).cookie_provider(jar).build()?)
}
