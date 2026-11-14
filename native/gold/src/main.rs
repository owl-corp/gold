mod app;
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

fn runtime_dir() -> PathBuf {
    let rt = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set");
    let d = PathBuf::from(rt).join("gold");
    std::fs::create_dir_all(&d).ok();
    d
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
            let mut s = UnixStream::connect(runtime_dir().join("ctl.sock")).await?;
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
        while closed.recv().await.is_ok() {
            closing_app.next_epoch();
            if let Err(e) = hypr::focus_game().await {
                eprintln!("focus_game: {e:#}");
            }
        }
    });

    let (action_app, action_ui) = (app.clone(), ui.clone());
    let mut actions = ui.actions();
    tokio::spawn(async move {
        while let Ok(action) = actions.recv().await {
            flow::handle_action(&action_app, &action_ui, action).await;
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
            app.next_epoch();
            ui.hide();
            hypr::focus_game().await?;
            Ok("ok".into())
        }
        "status" => Ok(format!("ui clients: {}", ui.clients())),
        other => bail!("unknown command: {other}"),
    }
}
