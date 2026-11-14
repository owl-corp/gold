mod app;
mod chat;
mod clipboard;
mod config;
mod flow;
mod hypr;
mod search;
mod session;
mod ui;
mod view;

use anyhow::{bail, Result};
use app::App;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;

/// `GOLD_DEV=1` runs a second daemon (and overlay) in its own directory, so a dev build can be tried
/// next to the installed one. `gold ctl` talks to a running dev daemon first, so the keybinds follow it.
fn runtime_dir() -> PathBuf {
    let rt = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set");
    let d = PathBuf::from(rt).join(if std::env::var_os("GOLD_DEV").is_some() { "gold-dev" } else { "gold" });
    std::fs::create_dir_all(&d).ok();
    d
}

/// The dev daemon's control socket, if one is running (a stale socket file does not answer).
async fn connect_dev() -> Option<UnixStream> {
    let dir = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join("gold-dev");
    UnixStream::connect(dir.join("ctl.sock")).await.ok()
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("daemon") => daemon().await,
        Some("ctl") => {
            let cmd: Vec<String> = args.collect();
            if cmd.is_empty() {
                bail!("usage: gold ctl <price-check|close|status>");
            }
            let mut s = match connect_dev().await {
                Some(s) => s,
                None => UnixStream::connect(runtime_dir().join("ctl.sock")).await?,
            };
            s.write_all(format!("{}\n", cmd.join(" ")).as_bytes()).await?;
            let mut reply = String::new();
            BufReader::new(s).read_line(&mut reply).await?;
            print!("{reply}");
            Ok(())
        }
        _ => bail!("usage: gold <daemon|ctl ...>"),
    }
}

async fn daemon() -> Result<()> {
    // A second daemon would take over both sockets and leave the first one orphaned.
    if UnixStream::connect(runtime_dir().join("ctl.sock")).await.is_ok() {
        bail!("an gold daemon is already running");
    }
    let app = Arc::new(App::new(config::Config::load())?);
    let ui = ui::Ui::start(runtime_dir().join("ui.sock"))?;

    let path = runtime_dir().join("ctl.sock");
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    eprintln!("gold daemon ready");

    // Overlay closed by the UI (Esc, button): hand the keyboard back to the game.
    let mut closed = ui.closed();
    let closing_app = app.clone();
    tokio::spawn(async move {
        loop {
            match closed.recv().await {
                Ok(()) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break,
            }
            flow::closed_by_overlay(&closing_app);
            if let Err(e) = hypr::focus_game().await {
                eprintln!("focus_game: {e:#}");
            }
        }
    });

    let (action_app, action_ui) = (app.clone(), ui.clone());
    let mut actions = ui.actions();
    tokio::spawn(async move {
        loop {
            match actions.recv().await {
                Ok(action) => flow::handle_action(&action_app, &action_ui, action).await,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    loop {
        let (stream, _) = listener.accept().await?;
        let ui = ui.clone();
        let app = app.clone();
        tokio::spawn(async move {
            let (r, mut w) = stream.into_split();
            let mut line = String::new();
            if BufReader::new(r).read_line(&mut line).await.is_ok() {
                let reply = match handle(&app, &ui, line.trim()).await {
                    Ok(s) => s,
                    Err(e) => format!("error: {e:#}"),
                };
                let _ = w.write_all(format!("{reply}\n").as_bytes()).await;
            }
        });
    }
}

async fn handle(app: &Arc<App>, ui: &ui::Ui, cmd: &str) -> Result<String> {
    match cmd {
        "price-check" => {
            flow::price_check(app, ui).await?;
            Ok("ok".into())
        }
        "close" => {
            if flow::close(app, ui) {
                hypr::focus_game().await?;
            }
            Ok("ok".into())
        }
        "status" => Ok(format!("ui clients: {}", ui.clients())),
        chat if chat.starts_with("chat ") => {
            if hypr::active_class().await? != hypr::POE_CLASS {
                bail!("Path of Exile is not the focused window");
            }
            let text = chat["chat ".len()..].trim_end();
            let (text, send) = match text.strip_suffix(" --send") {
                Some(t) => (t, true),
                None => (text, false),
            };
            chat::type_in_chat(text, send).await?;
            Ok("ok".into())
        }
        other => bail!("unknown command: {other}"),
    }
}
