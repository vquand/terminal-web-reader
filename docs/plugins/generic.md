# `generic` plugin

Catch-all fallback. `matches()` is `true` for every URL, so this plugin runs
whenever no site-specific plugin has claimed the URL.

## What it handles well

- Static HTML articles (blog posts, docs, news sites).
- Pages that declare chapter navigation via `<link rel="next">` /
  `<link rel="prev">` or `<a rel="next">` / `<a rel="prev">`.
- Pages with conventional "next chapter" / "previous chapter" anchor text in
  English, Chinese (`下一章` / `上一章`), Japanese (`次の` / `前の`), or
  Vietnamese (`chương sau` / `chương trước`).

## What it does not handle

- JavaScript-rendered content. `reqwest` pulls the HTML as the server sends
  it; if the article body is populated by client-side JS, `dom_smoothie` will
  extract whatever fallback text the shell contains (often empty or a
  placeholder).
- Sites with anti-bot TLS filtering that reject non-browser clients. If
  `twr --print <URL>` returns a challenge page, you need a site-specific
  plugin using `chromiumoxide`.

## Under the hood

1. `reqwest` GET with a Chrome-like User-Agent, rustls TLS stack.
2. `dom_smoothie::Readability` parses and scores the DOM, returning a
   cleaned `Article { title, byline, text_content }`.
3. `next` / `prev` walk the document looking for `rel="next"` / `rel="prev"`
   on `<link>` and `<a>` first, then fall back to text/aria-label matching
   against a multilingual keyword list.

No keyboard shortcut is needed to trigger it — just pass any URL that isn't
claimed by another plugin.
