//! Item copy: send Ctrl+C to the game, read the item text from the Wayland clipboard.
//! Uses wl-clipboard so no
//! background clipboard server has to live inside this process.
use crate::hypr;
use anyhow::{bail, Context, Result};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// `wl-copy`/`wl-paste` need `WAYLAND_DISPLAY`, which a systemd user service may not have;
/// fall back to the first Wayland socket in the runtime directory.
fn wl_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        let socket = crate::config::xdg_runtime_dir()
            .ok()
            .and_then(|dir| std::fs::read_dir(dir).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name())
            .filter(|n| {
                n.to_str().is_some_and(|n| {
                    n.starts_with("wayland-")
                        && !std::path::Path::new(n)
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("lock"))
                })
            })
            .min();
        if let Some(socket) = socket {
            cmd.env("WAYLAND_DISPLAY", socket);
        }
    }
    cmd
}
use tokio::sync::Mutex;

const POLL_DELAY: Duration = Duration::from_millis(48);
const POLL_LIMIT: Duration = Duration::from_millis(500);

/// Held while the clipboard and the game's keys are in use, by a copy or a chat command.
/// A second press while one runs is rejected, and two flows never interleave key presses.
pub static BUSY: Mutex<()> = Mutex::const_new(());

/// A copy or chat command is already running. Not worth showing to the user.
#[derive(Debug)]
pub struct Busy;

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a copy is already in progress")
    }
}

impl std::error::Error for Busy {}

fn is_item(text: &str) -> bool {
    text.starts_with("Item Class: ")
}

pub async fn read_text() -> Option<String> {
    read().await
}

/// Puts back what `read_text` returned; an empty clipboard stays empty.
pub async fn restore(saved: Option<String>) {
    let result = match saved {
        Some(text) => write(&text).await,
        None => clear().await,
    };
    if let Err(e) = result {
        eprintln!("clipboard restore failed: {e:#}");
    }
}

pub async fn write_text(text: &str) -> Result<()> {
    write(text).await
}

async fn read() -> Option<String> {
    let out = wl_command("wl-paste")
        .args(["--no-newline", "--type", "text"])
        .output()
        .await
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

async fn write(text: &str) -> Result<()> {
    let mut child = wl_command("wl-copy")
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
    wl_command("wl-copy").arg("--clear").status().await?;
    Ok(())
}

/// Copy the item under the cursor in the game and return its text.
pub async fn copy_item() -> Result<String> {
    let _guard = BUSY.try_lock().map_err(|_busy| anyhow::Error::new(Busy))?;

    // Keep what the user had on the clipboard, unless it is an old item.
    let before = read().await.filter(|t| !is_item(t));

    // Proton only publishes the new clipboard after a change, so make sure
    // the current contents differ from anything the game could copy.
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    write(&format!("__GOLD_FORCE_EMPTY_{stamp}")).await?;

    let sent = hypr::send_combo("Control_L", "c").await;

    let mut found = None;
    if sent.is_ok() {
        let mut waited = Duration::ZERO;
        while waited < POLL_LIMIT {
            tokio::time::sleep(POLL_DELAY).await;
            waited += POLL_DELAY;
            if let Some(text) = read().await.filter(|t| is_item(t)) {
                found = Some(text);
                break;
            }
        }
    }

    // always put the user's clipboard back, even when the key press failed, and don't
    // let a failed restore throw away an item that was read
    restore(before).await;
    sent?;

    let Some(text) = found else {
        bail!("no item text found; is an item under the cursor?");
    };
    if let Err(e) = log_item(&text) {
        eprintln!("could not log the item: {e}");
    }
    Ok(text)
}

/// Every copied item is appended to a log, so real items can be added to the test corpus later.
fn log_item(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let dir = crate::config::xdg_dir("XDG_STATE_HOME", ".local/state")
        .ok_or_else(|| std::io::Error::other("no state directory"))?
        .join("gold");
    std::fs::create_dir_all(&dir)?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("items.log"))?;
    writeln!(f, "{text}\n=====")
}
