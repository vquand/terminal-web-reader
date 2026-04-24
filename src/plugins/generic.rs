use anyhow::{Context, Result};
use async_trait::async_trait;
use dom_smoothie::Readability;
use scraper::{Html, Selector};
use url::Url;

use crate::plugin::{Article, RenderedPage, SitePlugin};

const USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/124.0.0.0 Safari/537.36";

pub struct GenericPlugin {
    client: reqwest::Client,
}

impl GenericPlugin {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .context("build reqwest client")?;
        Ok(Self { client })
    }
}

#[async_trait]
impl SitePlugin for GenericPlugin {
    fn name(&self) -> &'static str {
        "generic"
    }

    fn matches(&self, _url: &Url) -> bool {
        true
    }

    async fn fetch(&self, url: &Url) -> Result<RenderedPage> {
        let resp = self
            .client
            .get(url.as_str())
            .send()
            .await
            .with_context(|| format!("GET {url}"))?
            .error_for_status()?;
        let html = resp.text().await?;
        Ok(RenderedPage {
            url: url.clone(),
            html,
        })
    }

    fn extract(&self, page: &RenderedPage) -> Result<Article> {
        let mut r = Readability::new(page.html.as_str(), Some(page.url.as_str()), None)
            .context("readability init")?;
        let art = r.parse().context("readability parse")?;
        Ok(Article {
            title: (!art.title.is_empty()).then(|| art.title.to_string()),
            byline: art.byline,
            body_text: art.text_content.to_string(),
        })
    }

    fn next(&self, page: &RenderedPage) -> Option<Url> {
        find_rel_or_text(&page.html, &page.url, Direction::Next)
    }

    fn prev(&self, page: &RenderedPage) -> Option<Url> {
        find_rel_or_text(&page.html, &page.url, Direction::Prev)
    }
}

#[derive(Copy, Clone)]
enum Direction {
    Next,
    Prev,
}

impl Direction {
    fn rel(&self) -> &'static str {
        match self {
            Direction::Next => "next",
            Direction::Prev => "prev",
        }
    }

    fn text_patterns(&self) -> &'static [&'static str] {
        match self {
            Direction::Next => &["next", "下一章", "下一页", "次の", "chương sau", "sau"],
            Direction::Prev => &[
                "prev",
                "previous",
                "上一章",
                "上一页",
                "前の",
                "chương trước",
                "trước",
            ],
        }
    }
}

fn find_rel_or_text(html: &str, base: &Url, dir: Direction) -> Option<Url> {
    let doc = Html::parse_document(html);

    if let Ok(sel) = Selector::parse(&format!(r#"link[rel="{}"]"#, dir.rel())) {
        if let Some(el) = doc.select(&sel).next() {
            if let Some(href) = el.value().attr("href") {
                if let Ok(u) = base.join(href) {
                    return Some(u);
                }
            }
        }
    }

    if let Ok(sel) = Selector::parse(&format!(r#"a[rel="{}"]"#, dir.rel())) {
        if let Some(el) = doc.select(&sel).next() {
            if let Some(href) = el.value().attr("href") {
                if let Ok(u) = base.join(href) {
                    return Some(u);
                }
            }
        }
    }

    let a_sel = Selector::parse("a[href]").ok()?;
    let patterns = dir.text_patterns();
    for el in doc.select(&a_sel) {
        let text = el.text().collect::<String>().to_lowercase();
        let aria = el.value().attr("aria-label").unwrap_or("").to_lowercase();
        let title = el.value().attr("title").unwrap_or("").to_lowercase();
        let hay = format!("{text} {aria} {title}");
        if patterns.iter().any(|p| hay.contains(&p.to_lowercase())) {
            if let Some(href) = el.value().attr("href") {
                if let Ok(u) = base.join(href) {
                    return Some(u);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(html: &str) -> RenderedPage {
        RenderedPage {
            url: Url::parse("https://example.com/chapter/2").unwrap(),
            html: html.to_string(),
        }
    }

    #[test]
    fn link_rel_next_is_resolved() {
        let p = page(r#"<html><head><link rel="next" href="/chapter/3"></head></html>"#);
        let next = find_rel_or_text(&p.html, &p.url, Direction::Next).unwrap();
        assert_eq!(next.as_str(), "https://example.com/chapter/3");
    }

    #[test]
    fn a_rel_prev_is_resolved() {
        let p = page(r#"<html><body><a rel="prev" href="1">back</a></body></html>"#);
        let prev = find_rel_or_text(&p.html, &p.url, Direction::Prev).unwrap();
        assert_eq!(prev.as_str(), "https://example.com/chapter/1");
    }

    #[test]
    fn text_fallback_matches_next_chapter_link() {
        let p = page(r#"<a href="/ch3">Next Chapter</a>"#);
        let next = find_rel_or_text(&p.html, &p.url, Direction::Next).unwrap();
        assert_eq!(next.as_str(), "https://example.com/ch3");
    }

    #[test]
    fn text_fallback_matches_chinese() {
        let p = page(r#"<a href="/ch3">下一章</a>"#);
        let next = find_rel_or_text(&p.html, &p.url, Direction::Next).unwrap();
        assert_eq!(next.as_str(), "https://example.com/ch3");
    }

    #[test]
    fn no_match_returns_none() {
        let p = page(r#"<a href="/home">Home</a>"#);
        assert!(find_rel_or_text(&p.html, &p.url, Direction::Next).is_none());
    }

    #[test]
    fn extract_pulls_article_body() {
        let html = r#"<!doctype html><html><head><title>Hello</title></head>
            <body><article><h1>Hello</h1>
            <p>The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog.
            The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog.</p>
            <p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor
            incididunt ut labore et dolore magna aliqua.</p>
            </article></body></html>"#;
        let plugin = GenericPlugin::new().unwrap();
        let p = RenderedPage {
            url: Url::parse("https://example.com/post").unwrap(),
            html: html.to_string(),
        };
        let art = plugin.extract(&p).unwrap();
        assert!(art.body_text.contains("quick brown fox"));
    }
}
