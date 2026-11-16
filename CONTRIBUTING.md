# Contributing

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

## Development

```
nix develop -c cargo test     # unit tests and golden tests
nix build .#default           # package (runs the tests)
scripts/dev.sh                # dev daemon and overlay next to the installed ones
```

`crates/gold-core/tests/` checks the parser, filters and trade requests against the golden files in
`tests/golden` on the items in `tests/corpus`. A corpus change needs hand-checked golden entries.

Config option `data_dir` (a directory containing `en/`) overrides the shipped game data.
