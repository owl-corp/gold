//! Chat commands: paste text into the game's chat box..
use crate::{clipboard, hypr};
use anyhow::{anyhow, Result};
use std::time::Duration;
use tokio::sync::Mutex;

const PLACEHOLDER_LAST: &str = "@last";
/// Typing one of these first makes the game clear the chat box itself.
const AUTO_CLEAR: [char; 6] = ['#', '%', '@', '$', '&', '/'];
/// The game must read the clipboard within this time; after that the old contents come back.
const RESTORE_AFTER: Duration = Duration::from_millis(120);
const CTRL: &str = "Control_L";

/// Only one chat action at a time; a second press while one runs is ignored.
static BUSY: Mutex<()> = Mutex::const_new(());

pub async fn type_in_chat(text: &str, send: bool) -> Result<()> {
    let _guard = BUSY
        .try_lock()
        .map_err(|_| anyhow!("a chat command is already running"))?;
    let saved = clipboard::read_text().await;

    let result = keys(text, send).await;

    tokio::time::sleep(RESTORE_AFTER).await;
    clipboard::restore(saved).await;
    result
}

async fn keys(text: &str, send: bool) -> Result<()> {
    if let Some(rest) = text.strip_prefix(&format!("{PLACEHOLDER_LAST} ")) {
        clipboard::write_text(rest).await?;
        hypr::send_combo(CTRL, "Return").await?;
    } else if let Some(rest) = text.strip_suffix(PLACEHOLDER_LAST) {
        clipboard::write_text(rest).await?;
        hypr::send_combo(CTRL, "Return").await?;
        hypr::tap("Home").await?;
        // press twice to focus input when using controller
        hypr::tap("Home").await?;
        hypr::tap("Delete").await?;
    } else {
        clipboard::write_text(text).await?;
        hypr::tap("Return").await?;
        if !text.starts_with(AUTO_CLEAR) {
            hypr::send_combo(CTRL, "a").await?;
        }
    }

    hypr::send_combo(CTRL, "v").await?;

    if send {
        hypr::tap("Return").await?;
        // restore the last chat
        hypr::tap("Return").await?;
        hypr::tap("Up").await?;
        hypr::tap("Up").await?;
        hypr::tap("Escape").await?;
    }
    Ok(())
}
