use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::sync::oneshot;
use url::Url;

use crate::bosskey::BossState;
use crate::plugin::{Article, Registry};
use crate::state::{Bookmark, State, now_unix};

pub enum Mode {
    Loading {
        url: Url,
        started_at: Instant,
        rx: oneshot::Receiver<Result<Article>>,
        restore_scroll: u16,
    },
    Reading {
        url: Url,
        article: Article,
        scroll: u16,
        loaded_in: Duration,
    },
    Error {
        message: String,
    },
}

pub struct App {
    pub registry: Arc<Registry>,
    pub state: State,
    pub mode: Mode,
    pub tick: u64,
    pub quit: bool,
    pub size: (u16, u16),
    pub flash: Option<(String, Instant)>,
    pub boss: Option<BossState>,
}

impl App {
    pub fn new(registry: Arc<Registry>, state: State, url: Url) -> Self {
        let mut app = Self {
            registry,
            state,
            mode: Mode::Error {
                message: "initializing".into(),
            },
            tick: 0,
            quit: false,
            size: (80, 24),
            flash: None,
            boss: None,
        };
        let restore = app.state.lookup(&url).unwrap_or(0);
        app.start_fetch(url, restore);
        app
    }

    pub fn start_fetch(&mut self, url: Url, restore_scroll: u16) {
        let reg = Arc::clone(&self.registry);
        let (tx, rx) = oneshot::channel();
        let url_for_task = url.clone();
        tokio::spawn(async move {
            let result = async {
                let plugin = reg.resolve(&url_for_task);
                let page = plugin
                    .fetch(&url_for_task)
                    .await
                    .with_context(|| format!("fetch {url_for_task}"))?;
                let article = plugin.extract(&page)?;
                Ok::<_, anyhow::Error>(article)
            }
            .await;
            let _ = tx.send(result);
        });
        self.mode = Mode::Loading {
            url,
            started_at: Instant::now(),
            rx,
            restore_scroll,
        };
    }

    pub fn poll_fetch(&mut self) {
        let finished = match &mut self.mode {
            Mode::Loading { rx, .. } => match rx.try_recv() {
                Ok(result) => Some(result),
                Err(oneshot::error::TryRecvError::Empty) => None,
                Err(oneshot::error::TryRecvError::Closed) => {
                    Some(Err(anyhow::anyhow!("fetch task panicked")))
                }
            },
            _ => None,
        };

        let Some(result) = finished else { return };
        let (url, started_at, restore_scroll) = match std::mem::replace(
            &mut self.mode,
            Mode::Error {
                message: String::new(),
            },
        ) {
            Mode::Loading {
                url,
                started_at,
                restore_scroll,
                ..
            } => (url, started_at, restore_scroll),
            _ => unreachable!(),
        };

        match result {
            Ok(article) => {
                // Cap the restore to something sensible — if the chapter layout
                // changed the scroll offset from the last read may now overshoot.
                let max_sensible = article.body_text.lines().count() as u16;
                let scroll = restore_scroll.min(max_sensible.saturating_add(200));
                self.state.record(&url, scroll);
                let _ = self.state.save();
                self.mode = Mode::Reading {
                    url,
                    article,
                    scroll,
                    loaded_in: started_at.elapsed(),
                };
            }
            Err(e) => {
                self.mode = Mode::Error {
                    message: format!("{e:#}"),
                };
            }
        }
    }

    pub fn next_chapter(&mut self) {
        let url = match &self.mode {
            Mode::Reading { url, .. } => {
                let plugin = self.registry.resolve(url);
                let page = crate::plugin::RenderedPage {
                    url: url.clone(),
                    html: String::new(),
                };
                plugin.next(&page)
            }
            _ => None,
        };
        if let Some(u) = url {
            self.start_fetch(u, 0);
        }
    }

    pub fn prev_chapter(&mut self) {
        let url = match &self.mode {
            Mode::Reading { url, .. } => {
                let plugin = self.registry.resolve(url);
                let page = crate::plugin::RenderedPage {
                    url: url.clone(),
                    html: String::new(),
                };
                plugin.prev(&page)
            }
            _ => None,
        };
        if let Some(u) = url {
            self.start_fetch(u, 0);
        }
    }

    pub fn scroll_by(&mut self, delta: i32, max: u16) {
        if let Mode::Reading { scroll, .. } = &mut self.mode {
            let new = (*scroll as i32 + delta).clamp(0, max as i32);
            *scroll = new as u16;
        }
    }

    /// Save current reading position to disk. Called on quit and on chapter
    /// load — keeps `~/.config/twr/state.toml` in sync with what's on screen.
    pub fn save_now(&mut self) {
        if let Mode::Reading { url, scroll, .. } = &self.mode {
            self.state.record(url, *scroll);
        }
        if let Err(e) = self.state.save() {
            tracing::warn!("save state: {e:#}");
        }
    }

    /// Add a bookmark for the current chapter with an auto-derived name.
    pub fn bookmark_current(&mut self) {
        let Mode::Reading { url, .. } = &self.mode else {
            return;
        };
        let name = default_bookmark_name(url);
        self.state.bookmarks.retain(|b| b.name != name);
        self.state.bookmarks.push(Bookmark {
            name: name.clone(),
            url: url.to_string(),
            saved_at: now_unix(),
        });
        let _ = self.state.save();
        self.flash = Some((format!("bookmarked as {name}"), Instant::now()));
    }

    pub fn toggle_boss(&mut self) {
        if self.boss.is_some() {
            self.boss = None;
        } else {
            self.boss = Some(BossState::new());
        }
    }

    pub fn flash_text(&self) -> Option<&str> {
        let (msg, at) = self.flash.as_ref()?;
        if at.elapsed() < Duration::from_secs(3) {
            Some(msg.as_str())
        } else {
            None
        }
    }
}

pub fn default_bookmark_name(url: &Url) -> String {
    let host = url.host_str().unwrap_or("unknown");
    let last = url
        .path_segments()
        .and_then(|segs| segs.filter(|s| !s.is_empty()).last())
        .unwrap_or("root");
    format!("{host}/{last}")
}
