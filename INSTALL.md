# Installing `twr`

## Prerequisites

| Requirement | Why | How to get it |
|---|---|---|
| Rust toolchain (≥1.85) | build the binary | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Chromium or Chrome (only for the default `js` feature) | the `SangtacvietPlugin` uses headless Chromium | Homebrew Chrome, Chromium.app, or Playwright's bundle (see below) |

If you don't need sangtacviet support, you can skip the Chromium install and
build with `--no-default-features` instead.

## Install the binary

```bash
# 1. clone
git clone <your-repo-url> terminal-web-reader
cd terminal-web-reader

# 2. install
cargo install --path .               # full build (js feature on, default)
# or:
cargo install --path . --no-default-features   # no chromium dep, loses sangtacviet
```

`cargo install` places the binary at `~/.cargo/bin/twr`. That directory is
already on PATH for most Rust setups; verify with `which twr`.

> **Common mistake:** `--path .` resolves against your current working
> directory. If you get `does not contain a Cargo.toml file`, you're not in
> the project root. Either `cd` first or pass an absolute path:
> `cargo install --path ~/GitHub/terminal-web-reader`.

## Point `twr` at Chromium

`SangtacvietPlugin` calls out to a local Chromium/Chrome binary at runtime.
`chromiumoxide` auto-detects common install paths; if detection fails or you
want to pin a specific binary, set `TWR_CHROME`.

### Option A — you already have Google Chrome

Usually auto-detected; no action needed. If not:

```bash
export TWR_CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
```

### Option B — you have Playwright's Chromium

This is the recommended path if you also ran the POC. The version directory
(`chromium-1217` below) may differ; list your cache first to find the actual
directory.

```bash
ls ~/Library/Caches/ms-playwright/   # find the chromium-NNNN directory

export TWR_CHROME="$HOME/Library/Caches/ms-playwright/chromium-1217/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
```

### Option C — install Playwright + Chromium just for this

```bash
python3 -m pip install playwright
playwright install chromium
# then use Option B to export TWR_CHROME
```

### Make it permanent

Append the export to your shell rc so new shells pick it up:

```bash
echo 'export TWR_CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"' >> ~/.zshrc
```

## Verify

```bash
twr --version
twr --print <URL>      # static site, no chromium needed
twr '<CHAPTER_URL>'    # TUI mode; exercises chromium for sites that need it
```

If the last command hangs or errors with "launch chromium" → either
`TWR_CHROME` points at a non-existent binary, or the chosen Chromium is too
old to speak the DevTools Protocol version `chromiumoxide 0.9` expects.
Update Chromium or switch to Option A.

## Where state lives

First run creates these under the platform config dir:

| Path (macOS) | Contents |
|---|---|
| `~/Library/Application Support/twr/state.toml` | history + bookmarks |
| `~/Library/Application Support/twr/twr.log`    | tracing output (TUI only) |

On Linux the prefix is `$XDG_CONFIG_HOME/twr/` (usually `~/.config/twr/`).
Both files are plain text and safe to hand-edit or delete.

## Uninstall

```bash
cargo uninstall twr
rm -rf ~/Library/Application\ Support/twr/   # optional, removes saved state
```
