use anyhow::Result;
use async_trait::async_trait;
use url::Url;

#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub url: Url,
    pub html: String,
}

#[derive(Debug, Clone)]
pub struct Article {
    pub title: Option<String>,
    pub byline: Option<String>,
    pub body_text: String,
    pub next_url: Option<Url>,
    pub prev_url: Option<Url>,
}

#[async_trait]
pub trait SitePlugin: Send + Sync {
    fn name(&self) -> &'static str;

    fn matches(&self, url: &Url) -> bool;

    async fn fetch(&self, url: &Url) -> Result<RenderedPage>;

    fn extract(&self, page: &RenderedPage) -> Result<Article>;

    fn next(&self, page: &RenderedPage) -> Option<Url>;

    fn prev(&self, page: &RenderedPage) -> Option<Url>;
}

pub struct Registry {
    plugins: Vec<Box<dyn SitePlugin>>,
}

impl Registry {
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    pub fn register(&mut self, plugin: Box<dyn SitePlugin>) {
        self.plugins.push(plugin);
    }

    pub fn resolve(&self, url: &Url) -> &dyn SitePlugin {
        self.plugins
            .iter()
            .find(|p| p.matches(url))
            .map(|b| b.as_ref())
            .expect("registry must contain a catch-all plugin")
    }
}
