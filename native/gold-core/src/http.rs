//! The HTTP client shared by every request to GGG and poe.ninja.
use anyhow::Result;

pub use reqwest::Client;

/// `contact` goes into the User-Agent, because GGG asks API clients to identify themselves.
pub fn client(contact: &str) -> Result<Client> {
    let agent = format!("gold-native/{} (contact: {contact})", env!("CARGO_PKG_VERSION"));
    Ok(Client::builder().user_agent(agent).cookie_store(true).build()?)
}
