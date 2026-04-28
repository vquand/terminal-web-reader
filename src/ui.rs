use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use url::Url;

use crate::app::{App, MAX_FONT_SIZE_STEP, MIN_FONT_SIZE_STEP, Mode};

const INDENT: &str = "  ";
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.size = (area.width, area.height);

    if let Some(boss) = &app.boss {
        boss.render(f);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0)])
        .split(area);
    let main = chunks[0];

    let title = header_title(app);
    let footer = footer_line();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(title)
        .title_bottom(footer);
    let inner = block.inner(main);
    f.render_widget(block, main);

    // Split inner into: status line (1), gap (1), body (rest).
    let inner_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    f.render_widget(status_line(app), inner_rows[0]);
    render_body(f, app, inner_rows[2]);
}

fn header_title(app: &App) -> Line<'static> {
    let path = match &app.mode {
        Mode::Loading { url, .. } => pseudo_path(url),
        Mode::Reading { url, .. } => pseudo_path(url),
        Mode::Error { .. } => "src/error.rs".to_string(),
    };
    Line::from(vec![
        Span::styled("─ ", Style::default().fg(Color::DarkGray)),
        Span::styled(path, Style::default().fg(Color::Gray)),
        Span::styled(" ─", Style::default().fg(Color::DarkGray)),
    ])
}

fn footer_line() -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    let key = Style::default().fg(Color::Cyan);
    Line::from(vec![
        Span::styled("─ ", dim),
        Span::styled("[←]", key),
        Span::styled(" prev  ", dim),
        Span::styled("[→]", key),
        Span::styled(" next  ", dim),
        Span::styled("[↑/↓]", key),
        Span::styled(" scroll  ", dim),
        Span::styled("[Ctrl+=/-]", key),
        Span::styled(" font  ", dim),
        Span::styled("[q]", key),
        Span::styled(" quit ─", dim),
    ])
}

fn status_line(app: &App) -> Paragraph<'static> {
    let time = now_hms();
    let dim = Style::default().fg(Color::DarkGray);
    let info = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let warn = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let err = Style::default()
        .fg(Color::Red)
        .add_modifier(Modifier::BOLD);

    let (tag_style, tag, body) = match &app.mode {
        Mode::Loading { started_at, url, .. } => {
            let frame = SPINNER[(app.tick as usize) % SPINNER.len()];
            (
                warn,
                "WARN",
                format!(
                    "{frame} fetching {url} ({:.1}s)",
                    started_at.elapsed().as_secs_f32()
                ),
            )
        }
        Mode::Reading {
            article,
            loaded_in,
            ..
        } => {
            let title = article.title.as_deref().unwrap_or("untitled");
            let size_kb = article.body_text.len() as f32 / 1024.0;
            (
                info,
                "INFO",
                format!(
                    "loaded \"{}\" {:.1} KB in {:.1}s",
                    title,
                    size_kb,
                    loaded_in.as_secs_f32()
                ),
            )
        }
        Mode::Error { message, .. } => (err, "ERR ", truncate(message, 200)),
    };

    let mut spans = vec![
        Span::raw(" "),
        Span::styled(time, dim),
        Span::raw(" ["),
        Span::styled(tag, tag_style),
        Span::raw("] "),
        Span::raw(body),
    ];
    if let Some(flash) = app.flash_text() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("· {flash}"),
            Style::default().fg(Color::Magenta),
        ));
    }
    Paragraph::new(Text::from(Line::from(spans)))
}

fn render_body(f: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line<'static>> = match &app.mode {
        Mode::Loading { .. } => vec![Line::from(Span::styled(
            format!("{INDENT}// awaiting chapter content…"),
            Style::default().fg(Color::DarkGray),
        ))],
        Mode::Reading { article, .. } => {
            body_lines(&article.body_text, area.width, app.state.font_size_step)
        }
        Mode::Error { message, .. } => vec![
            Line::from(Span::styled(
                format!("{INDENT}panic: {message}"),
                Style::default().fg(Color::Red),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("{INDENT}// press [r] to retry, [q] to quit"),
                Style::default().fg(Color::DarkGray),
            )),
        ],
    };

    let scroll = match &app.mode {
        Mode::Reading { scroll, .. } => *scroll,
        _ => 0,
    };

    let para = Paragraph::new(Text::from(lines))
        .style(Style::default().fg(Color::Gray))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(para, area);
}

fn body_lines(text: &str, width: u16, font_size_step: i8) -> Vec<Line<'static>> {
    // Width minus the 2-char indent. Give a little right padding to avoid cramped
    // wraps. Clamp to something sane on narrow terminals.
    let base_width = (width as usize).saturating_sub(INDENT.len() + 2).max(20);
    let wrap_width = scaled_wrap_width(base_width, font_size_step);
    let normalized = normalize_whitespace(text);
    let paragraphs: Vec<&str> = normalized
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .collect();

    let mut out: Vec<Line<'static>> = Vec::new();
    for (idx, para) in paragraphs.iter().enumerate() {
        if idx > 0 {
            out.push(Line::from(""));
        }
        for wrapped in textwrap::wrap(para.trim(), wrap_width) {
            out.push(Line::from(format!("{INDENT}{wrapped}")));
        }
    }
    if out.is_empty() {
        out.push(Line::from(format!("{INDENT}(empty chapter)")));
    }
    out
}

fn scaled_wrap_width(base_width: usize, font_size_step: i8) -> usize {
    let step = font_size_step.clamp(MIN_FONT_SIZE_STEP, MAX_FONT_SIZE_STEP);
    let percent = match step {
        -3 => 145,
        -2 => 130,
        -1 => 115,
        0 => 100,
        1 => 88,
        2 => 78,
        3 => 70,
        _ => unreachable!(),
    };
    (base_width * percent / 100).max(20)
}

fn normalize_whitespace(text: &str) -> String {
    // Collapse runs of whitespace to single spaces, but keep paragraph breaks
    // (detected as consecutive newlines or sentence-ending punctuation followed
    // by quoted dialog).
    let mut s = text.replace("\r\n", "\n");
    // Common heuristic for novel text: a period/quote followed by a capital or
    // opening quote is a paragraph boundary. Cheap approximation.
    s = s.replace(".\"", ".\"\n\n");
    s = s.replace(".”", ".”\n\n");
    s = s.replace("”", "”\n\n");
    s
}

fn pseudo_path(url: &Url) -> String {
    let host = url.host_str().unwrap_or("unknown");
    let last = url
        .path_segments()
        .and_then(|segs| segs.filter(|s| !s.is_empty()).last())
        .unwrap_or("index");
    match host {
        "sangtacviet.vip" => format!("src/chapter_{last}.rs"),
        _ => format!("src/{}.rs", sanitize(last)),
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => c,
            _ => '_',
        })
        .collect()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

fn now_hms() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    // Local-ish HH:MM:SS — we don't know the TZ without a crate. Use UTC for
    // simplicity; the point is ambient color, not accuracy.
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// Max scroll offset for the current mode, given the visible body height.
pub fn max_scroll(app: &App, body_height: u16) -> u16 {
    match &app.mode {
        Mode::Reading { article, .. } => {
            let width = app.size.0.saturating_sub(4); // inside borders
            let lines = body_lines(&article.body_text, width, app.state.font_size_step);
            (lines.len() as u16).saturating_sub(body_height)
        }
        _ => 0,
    }
}
