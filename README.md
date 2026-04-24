# terminal-web-reader (`twr`)

Terminal web reader with reader-mode extraction and per-site plugins. Renders
inside a disguised-as-code ratatui pane — the kind of thing that passes for
"checking logs" when your manager walks past.

## Install

See [`INSTALL.md`](INSTALL.md) for the full walkthrough, including how to
point `twr` at a Chromium binary. Short version:

```bash
cd terminal-web-reader
cargo install --path .                       # full build
# or: cargo install --path . --no-default-features   # strip the headless browser
export TWR_CHROME="/path/to/Chromium"        # only needed if auto-detect fails
```

## Usage

```bash
twr <URL>                             # open a chapter / article
twr --resume                          # continue where you left off
twr --bookmark <NAME>                 # jump to a saved bookmark
twr --list-bookmarks                  # list saved bookmarks
twr --print <URL>                     # stdout, no TUI
twr --print --follow 3 <URL>          # stdout + 3 more chapters via `next` links
```

For the per-site URL shapes, quirks, and requirements of each shipping plugin,
see [`docs/plugins/`](docs/plugins/).

## Keybindings

| Key            | Action                                     |
|----------------|--------------------------------------------|
| `←` / `h`      | previous chapter                           |
| `→` / `l`      | next chapter                               |
| `↑` / `↓` / `j` / `k` | scroll one line                     |
| `Space` / `PgDn` / `PgUp` | scroll one page                  |
| `g` / `Home`   | jump to top                                |
| `G` / `End`    | jump to bottom                             |
| `b`            | bookmark current chapter                   |
| `r`            | reload current chapter                     |
| **`Ctrl-B`**   | **boss key** — swap for a fake log pane    |
| `q` / `Ctrl-C` | quit (saves position)                      |

## Persistence

Everything lives in the platform config dir (`~/Library/Application Support/twr/`
on macOS, `$XDG_CONFIG_HOME/twr/` on Linux):

- `state.toml` — per-domain history entry (`{ url, scroll, saved_at }`) + named bookmarks
- `twr.log` — tracing output (TUI mode only; `--print` mode writes to stderr)

Scroll offset is restored only when re-opening the exact same URL — re-opening
the same book at a different chapter starts fresh at line 0.

## How it works

Each site gets its own plugin implementing the `SitePlugin` trait. Dispatch
is first-match, so specific plugins register before the `generic` catch-all.

- Per-plugin usage docs: [`docs/plugins/`](docs/plugins/)
- How to write your own: [`docs/writing-plugins.md`](docs/writing-plugins.md)

## Development

```bash
cargo test                    # 16 unit tests across generic/sangtacviet/state
cargo test --no-default-features   # without the js feature (11 tests excluded)
cargo run -- --print https://example.com/
```

[`chromiumoxide`]: https://github.com/mattsse/chromiumoxide
