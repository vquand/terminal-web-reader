use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub history: HashMap<String, HistoryEntry>,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    #[serde(default)]
    pub scroll: u16,
    #[serde(default)]
    pub saved_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub saved_at: u64,
}

impl State {
    pub fn path() -> Result<PathBuf> {
        let proj = ProjectDirs::from("", "", "twr")
            .ok_or_else(|| anyhow!("could not locate a config directory"))?;
        Ok(proj.config_dir().join("state.toml"))
    }

    pub fn log_path() -> Result<PathBuf> {
        let proj = ProjectDirs::from("", "", "twr")
            .ok_or_else(|| anyhow!("could not locate a config directory"))?;
        Ok(proj.config_dir().join("twr.log"))
    }

    /// Load state; any I/O or parse error yields an empty State (file may not
    /// exist yet). Errors are logged, not propagated — persistence is best-effort.
    pub fn load() -> Self {
        let path = match Self::path() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("state path: {e:#}");
                return Self::default();
            }
        };
        match fs::read_to_string(&path) {
            Ok(s) => match toml::from_str::<State>(&s) {
                Ok(state) => state,
                Err(e) => {
                    tracing::warn!("state parse at {path:?}: {e:#}");
                    Self::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                tracing::warn!("state read at {path:?}: {e:#}");
                Self::default()
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create {parent:?}"))?;
        }
        let body = toml::to_string_pretty(self).context("serialize state")?;
        fs::write(&path, body).with_context(|| format!("write {path:?}"))?;
        Ok(())
    }

    pub fn record(&mut self, url: &Url, scroll: u16) {
        let Some(domain) = url.host_str() else {
            return;
        };
        self.history.insert(
            domain.to_string(),
            HistoryEntry {
                url: url.to_string(),
                scroll,
                saved_at: now_unix(),
            },
        );
    }

    pub fn lookup(&self, url: &Url) -> Option<u16> {
        let domain = url.host_str()?;
        let entry = self.history.get(domain)?;
        (entry.url == url.as_str()).then_some(entry.scroll)
    }

    /// Most recently saved history entry across all domains, for `--resume`.
    pub fn latest(&self) -> Option<&HistoryEntry> {
        self.history.values().max_by_key(|e| e.saved_at)
    }

    pub fn find_bookmark(&self, name: &str) -> Option<&Bookmark> {
        self.bookmarks.iter().find(|b| b.name == name)
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_lookup_same_url_returns_scroll() {
        let mut s = State::default();
        let u = Url::parse("https://example.com/chapter/5").unwrap();
        s.record(&u, 42);
        assert_eq!(s.lookup(&u), Some(42));
    }

    #[test]
    fn lookup_different_url_same_domain_returns_none() {
        let mut s = State::default();
        let u = Url::parse("https://example.com/chapter/5").unwrap();
        s.record(&u, 42);
        let other = Url::parse("https://example.com/chapter/6").unwrap();
        assert_eq!(s.lookup(&other), None);
    }

    #[test]
    fn record_overwrites_same_domain() {
        let mut s = State::default();
        let a = Url::parse("https://example.com/a").unwrap();
        let b = Url::parse("https://example.com/b").unwrap();
        s.record(&a, 10);
        s.record(&b, 20);
        assert_eq!(s.history.len(), 1);
        assert_eq!(s.lookup(&b), Some(20));
    }

    #[test]
    fn latest_picks_highest_saved_at() {
        let mut s = State::default();
        s.history.insert(
            "older.com".into(),
            HistoryEntry {
                url: "https://older.com/".into(),
                scroll: 0,
                saved_at: 100,
            },
        );
        s.history.insert(
            "newer.com".into(),
            HistoryEntry {
                url: "https://newer.com/".into(),
                scroll: 0,
                saved_at: 200,
            },
        );
        assert_eq!(s.latest().unwrap().url, "https://newer.com/");
    }

    #[test]
    fn roundtrip_toml() {
        let mut s = State::default();
        let u = Url::parse("https://example.com/chapter/5").unwrap();
        s.record(&u, 17);
        s.bookmarks.push(Bookmark {
            name: "test".into(),
            url: u.to_string(),
            saved_at: 123,
        });
        let body = toml::to_string_pretty(&s).unwrap();
        let back: State = toml::from_str(&body).unwrap();
        assert_eq!(back.lookup(&u), Some(17));
        assert_eq!(back.bookmarks[0].name, "test");
    }
}
