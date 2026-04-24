use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

const MAX_LINES: usize = 256;
const EMIT_EVERY: Duration = Duration::from_millis(150);

pub struct BossState {
    lines: VecDeque<LogLine>,
    last_emit: Instant,
    rng_state: u64,
    counter: u64,
}

#[derive(Clone, Copy)]
enum Level {
    Info,
    Warn,
    Error,
    Debug,
}

#[derive(Clone)]
struct LogLine {
    ts_secs: u64,
    level: Level,
    message: String,
}

impl BossState {
    pub fn new() -> Self {
        let mut s = Self {
            lines: VecDeque::with_capacity(MAX_LINES),
            last_emit: Instant::now(),
            rng_state: seed_from_time(),
            counter: 0,
        };
        for _ in 0..24 {
            s.emit_one();
        }
        s
    }

    pub fn tick(&mut self) {
        if self.last_emit.elapsed() >= EMIT_EVERY {
            self.emit_one();
            self.last_emit = Instant::now();
        }
    }

    pub fn render(&self, f: &mut Frame) {
        let area = f.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(0)])
            .split(area);

        self.render_header(f, chunks[0]);
        self.render_log(f, chunks[1]);
    }

    fn render_header(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Line::from(vec![
                Span::styled("─ ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "production.api.edge-west-2",
                    Style::default().fg(Color::Green),
                ),
                Span::styled(" ─", Style::default().fg(Color::DarkGray)),
            ]));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let dim = Style::default().fg(Color::DarkGray);
        let ok = Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let warn = Style::default().fg(Color::Yellow);

        // Fabricate some stable-but-jittery metrics seeded by `counter`.
        let rps = 412 + (self.counter as i64 % 41) - 20;
        let p99 = 34 + (self.counter as i64 % 17) - 8;
        let pool_active = 12 + (self.counter as i64 % 5);
        let err_rate = 0.03 + ((self.counter as i64 % 13) as f64) * 0.007;

        let rows = vec![
            Line::from(vec![
                Span::styled("status    ", dim),
                Span::styled("● healthy", ok),
                Span::styled("    pods  ", dim),
                Span::styled("3/3 ready", ok),
                Span::styled("    traffic  ", dim),
                Span::raw(format!("{rps} req/s")),
            ]),
            Line::from(vec![
                Span::styled("p50       ", dim),
                Span::raw(format!("{}ms", (p99 as i64).saturating_sub(20).max(3))),
                Span::styled("    p99   ", dim),
                Span::raw(format!("{p99}ms")),
                Span::styled("    errors   ", dim),
                if err_rate > 0.08 {
                    Span::styled(format!("{:.2}%", err_rate * 100.0), warn)
                } else {
                    Span::raw(format!("{:.2}%", err_rate * 100.0))
                },
            ]),
            Line::from(vec![
                Span::styled("db.pool   ", dim),
                Span::raw(format!("{pool_active}/20 active")),
                Span::styled("    cache  ", dim),
                Span::raw(format!("{:.1}% hit", 94.0 + (self.counter as i64 % 5) as f64)),
            ]),
        ];
        let para = Paragraph::new(rows).style(Style::default().fg(Color::Gray));
        f.render_widget(para, inner);
    }

    fn render_log(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Line::from(vec![
                Span::styled("─ ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "tail -f /var/log/api/access.log",
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(" ─", Style::default().fg(Color::DarkGray)),
            ]))
            .title_bottom(Line::from(vec![
                Span::styled("─ ", Style::default().fg(Color::DarkGray)),
                Span::styled("[Ctrl-B]", Style::default().fg(Color::Cyan)),
                Span::styled(" back ─", Style::default().fg(Color::DarkGray)),
            ]));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let capacity = inner.height as usize;
        let start = self.lines.len().saturating_sub(capacity);
        let lines: Vec<Line> = self
            .lines
            .iter()
            .skip(start)
            .map(|l| line_for(l))
            .collect();
        f.render_widget(
            Paragraph::new(lines).style(Style::default().fg(Color::Gray)),
            inner,
        );
    }

    fn emit_one(&mut self) {
        let ts_secs = now_unix();
        let (level, message) = self.pick_template(ts_secs);
        if self.lines.len() == MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(LogLine {
            ts_secs,
            level,
            message,
        });
        self.counter = self.counter.wrapping_add(1);
    }

    fn pick_template(&mut self, ts: u64) -> (Level, String) {
        let r = self.next_rand();
        let latency = 5 + (r % 220);
        let status = pick(&r, &[200u16, 200, 200, 200, 201, 204, 301, 404, 500]);
        let route = pick(
            &r,
            &[
                "GET /api/users/{id}",
                "GET /api/orders",
                "POST /api/events",
                "GET /healthz",
                "PATCH /api/users/{id}",
                "GET /api/search?q={term}",
                "DELETE /api/sessions/{id}",
            ],
        );
        let route = route.replace("{id}", &format!("{}", 100_000 + (r % 900_000)));
        let route = route.replace("{term}", pick(&r, &["widget", "invoice", "user"]));

        if r % 53 == 0 {
            let msg = format!(
                "worker-{} retrying job={} attempt=2/5",
                1 + (r % 4),
                16_000_000 + (r % 1_000_000)
            );
            return (Level::Warn, msg);
        }
        if r % 97 == 0 {
            return (
                Level::Error,
                format!(
                    "upstream timeout after {}ms route=\"{route}\" trace_id={:016x}",
                    1200 + (r % 800),
                    ts ^ r
                ),
            );
        }
        if r % 23 == 0 {
            return (
                Level::Debug,
                format!(
                    "cache miss key=usr:{} ttl=300s",
                    200_000 + (r % 800_000)
                ),
            );
        }
        let msg = format!(
            "{route} → {status} {latency}ms client=\"10.{}.{}.{}\"",
            r % 255,
            (r >> 8) % 255,
            (r >> 16) % 255,
        );
        (Level::Info, msg)
    }

    fn next_rand(&mut self) -> u64 {
        // xorshift64 — tiny, no_std-friendly.
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        x
    }
}

fn line_for(l: &LogLine) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    let ts = format_ts(l.ts_secs);
    let (label, lstyle) = match l.level {
        Level::Info => (
            "INFO ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Level::Warn => (
            "WARN ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Level::Error => (
            "ERROR",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
        Level::Debug => ("DEBUG", Style::default().fg(Color::Blue)),
    };
    Line::from(vec![
        Span::styled(ts, dim),
        Span::raw(" "),
        Span::styled(label, lstyle),
        Span::raw(" "),
        Span::raw(l.message.clone()),
    ])
}

fn format_ts(secs: u64) -> String {
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn seed_from_time() -> u64 {
    let mut x = now_unix().wrapping_mul(0x9E3779B97F4A7C15);
    x ^= x >> 33;
    x | 1
}

fn pick<'a, T: Copy>(seed: &u64, choices: &'a [T]) -> T {
    choices[(*seed as usize) % choices.len()]
}
