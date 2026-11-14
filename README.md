# Gold: a native Wayland price check for Path of Exile 1

A price check and chat-command helper for Path of Exile 1 on Hyprland: a Rust daemon plus a
Quickshell overlay. It is a port of the rules and game data of
[Awakened PoE Trade](https://github.com/SnosMe/awakened-poe-trade) (MIT) without the Electron app.

## How it works

```
Hyprland bind ──> gold ctl price-check ──> gold daemon ──> Quickshell overlay (QML, view only)
                                            │
        Ctrl+C sent to the game ────────────┤  parser, filters, trade search, poe.ninja
        item text read from the clipboard ──┘
```

- `crates/gold-core/`: the logic (parser, filters, pseudo stats, trade client, rate limiter, poe.ninja). No UI.
- `crates/gold/`: the daemon and the `gold` command. Talks to Hyprland, the clipboard and the overlay.
- `quickshell/GoldOverlay.qml`: the overlay. It draws what the daemon sends and sends clicks back.
- `assets/en/`: the English game data the daemon reads (`client_strings.js`, `items.ndjson`, `stats.ndjson`).

The overlay is a layer-shell surface that exists only while a check is open, takes keyboard focus
on demand, and only its panel takes clicks. Keys reach the game through Hyprland's `send_key_state`
(the modifier is pressed as its own key, because `send_shortcut` drops it for an XWayland window).

## Use

Run the daemon (`gold daemon`, e.g. as a user service) and add the overlay to your Quickshell
config (`GoldOverlay {}` in `shell.qml`, next to your `Theme.qml` and `StyledText.qml`).
Bind keys in Hyprland:

```lua
hl.bind("CTRL + D", hl.dsp.exec_cmd("gold ctl price-check"), { non_consuming = true })
hl.bind("F5", hl.dsp.exec_cmd("gold ctl chat /hideout --send"), { non_consuming = true })
```

- `gold ctl price-check | close | status`: control the daemon.
- `gold ctl chat <text>` pastes into the chat box; `--send` also sends it.
- `gold data update` adds items the trade API lists and the shipped data lacks (new base types)
  to `$XDG_DATA_HOME/gold/data`. Restart the daemon afterwards.

Searches default to instant buyout only (toggle it in the overlay). Config is optional, in `~/.config/gold/config.toml`:

```toml
contact = "you@example.com"   # goes into the User-Agent; GGG asks API clients to identify themselves
league = "Covens"             # default: the current challenge league
poesessid = "..."             # optional; keep the file private (chmod 600)
data_dir = "/path/to/data"    # default: the data shipped with the package (a dir containing en/)
default_currency = "divine"   # price listings in one currency (trade site id); default: any
collapse_listings = "api"     # "api" (the site hides duplicate listings) or "app" (show all)
activate_stock_filter = false # filter stacked currency by stack size
account_name = "Name"         # marks your own listings in the results
```

## Development

```
nix develop -c cargo test     # unit tests and golden tests
nix build .#default           # package (runs the tests)
scripts/dev.sh                # dev daemon and overlay next to the installed ones
```

`crates/gold-core/tests/` checks the parser, filters and trade requests against the golden files in
`tests/golden` on the items in `tests/corpus`. A corpus change needs hand-checked golden entries.
