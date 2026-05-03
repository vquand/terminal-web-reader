use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::network::CookieParam;
use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
use chromiumoxide::js::EvaluationResult;
use chromiumoxide::Page;
use futures::StreamExt;
use scraper::{Html, Selector};
use serde::de::DeserializeOwned;
use url::Url;

use crate::plugin::{Article, RenderedPage, SitePlugin};

const DOMAIN: &str = "sangtacviet.vip";

pub struct SangtacvietPlugin {
    chrome_executable: Option<PathBuf>,
}

impl SangtacvietPlugin {
    pub fn new() -> Self {
        let chrome_executable = std::env::var_os("TWR_CHROME").map(PathBuf::from);
        Self { chrome_executable }
    }

    fn parse_chapter_url(url: &Url) -> Option<(String, u64, u64)> {
        if url.host_str()? != DOMAIN {
            return None;
        }
        let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
        if segs.len() < 5 || segs[0] != "truyen" {
            return None;
        }
        let host = segs[1].to_string();
        let book: u64 = segs[3].parse().ok()?;
        let chap: u64 = segs[4].parse().ok()?;
        if book == 0 || chap == 0 {
            return None;
        }
        Some((host, book, chap))
    }

}

#[async_trait]
impl SitePlugin for SangtacvietPlugin {
    fn name(&self) -> &'static str {
        "sangtacviet"
    }

    fn matches(&self, url: &Url) -> bool {
        Self::parse_chapter_url(url).is_some()
    }

    async fn fetch(&self, url: &Url) -> Result<RenderedPage> {
        let mut builder = BrowserConfig::builder()
            .arg("--disable-blink-features=AutomationControlled")
            .arg("--no-first-run")
            .arg("--hide-scrollbars")
            .request_timeout(Duration::from_secs(30));
        if let Some(ref exe) = self.chrome_executable {
            builder = builder.chrome_executable(exe);
        }
        let config = builder
            .build()
            .map_err(|e| anyhow!("browser config: {e}"))?;

        let (mut browser, mut handler) = Browser::launch(config)
            .await
            .context("launch chromium; set TWR_CHROME to a Chromium/Chrome binary if autodetect fails")?;
        let handler_task = tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if h.is_err() {
                    break;
                }
            }
        });

        let origin = format!("https://{DOMAIN}/");
        let cookie_result = browser
            .set_cookies(vec![
                CookieParam::builder()
                    .name("foreignlang")
                    .value("vi")
                    .url(origin.clone())
                    .build()
                    .map_err(|e| anyhow!("cookie: {e}"))?,
                CookieParam::builder()
                    .name("transmode")
                    .value("name")
                    .url(origin)
                    .build()
                    .map_err(|e| anyhow!("cookie: {e}"))?,
            ])
            .await;

        let result = match cookie_result {
            Ok(_) => run_fetch(&mut browser, url).await,
            Err(e) => Err(anyhow!("set_cookies: {e}")),
        };

        let _ = browser.close().await;
        let _ = handler_task.await;

        result
    }

    fn extract(&self, page: &RenderedPage) -> Result<Article> {
        let doc = Html::parse_document(&page.html);
        let body_sel =
            Selector::parse(r#"[id^="cld-"], #maincontent"#).map_err(|e| anyhow!("{e:?}"))?;
        let body_el = doc
            .select(&body_sel)
            .next()
            .ok_or_else(|| anyhow!("chapter container not found"))?;
        let body_text = body_el.text().collect::<String>();
        let body_text = body_text.trim();
        if body_text.len() < 200 {
            bail!("chapter body never loaded (still showing placeholder)");
        }

        let chap_sel = Selector::parse("#bookchapnameholder").map_err(|e| anyhow!("{e:?}"))?;
        let title = doc
            .select(&chap_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty() && s != "_");

        let book_sel = Selector::parse("#booknameholder").map_err(|e| anyhow!("{e:?}"))?;
        let byline = doc
            .select(&book_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty() && s != "_");

        Ok(Article {
            title,
            byline,
            body_text: body_text.to_string(),
            next_url: find_nav_url(page, &["navnexttop", "navnextbot"]),
            prev_url: find_nav_url(page, &["navprevtop", "navprevbot"]),
        })
    }

    fn next(&self, page: &RenderedPage) -> Option<Url> {
        find_nav_url(page, &["navnexttop", "navnextbot"])
    }

    fn prev(&self, page: &RenderedPage) -> Option<Url> {
        find_nav_url(page, &["navprevtop", "navprevbot"])
    }
}

fn find_nav_url(page: &RenderedPage, ids: &[&str]) -> Option<Url> {
    let doc = Html::parse_document(&page.html);
    for id in ids {
        let Ok(selector) = Selector::parse(&format!(r#"a#{id}[href]"#)) else {
            continue;
        };
        let Some(href) = doc
            .select(&selector)
            .next()
            .and_then(|el| el.value().attr("href"))
            .map(str::trim)
            .filter(|href| !href.is_empty() && *href != "#")
        else {
            continue;
        };
        let Ok(url) = page.url.join(href) else {
            continue;
        };
        if SangtacvietPlugin::parse_chapter_url(&url).is_some() {
            return Some(url);
        }
    }

    None
}

async fn run_fetch(browser: &mut Browser, url: &Url) -> Result<RenderedPage> {
    let page = browser.new_page("about:blank").await?;

    page.evaluate_on_new_document(
        "Object.defineProperty(navigator, 'webdriver', { get: () => undefined });",
    )
    .await
    .context("install webdriver-hiding init script")?;

    page.goto(url.as_str()).await.context("navigate to chapter")?;

    wait_for_maincontent(&page).await?;
    click_maincontent(&page).await?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if tokio::time::Instant::now() > deadline {
            bail!("timed out waiting for chapter text to load");
        }
        let len: u64 = evaluate_with_context_retry(
            &page,
            r#"(() => {
                const el = document.querySelector('[id^="cld-"]') || document.getElementById('maincontent');
                const t = (el && el.innerText) || '';
                return t.length;
            })()"#,
        )
        .await
        .context("poll chapter text length")?;
        if len > 500 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let html = evaluate_with_context_retry(
        &page,
        r#"(() => {
            let retVal = '';
            if (document.doctype) {
                retVal = new XMLSerializer().serializeToString(document.doctype);
            }
            if (document.documentElement) {
                retVal += document.documentElement.outerHTML;
            }
            return retVal;
        })()"#,
    )
    .await
    .context("read rendered page content")?;

    Ok(RenderedPage {
        url: url.clone(),
        html,
    })
}

async fn evaluate_with_context_retry<T>(page: &Page, expression: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let mut last_error = None;
    for _ in 0..5 {
        match evaluate_without_context(page, expression).await {
            Ok(value) => return Ok(value),
            Err(e) if is_missing_execution_context(&e) => {
                last_error = Some(e);
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(e) => return Err(e),
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("runtime evaluation failed")))
}

async fn evaluate_without_context<T>(page: &Page, expression: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let params = EvaluateParams::builder()
        .expression(expression)
        .return_by_value(true)
        .await_promise(true)
        .build()
        .map_err(|e| anyhow!("runtime evaluate params: {e}"))?;

    let response = page.execute(params).await.context("runtime evaluate")?;
    if let Some(exception) = response.result.exception_details {
        bail!("javascript exception during runtime evaluate: {exception:?}");
    }

    EvaluationResult::new(response.result.result)
        .into_value()
        .context("deserialize runtime evaluation result")
}

fn is_missing_execution_context(error: &anyhow::Error) -> bool {
    let text = format!("{error:#}");
    text.contains("Cannot find context with specified id")
}

async fn wait_for_maincontent(page: &chromiumoxide::Page) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if tokio::time::Instant::now() > deadline {
            bail!("chapter shell did not expose #maincontent");
        }

        let has_container = page
            .evaluate(
                r#"(() => Boolean(
                    document.getElementById('maincontent') ||
                    document.querySelector('[id^="cld-"]')
                ))()"#,
            )
            .await?
            .into_value()
            .unwrap_or(false);

        if has_container {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn click_maincontent(page: &chromiumoxide::Page) -> Result<()> {
    if let Ok(el) = page.find_element("#maincontent").await {
        el.click()
            .await
            .context("click chapter container through browser input")?;
        return Ok(());
    }

    let already_loaded = page
        .evaluate(r#"(() => Boolean(document.querySelector('[id^="cld-"]')))()"#)
        .await?
        .into_value()
        .unwrap_or(false);

    if !already_loaded {
        bail!("chapter container disappeared before click");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chapter_url() {
        let u = Url::parse("https://sangtacviet.vip/truyen/yushubo/1/134050/42/").unwrap();
        let (host, book, chap) = SangtacvietPlugin::parse_chapter_url(&u).unwrap();
        assert_eq!(host, "yushubo");
        assert_eq!(book, 134050);
        assert_eq!(chap, 42);
    }

    #[test]
    fn rejects_non_chapter_paths() {
        let u = Url::parse("https://sangtacviet.vip/truyen/yushubo/1/134050/").unwrap();
        assert!(SangtacvietPlugin::parse_chapter_url(&u).is_none());
    }

    #[test]
    fn rejects_zero_chapter_ids() {
        let u = Url::parse("https://sangtacviet.vip/truyen/qidian/1/1048763609/0/").unwrap();
        assert!(SangtacvietPlugin::parse_chapter_url(&u).is_none());
    }

    #[test]
    fn rejects_other_domains() {
        let u = Url::parse("https://other.vip/truyen/yushubo/1/134050/1/").unwrap();
        assert!(SangtacvietPlugin::parse_chapter_url(&u).is_none());
    }

    #[test]
    fn next_uses_loaded_nav_href() {
        let plugin = SangtacvietPlugin::new();
        let page = RenderedPage {
            url: Url::parse("https://sangtacviet.vip/truyen/qidian/1/1048763609/898166880/")
                .unwrap(),
            html: r#"<a id="navnexttop" href="/truyen/qidian/1/1048763609/899124135/">Chương sau</a>"#
                .to_string(),
        };
        let n = plugin.next(&page).unwrap();
        assert_eq!(
            n.as_str(),
            "https://sangtacviet.vip/truyen/qidian/1/1048763609/899124135/"
        );
    }

    #[test]
    fn missing_nav_returns_none() {
        let plugin = SangtacvietPlugin::new();
        let page = RenderedPage {
            url: Url::parse("https://sangtacviet.vip/truyen/qidian/1/1048763609/898166880/")
                .unwrap(),
            html: r#"<a id="navnexttop" href="">Chương sau</a>"#.to_string(),
        };
        assert!(plugin.next(&page).is_none());
    }

    #[test]
    fn zero_nav_href_returns_none() {
        let plugin = SangtacvietPlugin::new();
        let page = RenderedPage {
            url: Url::parse("https://sangtacviet.vip/truyen/qidian/1/1048763609/898166880/")
                .unwrap(),
            html: r#"<a id="navnexttop" href="/truyen/qidian/1/1048763609/0/">Chương sau</a>"#
                .to_string(),
        };
        assert!(plugin.next(&page).is_none());
    }
}
