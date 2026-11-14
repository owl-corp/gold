//! Item copy: send Ctrl+C to the game, read the item text from the Wayland clipboard.
//! Uses wl-clipboard so no
//! background clipboard server has to live inside this process.
use crate::hypr;
use anyhow::{anyhow, bail, Context, Result};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;

const POLL_DELAY: Duration = Duration::from_millis(48);
const POLL_LIMIT: Duration = Duration::from_millis(500);

/// Only one copy at a time; a second bind press while polling is ignored.
static BUSY: Mutex<()> = Mutex::const_new(());

fn is_item(text: &str) -> bool {
    text.starts_with("Item Class: ")
}

async fn read() -> Option<String> {
    let out = Command::new("wl-paste")
        .args(["--no-newline", "--type", "text"])
        .output()
        .await
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

async fn write(text: &str) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .context("wl-copy not found")?;
    child
        .stdin
        .take()
        .context("no stdin")?
        .write_all(text.as_bytes())
        .await?;
    child.wait().await?;
    Ok(())
}

async fn clear() -> Result<()> {
    Command::new("wl-copy").arg("--clear").status().await?;
    Ok(())
}

/// Copy the item under the cursor in the game and return its text.
pub async fn copy_item() -> Result<String> {
    let _guard = BUSY.try_lock().map_err(|_| anyhow!("a copy is already in progress"))?;

    // Keep what the user had on the clipboard, unless it is an old item.
    let before = read().await.filter(|t| !is_item(t));

    // Proton only publishes the new clipboard after a change, so make sure
    // the current contents differ from anything the game could copy.
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    write(&format!("__GOLD_FORCE_EMPTY_{stamp}")).await?;

    hypr::send_combo("Control_L", "c").await?;

    let mut found = None;
    let mut waited = Duration::ZERO;
    while waited < POLL_LIMIT {
        tokio::time::sleep(POLL_DELAY).await;
        waited += POLL_DELAY;
        if let Some(text) = read().await.filter(|t| is_item(t)) {
            found = Some(text);
            break;
        }
    }

    match &before {
        Some(text) => write(text).await?,
        None => clear().await?,
    }

    let Some(text) = found else {
        bail!("no item text found; is an item under the cursor?");
    };
    log_item(&text);
    Ok(text)
}

/// Every copied item goes to a log; it becomes the parser's test corpus.
fn log_item(text: &str) {
    let dir = match (std::env::var("XDG_STATE_HOME"), std::env::var("HOME")) {
        (Ok(s), _) => std::path::PathBuf::from(s),
        (_, Ok(h)) => std::path::PathBuf::from(h).join(".local/state"),
        _ => return,
    }
    .join("gold");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("items.log"))
    {
        use std::io::Write;
        let _ = writeln!(f, "{text}\n=====");
    }
}
