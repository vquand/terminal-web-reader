use std::sync::Arc;

use anyhow::Result;
use regex::Regex;
use reqwest::cookie::Jar;
use reqwest::Url;

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
const SHELL: &str = "https://sangtacviet.vip/truyen/yushubo/1/134050/1/";
const CHAPTER_ENDPOINT: &str = "https://sangtacviet.vip/index.php?bookid=134050&h=yushubo&c=1&ngmar=readc&sajax=readchapter&sty=1&exts=";

#[tokio::main]
async fn main() -> Result<()> {
    let jar = Arc::new(Jar::default());
    let shell_url = Url::parse(SHELL)?;
    jar.add_cookie_str("foreignlang=vi; path=/", &shell_url);
    jar.add_cookie_str("transmode=name; path=/", &shell_url);

    let client = reqwest::Client::builder()
        .user_agent(UA)
        .cookie_provider(jar.clone())
        .build()?;

    let shell = client.get(SHELL).send().await?.text().await?;
    println!("shell bytes: {}", shell.len());

    let re = Regex::new(r#"document\.cookie\s*=\s*['"](_ac|_gac|_acx)=([^;'"]+)"#)?;
    for cap in re.captures_iter(&shell) {
        let name = &cap[1];
        let value = &cap[2];
        jar.add_cookie_str(&format!("{name}={value}; path=/"), &shell_url);
        println!("set cookie: {name}");
    }

    let r = client
        .post(CHAPTER_ENDPOINT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Referer", SHELL)
        .header("Origin", "https://sangtacviet.vip")
        .body("rescan=true&k=")
        .send()
        .await?;
    let status = r.status();
    let body = r.text().await?;
    println!("chapter status: {status}");
    println!("chapter body: {}", &body[..body.len().min(500)]);

    Ok(())
}
