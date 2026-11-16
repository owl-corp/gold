# Gold

A native Wayland/Hyprland price checker for Path of Exile 1. Press a hotkey while hovering an item to get its price from the trade site and poe.ninja.

## Install

Only supported under Hyprland and [Quickshell](https://quickshell.org). With Nix:

```
nix profile install github:owl-corp/gold
```

1. Run the daemon: `gold daemon` (for example as a user service).
2. Add the overlay to your Quickshell config: copy `share/gold/quickshell/GoldOverlay.qml` from the package next to your `Theme.qml` and `StyledText.qml`, and add `GoldOverlay {}` to `shell.qml`.
3. Bind keys in Hyprland:

```lua
hl.bind("CTRL + D", hl.dsp.exec_cmd("gold ctl price-check"), { non_consuming = true })
hl.bind("F5", hl.dsp.exec_cmd("gold ctl chat /hideout --send"), { non_consuming = true })
```

## Use

With the game focused, press the price check key while hovering an item. Click the filters to
turn them on or off, then press Search.

- `gold ctl price-check | close | status`: control the daemon.
- `gold ctl chat <text>`: paste into the chat box; `--send` also sends it.
- `gold data update`: add items the trade API lists that the shipped data lacks (new base types).
  Restart the daemon afterwards.

## Config

Optional, in `~/.config/gold/config.toml`:

```toml
contact = "you@example.com"   # goes into the User-Agent; GGG asks API clients to identify themselves
league = "Covens"             # default: the current challenge league
poesessid = "..."             # optional; keep the file private (chmod 600)
default_currency = "divine"   # price listings in one currency (trade site id); default: any
collapse_listings = "api"     # "api" (the site hides duplicate listings) or "app" (show all)
activate_stock_filter = false # filter stacked currency by stack size
account_name = "Name"         # marks your own listings in the results
```

Not supported yet: expedition logbooks and timeless jewels.

See [CONTRIBUTING.md](CONTRIBUTING.md) for how it works and how to develop it.
