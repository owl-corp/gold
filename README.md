# Gold: a native Wayland price check for Path of Exile 1

A price check and chat-command helper for Path of Exile 1 on Hyprland.

The parser, stat matching, trade queries and pseudo-stat rules are checked against golden files on a corpus of items (see "Testing").

## How it works

```
Hyprland bind ──> gold ctl price-check ──> gold daemon ──> Quickshell overlay (QML, view only)
                                            │
        Ctrl+C sent to the game ────────────┤  parser, filters, trade search, poe.ninja
        item text read from the clipboard ──┘
```

- `gold-core/`: the logic (parser, filters, trade client, rate limiter, poe.ninja). No UI.
- `gold/`: the daemon and the `gold ctl` command. Talks to Hyprland, the clipboard and the overlay.
- `quickshell/GoldOverlay.qml`: the overlay. It draws what the daemon sends and sends clicks back.

The game is fullscreen. The overlay is a layer-shell surface on the overlay layer that exists only
while a check is open, takes keyboard focus on demand, and only its panel takes clicks.
Keys reach the game through Hyprland's `send_key_state` (the modifier is pressed as its own key,
because `send_shortcut` drops it for this XWayland window).

## Use

Run the daemon (the nixcfg module does this as a user service) and add the overlay to your
Quickshell config (`GoldOverlay {}` in `shell.qml`, next to `Theme.qml` and `StyledText.qml`).
Bind keys in Hyprland:

```lua
hl.bind("CTRL + D", hl.dsp.exec_cmd("gold ctl price-check"), { non_consuming = true })
hl.bind("F5", hl.dsp.exec_cmd("gold ctl chat /hideout --send"), { non_consuming = true })
```

`gold ctl chat <text>` pastes into the chat box; `--send` also sends it.

Config is optional, in `~/.config/gold/config.toml`:

```toml
contact = "you@example.com"   # goes into the User-Agent; GGG asks API clients to identify themselves
league = "Covens"             # default: the current challenge league
poesessid = "..."             # optional; keep the file private (chmod 600)
data_dir = "/path/to/data"    # default: the data shipped with the package
```

## Testing

```
cargo test
```

`gold-core/tests/` checks the Rust parser, filters and trade requests against the golden files in `tests/golden` on the items in `tests/corpus`. A corpus change needs hand-checked golden entries.
