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
twr https://sangtacviet.vip/truyen/yushubo/1/134050/1/   # open a chapter
twr --resume                                              # continue where you left off
twr --bookmark yushubo-saved                              # jump to a bookmark
twr --list-bookmarks                                      # list saved bookmarks
twr --print https://example.com/                          # stdout, no TUI
twr --print --follow 3 https://example.com/chapter/1      # stdout + 3 more chapters
```

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

Each site gets its own plugin implementing the `SitePlugin` trait:

- `GenericPlugin` — static `reqwest` fetch, `dom_smoothie` readability extraction,
  rel=next/prev plus text heuristics in EN/CN/JP/VN.
- `SangtacvietPlugin` — headless Chromium via `chromiumoxide`. Hides
  `navigator.webdriver`, pre-seeds a `foreignlang=vi` cookie to skip the site's
  language modal, clicks `#maincontent` to trigger the chapter XHR, waits for
  `innerText > 500` chars, and reads the rendered DOM. Next/prev is URL
  arithmetic on the `/truyen/<host>/1/<book>/<chap>/` path.

Adding a new site is a small Rust file — see [`docs/plugins.md`](docs/plugins.md).

## Development

```bash
cargo test                    # 16 unit tests across generic/sangtacviet/state
cargo test --no-default-features   # without the js feature (11 tests excluded)
cargo run -- --print https://example.com/
```

[`chromiumoxide`]: https://github.com/mattsse/chromiumoxide
