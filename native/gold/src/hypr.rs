//! All compositor-specific syntax lives in this file.
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

pub const POE_CLASS: &str = "steam_app_238960";
/// Window selector (a regex) matching only the game.
const GAME_WINDOW: &str = "class:^(steam_app_238960)$";

fn sock(name: &str) -> Result<PathBuf> {
    let rt = std::env::var("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR not set")?;
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").context("not running under Hyprland")?;
    Ok(PathBuf::from(rt).join("hypr").join(sig).join(name))
}

async fn request(cmd: &str) -> Result<String> {
    let mut s = UnixStream::connect(sock(".socket.sock")?).await?;
    s.write_all(cmd.as_bytes()).await?;
    let mut out = String::new();
    s.read_to_string(&mut out).await?;
    Ok(out)
}

#[derive(Deserialize)]
pub struct Cursor {
    pub x: i32,
    pub y: i32,
}

#[derive(Deserialize)]
pub struct Monitor {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub focused: bool,
}

pub async fn cursor_pos() -> Result<Cursor> {
    Ok(serde_json::from_str(&request("j/cursorpos").await?)?)
}

pub async fn monitors() -> Result<Vec<Monitor>> {
    Ok(serde_json::from_str(&request("j/monitors").await?)?)
}

/// Run a Lua-config dispatcher, e.g. `hl.dsp.focus({ window = "class:foo" })`.
/// Hyprland answers "ok" on success and an error message otherwise.
async fn dispatch(lua: &str) -> Result<()> {
    let reply = request(&format!("dispatch {lua}")).await?;
    if reply.trim() != "ok" {
        bail!("hyprland dispatch failed: {}", reply.trim());
    }
    Ok(())
}

async fn send_key(state: &str, key: &str) -> Result<()> {
    dispatch(&format!(
        r#"hl.dsp.send_key_state({{ mods = "", state = "{state}", key = "{key}", window = "{GAME_WINDOW}" }})"#
    ))
    .await
}

/// Press `modifier` + `key` in the game even when it is not focused.
/// Hyprland's `send_shortcut` drops the modifier for this XWayland window (only
/// the bare key arrives), so the modifier is pressed as a key of its own.
pub async fn send_combo(modifier: &str, key: &str) -> Result<()> {
    send_key("down", modifier).await?;
    let tapped = async {
        send_key("down", key).await?;
        send_key("up", key).await
    }
    .await;
    // Always release the modifier, or the game would see it held down.
    let released = send_key("up", modifier).await;
    tapped.and(released)
}

/// Class of the currently focused window ("" when none).
pub async fn active_class() -> Result<String> {
    #[derive(Deserialize)]
    struct W {
        #[serde(default)]
        class: String,
    }
    let out = request("j/activewindow").await?;
    Ok(serde_json::from_str::<W>(&out).map(|w| w.class).unwrap_or_default())
}

/// Give keyboard focus back to the game, only if it lost it.
/// A focus dispatch warps the cursor to the window centre, so skip it when
/// Hyprland already returned focus to PoE on its own.
pub async fn focus_game() -> Result<()> {
    // Right after the overlay closes Hyprland may not have handed focus back yet;
    // checking too early would refocus (and warp the cursor) for nothing.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    if active_class().await? == POE_CLASS {
        return Ok(());
    }
    dispatch(&format!(r#"hl.dsp.focus({{ window = "{GAME_WINDOW}" }})"#)).await
}
