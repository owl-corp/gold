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
fn runtime_dir() -> Result<PathBuf> {
    let d = config::xdg_runtime_dir()?.join(if std::env::var_os("GOLD_DEV").is_some() {
        "gold-dev"
    } else {
        "gold"
    });
    if let Err(e) = std::fs::create_dir_all(&d) {
        eprintln!("cannot create {}: {e}", d.display());
    }
    Ok(d)
}

/// Removes a socket file a previous run left behind; one that is already gone is fine.
fn remove_stale(path: &std::path::Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!("cannot remove {}: {e}", path.display());
        }
    }
}

/// The dev daemon's control socket, if one is running (a stale socket file does not answer).
async fn connect_dev() -> Option<UnixStream> {
    let dir = config::xdg_runtime_dir().ok()?.join("gold-dev");
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
                None => UnixStream::connect(runtime_dir()?.join("ctl.sock")).await?,
            };
            s.write_all(format!("{}\n", cmd.join(" ")).as_bytes()).await?;
            let mut reply = String::new();
            BufReader::new(s).read_line(&mut reply).await?;
            print!("{reply}");
            Ok(())
        }
        Some("data") if args.next().as_deref() == Some("update") => update_data().await,
        _ => bail!("usage: gold <daemon|ctl ...|data update>"),
    }
}

/// Writes `lines` to `<dir>/<name>` through a temporary file.
fn write_lines(dir: &std::path::Path, name: &str, lines: &[String]) -> Result<std::path::PathBuf> {
    let target = dir.join(name);
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, lines.iter().flat_map(|l| [l.as_str(), "\n"]).collect::<String>())?;
    std::fs::rename(&tmp, &target)?;
    Ok(target)
}

/// `gold data update`: add the items and stats the trade API lists and the shipped data lacks.
async fn update_data() -> Result<()> {
    use gold_core::data_update::{extra_items, extra_stats};
    let config = config::Config::load();
    let contact = config.contact.clone().unwrap_or_else(|| "unset".into());
    let client = gold_core::http::client(&contact, None)?;
    let fetch = |what: &str| {
        client
            .get(format!("https://www.pathofexile.com/api/trade/data/{what}"))
            .send()
    };
    let items = fetch("items").await?.error_for_status()?.text().await?;
    let statics = fetch("static").await?.error_for_status()?.text().await?;
    let stats = fetch("stats").await?.error_for_status()?.text().await?;
    let shipped = |name: &str| std::fs::read_to_string(config.data_dir().join("en").join(name));
    let extra = extra_items(&items, &statics, &shipped("items.ndjson")?)?;
    let extra_stats = extra_stats(&stats, &shipped("stats.ndjson")?)?;

    let dir = config::extra_data_dir()
        .ok_or_else(|| anyhow::anyhow!("no HOME or XDG_DATA_HOME"))?
        .join("en");
    std::fs::create_dir_all(&dir)?;
    let target = write_lines(&dir, gold_core::data::EXTRA_ITEMS, &extra.lines)?;
    println!(
        "{}: {} items, {} uniques ({} uniques skipped, base type unknown; {} entries of unmapped groups).",
        target.display(),
        extra.bases,
        extra.uniques,
        extra.skipped_uniques,
        extra.unmapped
    );
    let target = write_lines(&dir, gold_core::data::EXTRA_STATS, &extra_stats.lines)?;
    println!(
        "{}: {} new stats, {} shipped stats extended ({} skipped). Restart the daemon to load them.",
        target.display(),
        extra_stats.added,
        extra_stats.patched,
        extra_stats.skipped
    );
    Ok(())
}

async fn daemon() -> Result<()> {
    // A second daemon would take over both sockets and leave the first one orphaned.
    if UnixStream::connect(runtime_dir()?.join("ctl.sock")).await.is_ok() {
        bail!("an gold daemon is already running");
    }
    let app = Arc::new(App::new(config::Config::load())?);
    let ui = ui::Ui::start(&runtime_dir()?.join("ui.sock"))?;

    let path = runtime_dir()?.join("ctl.sock");
    remove_stale(&path);
    let listener = UnixListener::bind(&path)?;
    eprintln!("gold daemon ready");

    // Overlay closed by the UI (Esc, button): hand the keyboard back to the game.
    let mut closed = ui.closed();
    let closing_app = app.clone();
    tokio::spawn(async move {
        loop {
            match closed.recv().await {
                Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => {}
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
                Ok(action) => flow::handle_action(&action_app, &action_ui, &action),
                Err(broadcast::error::RecvError::Lagged(_)) => {}
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
                if let Err(e) = w.write_all(format!("{reply}\n").as_bytes()).await {
                    eprintln!("cannot reply to the client: {e}");
                }
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
        other => {
            let Some(text) = other.strip_prefix("chat ") else {
                bail!("unknown command: {other}");
            };
            if hypr::active_class().await? != hypr::POE_CLASS {
                bail!("Path of Exile is not the focused window");
            }
            let text = text.trim_end();
            let (text, send) = text.strip_suffix(" --send").map_or((text, false), |t| (t, true));
            chat::type_in_chat(text, send).await?;
            Ok("ok".into())
        }
    }
}
