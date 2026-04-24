# `sangtacviet` plugin

Handles chapters on [sangtacviet.vip](https://sangtacviet.vip/). Requires the
`js` feature (default) because the site's chapter body is loaded by JavaScript
after a user-gesture click, and the `readchapter` XHR it fires is gated by
server-side state we can't reproduce from plain HTTP. See
[`poc/out/selectors.md`](../../poc/sangtacviet_probe.py) for the full probe
findings.

## URL shape

```
https://sangtacviet.vip/truyen/<host>/1/<book-id>/<chapter>/
```

- `<host>` — source site key (e.g. `yushubo`, `sangtac`, `dich`, `qidian`).
- `<book-id>` — numeric book id as shown in the sangtacviet UI.
- `<chapter>` — 1-indexed chapter number.

Any URL matching that shape is claimed by the plugin; anything else falls
through to `GenericPlugin`.

## Usage

```bash
twr '<CHAPTER_URL>'             # open in TUI
twr --print '<CHAPTER_URL>'     # dump extracted text to stdout
twr --resume                    # reopen last-read sangtacviet chapter
twr --print --follow 3 '<CHAPTER_URL>'   # fetch this chapter + 3 more
```

Inside the TUI:

| Key | Action |
|---|---|
| `→` / `l` | next chapter (URL arithmetic, no page scrape) |
| `←` / `h` | previous chapter |
| `b` | bookmark this chapter (auto-named `sangtacviet.vip/<chapter>`) |

## What the plugin does internally

1. Launches headless Chromium with `--disable-blink-features=AutomationControlled`
   (set `TWR_CHROME` to override the auto-detected binary).
2. Injects a pre-document script that hides `navigator.webdriver` — the site's
   obfuscated IIFE reads this flag and will sabotage `XMLHttpRequest.send` if
   it sees automation.
3. Pre-seeds cookies `foreignlang=vi` and `transmode=name` on the domain so
   the language-selection modal never appears on first load.
4. Navigates to the chapter URL, waits ~3 seconds for `stv.readinit.js` to
   bind its click handler on `#maincontent`, then synthesizes a click.
5. Polls until `#maincontent` (renamed to `cld-<book>-<chapter>` after load)
   contains >500 characters, or 30 seconds have elapsed.
6. Grabs the rendered DOM and extracts chapter text + book/chapter titles.

Navigation (`next`/`prev`) is pure URL arithmetic: increment or decrement
the last path segment, clamp at chapter 1. No second fetch needed.

## Known quirks

- **First click after startup sometimes throttles.** The first `readchapter`
  XHR occasionally comes back as `{"code":"7","time":30}` — the plugin polls
  past this and the second attempt usually succeeds within the 30 s timeout.
  If it times out, try again; headless Chromium launch is the slow part and
  the second run reuses a warmed-up cache.
- **Chapter 0 / index pages are not supported.** URLs ending at the book id
  (`/truyen/<host>/1/<book-id>/`) drop through to `GenericPlugin`, which
  renders the table-of-contents page readably but without the chapter-nav
  keys.
- **Site-side URL redirects are not followed.** If sangtacviet ever renames
  `<host>`, the plugin's `matches()` predicate won't care, but pre-saved
  bookmarks pointing at the old URL will still open them via the old host.
