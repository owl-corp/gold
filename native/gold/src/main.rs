mod app;
mod clipboard;
mod config;
mod hypr;
mod ui;
mod view;

use anyhow::{bail, Result};
use app::App;
use gold_core::parser::parse_clipboard;
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
            price_check(app, ui).await?;
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

async fn price_check(app: &Arc<App>, ui: &ui::Ui) -> Result<()> {
    if ui.clients() == 0 {
        bail!("no overlay connected (is the Quickshell overlay running?)");
    }
    if hypr::active_class().await? != hypr::POE_CLASS {
        bail!("Path of Exile is not the focused window");
    }
    let epoch = app.next_epoch();
    let text = clipboard::copy_item().await?;

    let cursor = hypr::cursor_pos().await?;
    let monitor = hypr::monitors().await?.into_iter().find(|m| m.focused);
    let (mx, my, screen) = monitor.map(|m| (m.x, m.y, m.name)).unwrap_or((0, 0, String::new()));
    let place = view::Placement {
        screen,
        x: cursor.x - mx,
        y: cursor.y - my,
    };

    let item = match parse_clipboard(&app.data, &text) {
        Ok(item) => item,
        Err(e) => {
            let mut lines = vec![e.code().to_string()];
            lines.extend(text.lines().take(12).map(String::from));
            ui.show(view::error_view(&place, "Item not recognised", lines));
            return Ok(());
        }
    };

    ui.show(view::item_view(&place, &item, None));

    let (app, ui) = (app.clone(), ui.clone());
    tokio::spawn(async move {
        let ninja = app.ninja().await.map_err(|e| format!("{e:#}"));
        if app.is_current(epoch) {
            ui.show(view::item_view(&place, &item, Some(&ninja)));
        }
    });
    Ok(())
}
