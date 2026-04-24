mod app;
mod bosskey;
mod plugin;
mod plugins;
mod state;
mod ui;

use std::io;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use crossterm::{execute, terminal};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tracing_subscriber::EnvFilter;
use url::Url;

use crate::app::{App, Mode};
use crate::plugin::Registry;
use crate::plugins::generic::GenericPlugin;
use crate::state::State;

#[derive(Parser, Debug)]
#[command(version, about = "Terminal web reader")]
struct Cli {
    /// Chapter or article URL. Optional if --resume or --bookmark is supplied.
    url: Option<String>,

    /// Open the most recently read URL from saved history.
    #[arg(long, conflicts_with_all = ["bookmark", "url"])]
    resume: bool,

    /// Open a saved bookmark by name.
    #[arg(long, value_name = "NAME")]
    bookmark: Option<String>,

    /// List saved bookmarks and exit.
    #[arg(long, conflicts_with_all = ["url", "resume", "bookmark"])]
    list_bookmarks: bool,

    /// Print extracted text to stdout instead of launching the TUI.
    #[arg(long)]
    print: bool,

    /// With --print: follow `next` links this many times and print each in sequence.
    #[arg(long, default_value_t = 0)]
    follow: u32,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli);
    let state = State::load();

    if cli.list_bookmarks {
        if state.bookmarks.is_empty() {
            println!("(no bookmarks)");
        } else {
            for b in &state.bookmarks {
                println!("{}\t{}", b.name, b.url);
            }
        }
        return Ok(());
    }

    let registry = Arc::new(build_registry()?);
    let url = resolve_target_url(&cli, &state)?;

    if cli.print {
        return run_print(registry, url, cli.follow).await;
    }

    run_tui(registry, state, url).await
}

fn init_tracing(cli: &Cli) {
    let default_filter =
        "warn,chromiumoxide::handler=error,chromiumoxide::browser=error".to_string();
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| default_filter.into());

    // --print mode logs to stderr (pipeable). TUI mode writes to a file so logs
    // don't clobber the alternate-screen buffer.
    if cli.print || cli.list_bookmarks {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .try_init();
        return;
    }

    let Ok(path) = State::log_path() else {
        return; // no config dir — silent logs are an acceptable fallback
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(file) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
    else {
        return;
    };
    let writer = std::sync::Mutex::new(file);
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init();
}

fn resolve_target_url(cli: &Cli, state: &State) -> Result<Url> {
    if let Some(raw) = &cli.url {
        return Url::parse(raw).with_context(|| format!("invalid URL: {raw}"));
    }
    if cli.resume {
        let entry = state
            .latest()
            .context("--resume: no history yet; open a URL first")?;
        return Url::parse(&entry.url).with_context(|| format!("parse {}", entry.url));
    }
    if let Some(name) = &cli.bookmark {
        let b = state
            .find_bookmark(name)
            .with_context(|| format!("--bookmark: no bookmark named \"{name}\""))?;
        return Url::parse(&b.url).with_context(|| format!("parse {}", b.url));
    }
    bail!("no URL provided (pass a URL, --resume, or --bookmark NAME)");
}

fn build_registry() -> Result<Registry> {
    let mut reg = Registry::new();
    #[cfg(feature = "js")]
    reg.register(Box::new(crate::plugins::sangtacviet::SangtacvietPlugin::new()));
    reg.register(Box::new(GenericPlugin::new()?));
    Ok(reg)
}

async fn run_print(registry: Arc<Registry>, mut url: Url, follow: u32) -> Result<()> {
    for _ in 0..=follow {
        let plugin = registry.resolve(&url);
        tracing::info!(plugin = plugin.name(), url = %url, "fetching");
        let page = plugin.fetch(&url).await?;
        let article = plugin.extract(&page)?;
        println!("=== {} ===", article.title.as_deref().unwrap_or("(untitled)"));
        if let Some(by) = &article.byline {
            println!("by {by}");
        }
        println!("source: {url}");
        println!();
        println!("{}", article.body_text.trim());
        println!();

        let Some(next) = plugin.next(&page) else { break };
        url = next;
    }
    Ok(())
}

async fn run_tui(registry: Arc<Registry>, state: State, url: Url) -> Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(registry, state, url);
    let result = event_loop(&mut app, &mut terminal).await;
    app.save_now();

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), terminal::LeaveAlternateScreen)?;
    terminal.show_cursor().ok();

    result
}

async fn event_loop(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(100));

    loop {
        terminal.draw(|f| ui::render(f, app))?;
        if app.quit {
            break;
        }

        tokio::select! {
            _ = ticker.tick() => {
                app.tick = app.tick.wrapping_add(1);
                app.poll_fetch();
                if let Some(boss) = app.boss.as_mut() {
                    boss.tick();
                }
            }
            Some(Ok(ev)) = events.next() => {
                handle_event(app, ev, terminal.size()?.height);
            }
        }
    }
    Ok(())
}

fn handle_event(app: &mut App, ev: Event, screen_height: u16) {
    let Event::Key(KeyEvent {
        code, modifiers, ..
    }) = ev
    else {
        return;
    };

    // Ctrl-B always toggles the boss overlay, regardless of current mode.
    if matches!(code, KeyCode::Char('b')) && modifiers.contains(KeyModifiers::CONTROL) {
        app.toggle_boss();
        return;
    }
    // While the boss is up, swallow every other key so accidental scroll /
    // navigation keys don't mutate the underlying reader state.
    if app.boss.is_some() {
        return;
    }

    let body_h = screen_height.saturating_sub(4);
    let max = ui::max_scroll(app, body_h);

    match (code, modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.quit = true;
        }
        (KeyCode::Right, _) | (KeyCode::Char('l'), _) => app.next_chapter(),
        (KeyCode::Left, _) | (KeyCode::Char('h'), _) => app.prev_chapter(),
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => app.scroll_by(1, max),
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => app.scroll_by(-1, max),
        (KeyCode::PageDown, _) | (KeyCode::Char(' '), _) => {
            app.scroll_by(body_h as i32 - 1, max)
        }
        (KeyCode::PageUp, _) => app.scroll_by(-(body_h as i32 - 1), max),
        (KeyCode::Home, _) | (KeyCode::Char('g'), _) => app.scroll_by(-i32::MAX, max),
        (KeyCode::End, _) | (KeyCode::Char('G'), _) => app.scroll_by(i32::MAX, max),
        (KeyCode::Char('b'), _) => app.bookmark_current(),
        (KeyCode::Char('r'), _) => {
            if let Mode::Reading { url, scroll, .. } = &app.mode {
                let (u, s) = (url.clone(), *scroll);
                app.start_fetch(u, s);
            }
        }
        _ => {}
    }
}
