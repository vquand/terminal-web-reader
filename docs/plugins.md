# Writing a site plugin

Each plugin is a Rust type that implements the `SitePlugin` trait defined in
[`src/plugin.rs`](../src/plugin.rs). The dispatcher (`Registry::resolve`) picks
the first plugin whose `matches(url)` returns `true` — so specific plugins get
registered first in [`build_registry()`](../src/main.rs) and `GenericPlugin`
catches everything that nothing else claimed.

## The trait

```rust
#[async_trait]
pub trait SitePlugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn matches(&self, url: &Url) -> bool;
    async fn fetch(&self, url: &Url) -> Result<RenderedPage>;
    fn extract(&self, page: &RenderedPage) -> Result<Article>;
    fn next(&self, page: &RenderedPage) -> Option<Url>;
    fn prev(&self, page: &RenderedPage) -> Option<Url>;
}
```

- `matches` — cheap URL check. Host/path-prefix is usually enough.
- `fetch` — return the raw HTML (whatever "raw" means for that site — after
  JS runs, after an XHR resolves, whatever).
- `extract` — pull `{ title, byline, body_text }` out of the HTML.
- `next` / `prev` — return the chapter URLs on either side. The argument is the
  already-fetched page, so you can scrape nav selectors from it, but most sites
  have predictable URL structure and pure math works.

## Two reference implementations

### Static + readability — `src/plugins/generic.rs`

```rust
async fn fetch(&self, url: &Url) -> Result<RenderedPage> {
    let html = self.client.get(url.as_str()).send().await?.text().await?;
    Ok(RenderedPage { url: url.clone(), html })
}

fn extract(&self, page: &RenderedPage) -> Result<Article> {
    let mut r = Readability::new(page.html.as_str(), Some(page.url.as_str()), None)?;
    let art = r.parse()?;
    Ok(Article {
        title: (!art.title.is_empty()).then(|| art.title.to_string()),
        byline: art.byline,
        body_text: art.text_content.to_string(),
    })
}
```

`next` / `prev` walk the document looking for `<link rel="next">`, `<a rel="next">`,
then fall back to text matching on common next-chapter strings in several languages.

### Headless + click — `src/plugins/sangtacviet.rs`

Some sites gate content behind a user-gesture + JS pipeline no static fetch can
reproduce. Sangtacviet is one: the obfuscated in-page IIFE mutates hidden state
that's validated server-side. Headless Chromium is the only option.

```rust
let (mut browser, mut handler) = Browser::launch(config).await?;
// drive the handler task in the background …

browser.set_cookies(vec![/* foreignlang=vi to skip the modal */]).await?;

let page = browser.new_page("about:blank").await?;
page.evaluate_on_new_document(
    "Object.defineProperty(navigator, 'webdriver', { get: () => undefined });",
).await?;
page.goto(url.as_str()).await?;
tokio::time::sleep(Duration::from_secs(3)).await;  // let handler scripts bind
page.find_element("#maincontent").await?.click().await?;

// poll until innerText > 500 chars, then scrape the page HTML
```

## Adding your own

1. Create `src/plugins/mysite.rs` with a struct + `impl SitePlugin`.
2. Export it from `src/plugins/mod.rs`.
3. Register it ahead of `GenericPlugin` in `build_registry()` so `matches()`
   wins the dispatch.
4. Add unit tests for `matches`, `next`, `prev` using offline URL fixtures —
   these are the bits the compiler can't catch.

## Debugging tips

- Run the POC pipeline first. Drive the site with Playwright (see
  [`poc/sangtacviet_probe.py`](../poc/sangtacviet_probe.py)) and capture the
  DOM + XHR trace into `poc/out/` before writing Rust. If the site needs a
  click, make sure the click actually loads the content in a real browser
  before wiring chromiumoxide — it's faster to iterate in Python.
- `cargo run -- --print <url>` prints the extracted text to stdout with
  tracing on stderr — handy for dumping the body and confirming your
  extractor works.
- TUI logs go to `~/Library/Application Support/twr/twr.log`. Tail it in
  another pane while you poke at keybindings.
- For anti-bot-protected sites: if `reqwest` gets `code:5/4002`-style errors
  that match the browser flow's input exactly, the JS is mutating hidden
  state. Don't reverse-engineer — reach for chromiumoxide.
