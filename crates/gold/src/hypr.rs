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
    let rt = crate::config::xdg_runtime_dir()?;
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").context("not running under Hyprland")?;
    Ok(rt.join("hypr").join(sig).join(name))
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
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default = "one")]
    pub scale: f64,
    /// Wayland transform: odd values (90/270 degrees, flipped or not) swap width and height.
    #[serde(default)]
    pub transform: i32,
}

const fn one() -> f64 {
    1.0
}

impl Monitor {
    /// Size in layout pixels (after scale and rotation).
    fn layout_size(&self) -> (f64, f64) {
        let (w, h) = if self.transform % 2 != 0 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        };
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        (f64::from(w) / scale, f64::from(h) / scale)
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        let (w, h) = self.layout_size();
        x >= self.x && y >= self.y && f64::from(x - self.x) < w && f64::from(y - self.y) < h
    }
}

/// The monitor under the cursor (else the focused one, else the first), and the cursor
/// position inside it in that monitor's logical pixels, which is what a layer surface uses.
pub fn locate(monitors: &[Monitor], cursor: &Cursor) -> Option<(String, i32, i32)> {
    let m = monitors
        .iter()
        .position(|m| m.contains(cursor.x, cursor.y))
        .or_else(|| monitors.iter().position(|m| m.focused))
        .or_else(|| (!monitors.is_empty()).then_some(0))?;
    let m = monitors.get(m)?;
    // the cursor is in layout coordinates, which are already logical, so no scaling is needed
    Some((m.name.clone(), cursor.x - m.x, cursor.y - m.y))
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

async fn send_key_state(state: &str, key: &str) -> Result<()> {
    dispatch(&format!(
        r#"hl.dsp.send_key_state({{ mods = "", state = "{state}", key = "{key}", window = "{GAME_WINDOW}" }})"#
    ))
    .await
}

/// Press and release a key in the game.
pub async fn tap(key: &str) -> Result<()> {
    send_key_state("down", key).await?;
    send_key_state("up", key).await
}

/// Press `modifier` + `key` in the game even when it is not focused.
/// Hyprland's `send_shortcut` drops the modifier for this `XWayland` window (only
/// the bare key arrives), so the modifier is pressed as a key of its own.
pub async fn send_combo(modifier: &str, key: &str) -> Result<()> {
    send_key_state("down", modifier).await?;
    let tapped = async {
        send_key_state("down", key).await?;
        send_key_state("up", key).await
    }
    .await;
    // Always release the modifier, or the game would see it held down.
    let released = send_key_state("up", modifier).await;
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
/// Hyprland already returned focus to `PoE` on its own.
pub async fn focus_game() -> Result<()> {
    // Right after the overlay closes Hyprland may not have handed focus back yet;
    // checking too early would refocus (and warp the cursor) for nothing.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    if active_class().await? == POE_CLASS {
        return Ok(());
    }
    dispatch(&format!(r#"hl.dsp.focus({{ window = "{GAME_WINDOW}" }})"#)).await
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "tests may panic on failure")]
mod tests {
    use super::*;

    fn mon(name: &str, x: i32, w: i32, h: i32, scale: f64, transform: i32) -> Monitor {
        Monitor {
            name: name.into(),
            x,
            y: 0,
            focused: name == "A",
            width: w,
            height: h,
            scale,
            transform,
        }
    }

    #[test]
    fn cursor_picks_its_monitor_with_scale_and_rotation() {
        // A: 2560x1440 at 0; B: 1920x1080 rotated, at x=2560, so 1080 wide and 1920 tall;
        // C: 3840x2160 at scale 2 (1920x1080 in the layout), at x=3640
        let ms = || {
            vec![
                mon("A", 0, 2560, 1440, 1.0, 0),
                mon("B", 2560, 1920, 1080, 1.0, 1),
                mon("C", 3640, 3840, 2160, 2.0, 0),
            ]
        };
        assert_eq!(locate(&ms(), &Cursor { x: 100, y: 50 }).unwrap(), ("A".into(), 100, 50));
        assert_eq!(
            locate(&ms(), &Cursor { x: 2600, y: 1800 }).unwrap(),
            ("B".into(), 40, 1800)
        );
        assert_eq!(
            locate(&ms(), &Cursor { x: 4000, y: 1000 }).unwrap(),
            ("C".into(), 360, 1000)
        );
        // outside everything: the focused monitor
        assert_eq!(locate(&ms(), &Cursor { x: 9000, y: 9000 }).unwrap().0, "A");
    }
}
