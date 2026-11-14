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
    /// Price listings in this currency (a trade site currency id such as "divine"); default: any.
    pub default_currency: Option<String>,
    /// "api" (the trade site collapses duplicate listings, default) or "app" (no collapsing).
    pub collapse_listings: Option<String>,
    /// Turn on the stack size filter for stacked currency items.
    pub activate_stock_filter: bool,
    /// Your account name; your own listings are marked in the results.
    pub account_name: Option<String>,
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
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Config::default();
        };
        let config: Config = toml::from_str(&text).unwrap_or_else(|e| {
            // toml errors quote the offending line, which may hold the session id
            eprintln!(
                "{}: invalid config ({})",
                path.display(),
                e.span()
                    .map_or("unknown position".into(), |s| format!("bytes {}-{}", s.start, s.end))
            );
            Config::default()
        });
        if config.poesessid.is_some() && is_readable_by_others(&path) {
            eprintln!(
                "warning: {} holds your session id; run `chmod 600` on it",
                path.display()
            );
        }
        config
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
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets"))
    }
}

/// Where `gold data update` writes the extra data: `$XDG_DATA_HOME/gold/data`.
pub fn extra_data_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("gold/data"))
}

fn is_readable_by_others(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o077 != 0)
}
