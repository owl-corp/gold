//! `~/.config/gold/config.toml`. Hotkeys are not here: they live in the Hyprland config.
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    /// How GGG can reach you if the client misbehaves; goes into the User-Agent.
    pub contact: Option<String>,
    /// Trade league. Default: the current challenge league.
    pub league: Option<String>,
    /// Directory holding `<lang>/{items,stats}.ndjson` and `client_strings.js`.
    pub data_dir: Option<PathBuf>,
    /// Session cookie, only needed for searches the site restricts to logged-in users.
    pub poesessid: Option<String>,
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("gold/config.toml"))
}

impl Config {
    pub fn load() -> Config {
        let Some(path) = config_path() else {
            return Config::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("{}: {e}", path.display());
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }

    /// Config, then `$GOLD_DATA_DIR`, then the data next to the installed binary,
    /// then the checkout this was built from.
    pub fn data_dir(&self) -> PathBuf {
        if let Some(d) = &self.data_dir {
            return d.clone();
        }
        if let Some(d) = std::env::var_os("GOLD_DATA_DIR") {
            return d.into();
        }
        if let Some(d) = std::env::current_exe()
            .ok()
            .and_then(|e| Some(e.parent()?.parent()?.join("share/gold/data")))
        {
            if d.is_dir() {
                return d;
            }
        }
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"))
    }
}
