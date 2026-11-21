//! Newline-delimited JSON link to the Quickshell overlay.
//! daemon -> UI: {"type":"show","view":{..}} | {"type":"hide"}
//! UI -> daemon: {"type":"closed"} | an `Action`, e.g. {"type":"toggle","index":3,"epoch":1}
use crate::view::View;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::broadcast;

/// What the daemon sends to the overlay.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ToUi<'a> {
    Show { view: &'a View },
    Hide,
}

/// What the overlay sends back. Every action carries the epoch of the view it was clicked in.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    /// The overlay closed itself (Esc, button).
    Closed,
    Toggle {
        epoch: u64,
        index: usize,
    },
    Set {
        epoch: u64,
        index: usize,
        min: Option<f64>,
        max: Option<f64>,
    },
    Preset {
        epoch: u64,
        index: usize,
    },
    Search {
        epoch: u64,
    },
    Hidden {
        epoch: u64,
    },
    More {
        epoch: u64,
    },
    Trade {
        epoch: u64,
        key: TradeKey,
    },
}

/// The trade filters the overlay can flip.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TradeKey {
    MagicOnly,
}

impl Action {
    pub const fn epoch(&self) -> Option<u64> {
        match self {
            Self::Closed => None,
            Self::Toggle { epoch, .. }
            | Self::Set { epoch, .. }
            | Self::Preset { epoch, .. }
            | Self::Search { epoch }
            | Self::Hidden { epoch }
            | Self::More { epoch }
            | Self::Trade { epoch, .. } => Some(*epoch),
        }
    }
}

#[derive(Clone)]
pub struct Ui {
    out: broadcast::Sender<String>,
    closed: broadcast::Sender<()>,
    actions: broadcast::Sender<Action>,
}

impl Ui {
    pub fn start(path: &Path) -> Result<Self> {
        drop(std::fs::remove_file(path));
        let listener = UnixListener::bind(path)?;
        let ui = Self {
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
                                Err(broadcast::error::RecvError::Lagged(_)) => {}
                                Err(broadcast::error::RecvError::Closed) => break,
                            }
                        }
                    });
                    let mut lines = BufReader::new(r).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        // anything that doesn't parse (unknown type, missing epoch) is ignored
                        match serde_json::from_str::<Action>(&line) {
                            Ok(Action::Closed) => {
                                drop(ui.closed.send(()));
                            }
                            Ok(action) => {
                                drop(ui.actions.send(action));
                            }
                            Err(_) => {}
                        }
                    }
                    writer.abort();
                });
            }
        });
        Ok(ui)
    }

    fn send(&self, msg: &ToUi) {
        if let Ok(json) = serde_json::to_string(msg) {
            drop(self.out.send(json));
        }
    }

    pub fn show(&self, view: &View) {
        self.send(&ToUi::Show { view });
    }

    pub fn hide(&self) {
        self.send(&ToUi::Hide);
    }

    /// Everything the overlay sends except "closed": toggles, edits, search requests.
    pub fn actions(&self) -> broadcast::Receiver<Action> {
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
