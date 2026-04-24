# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                             # default features (js on → pulls chromiumoxide)
cargo build --no-default-features       # minimal build without headless browser
cargo test                              # 16 tests (11 without js feature)
cargo test <module>::tests::<name>      # e.g. plugins::sangtacviet::tests::parses_chapter_url

# Run against a chapter URL (headless browser):
TWR_CHROME="$HOME/Library/Caches/ms-playwright/chromium-1217/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" \
  cargo run -- '<CHAPTER_URL>'

cargo run -- --print <URL>   # stdout mode, skips the TUI
```

`TWR_CHROME` is read by `SangtacvietPlugin`; unset → chromiumoxide auto-detects. Without the `js` feature, the binary still builds but has no sangtacviet support and never touches chromium.

## Architecture

**Plugin-first, not "generic with overrides".** Every site is assumed to need its own handling. `SitePlugin` (in `src/plugin.rs`) is the one abstraction the rest of the app depends on:

```rust
async fn fetch(&self, url: &Url) -> Result<RenderedPage>;
fn extract(&self, page: &RenderedPage) -> Result<Article>;
fn next/prev(&self, page: &RenderedPage) -> Option<Url>;
```

Dispatch is first-match via `Registry::resolve` — specific plugins register before `GenericPlugin` in `build_registry()` (see `src/main.rs`). `GenericPlugin` is the catch-all (reqwest + `dom_smoothie` readability + rel=next/prev heuristics).

`SangtacvietPlugin` is **only compiled with `--features js`** (gated in `src/plugins/mod.rs`). When `js` is off, the plugin file isn't included; registry just has `GenericPlugin`.

**Why some sites need chromiumoxide.** Sangtacviet validates session state via a JS-computed value we couldn't reproduce from pure reqwest. The POC (`poc/sangtacviet_probe.py`) confirmed this: equal cookies + equal headers → browser gets `{"code":"0"}`, scripted client gets `{"code":"5","err":"mã 4002"}`. Rule of thumb from `docs/plugins.md`: if a cookie/header replay yields different results from the browser, stop reverse-engineering and use chromiumoxide.

**TUI state machine** (`src/app.rs`). Three modes: `Loading { rx: oneshot::Receiver }` → `Reading { scroll }` → `Error`. Fetches run in `tokio::spawn`; `App::poll_fetch` is called on every tick and advances the state on channel recv. Arrow keys call `start_fetch` with a new URL (scroll=0); scroll keys mutate `Mode::Reading::scroll` directly.

**Boss-key overlay** (`src/bosskey.rs`) swaps the reader for a fake production log pane on Ctrl-B. `Option<BossState>` field on `App`; when `Some`, `ui::render` short-circuits to the overlay and the key handler swallows everything except Ctrl-B itself so scroll/nav keystrokes don't leak into the hidden reader.

**Persistence** (`src/state.rs`) is keyed **per-domain**, not per-URL. One history entry per host → `{ url, scroll, saved_at }`. Scroll restore only fires when the URL matches exactly. `--resume` picks the entry with the highest `saved_at`. Re-reading the same book at a different chapter starts fresh (intentional — prevents stale scroll offsets after content changes).

**Tracing destinations are split by mode.** TUI mode writes to `~/Library/Application Support/twr/twr.log` (alternate-screen buffer would clobber stderr output). `--print` and `--list-bookmarks` keep stderr. See `init_tracing` in `src/main.rs`.

**Defaults.** `EnvFilter` default is `"warn,chromiumoxide::handler=error,chromiumoxide::browser=error"` — chromiumoxide's CDP parser emits many `WS Invalid message` warnings that are noise and would flood the log file if not downgraded.

## macOS-specific gotcha

Apple's system `curl` is linked against **LibreSSL** and silently fails TLS handshakes against some Cloudflare zones (including sangtacviet). Rust's `rustls`-backed `reqwest` works fine; Playwright/Chromium works fine. If debugging a "site seems unreachable" issue, try `openssl s_client` or a direct Rust probe before concluding the site is blocked — it's almost always just LibreSSL.

## Plugin authoring

See `docs/plugins.md` for the full guide. Critical points not obvious from reading the trait:

- For headless plugins, cookies must be pre-seeded via `Browser::set_cookies` with `cookie.url` set — `Page::set_cookies` validates the current page URL against `about:blank` and rejects.
- `navigator.webdriver` must be hidden *before* page scripts run: use `page.evaluate_on_new_document()`, not a post-goto `evaluate`.
- Wait for the click handler to bind (`stv.readinit.js` took ~3s for sangtacviet) before synthesizing the click; otherwise the click fires but no handler catches it.
