//! Newline-delimited JSON link to the Quickshell overlay.
//! daemon -> UI: {"type":"show","view":{..}} | {"type":"hide"}
//! UI -> daemon: {"type":"closed"} | any other action, e.g. {"type":"toggle","index":3}
use anyhow::Result;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct Ui {
    out: broadcast::Sender<String>,
    closed: broadcast::Sender<()>,
    actions: broadcast::Sender<serde_json::Value>,
}

impl Ui {
    pub fn start(path: PathBuf) -> Result<Self> {
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        let ui = Ui {
            out: broadcast::channel(64).0,
            closed: broadcast::channel(8).0,
            actions: broadcast::channel(32).0,
        };
        let me = ui.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let ui = me.clone();
                tokio::spawn(async move {
                    let (r, mut w) = stream.into_split();
                    let mut rx = ui.out.subscribe();
                    let writer = tokio::spawn(async move {
                        loop {
                            match rx.recv().await {
                                Ok(msg) => {
                                    if w.write_all(format!("{msg}\n").as_bytes()).await.is_err() {
                                        break;
                                    }
                                }
                                // a slow client skips ahead; every view is complete, so the latest wins
                                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                                Err(broadcast::error::RecvError::Closed) => break,
                            }
                        }
                    });
                    let mut lines = BufReader::new(r).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let msg: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
                        if msg["type"] == "closed" {
                            let _ = ui.closed.send(());
                        } else if msg["type"].is_string() {
                            let _ = ui.actions.send(msg);
                        }
                    }
                    writer.abort();
                });
            }
        });
        Ok(ui)
    }

    pub fn show(&self, view: serde_json::Value) {
        let _ = self
            .out
            .send(serde_json::json!({ "type": "show", "view": view }).to_string());
    }

    pub fn hide(&self) {
        let _ = self.out.send(r#"{"type":"hide"}"#.into());
    }

    /// Everything the overlay sends except "closed": toggles, edits, search requests.
    pub fn actions(&self) -> broadcast::Receiver<serde_json::Value> {
        self.actions.subscribe()
    }

    pub fn closed(&self) -> broadcast::Receiver<()> {
        self.closed.subscribe()
    }

    /// Connected overlays: each one's writer task holds a receiver.
    pub fn clients(&self) -> usize {
        self.out.receiver_count()
    }
}
