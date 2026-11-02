use chrono::{DateTime, Utc};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};

mod detail;
mod popups;
mod status;
mod table;

use crate::app::{App, Focus, Mode};

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ACCENT: Color = Color::Cyan;
const DIM: Style = Style::new().fg(Color::DarkGray);

pub fn draw(f: &mut Frame, app: &mut App) {
    let [main, status] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(f.area());

    let split = app.show_preview && main.width >= 100;
    if split {
        let [list, detail] =
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
                .areas(main);
        table::draw(f, app, list);
        detail::draw(f, app, detail);
    } else if app.focus == Focus::Detail {
        detail::draw(f, app, main);
    } else {
        table::draw(f, app, main);
    }

    status::draw(f, app, status);

    match &app.mode {
        Mode::Transition { .. } => popups::transitions(f, app),
        Mode::Comment { .. } => popups::comment(f, app),
        Mode::Create { .. } => popups::create(f, app),
        Mode::Help => popups::help(f),
        Mode::Normal | Mode::Search => {}
    }
}

fn spinner(app: &App) -> &'static str {
    SPINNER[app.spinner % SPINNER.len()]
}

/// Draws folder-style tabs over a panel and returns the panel area below them.
/// The active tab is open into the panel; the rest of the border line continues
/// to the right corner. Pair the panel with `panel_block()`.
fn draw_tab_header(
    f: &mut Frame,
    area: Rect,
    tabs: &[(String, Style)],
    active: usize,
    border: Style,
) -> Rect {
    let [header, panel] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(area);
    let [top, line] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(header);

    let mut spans = Vec::new();
    let mut start = 0usize;
    let mut active_span = (0usize, 0usize);
    for (i, (name, style)) in tabs.iter().enumerate() {
        let w = name.chars().count() + 4;
        if i == active {
            active_span = (start, start + w);
            spans.push(Span::styled("╭ ", border));
            spans.push(Span::styled(name.clone(), *style));
            spans.push(Span::styled(" ╮", border));
        } else {
            spans.push(Span::styled(format!("  {name}  "), *style));
        }
        start += w;
    }
    f.render_widget(Line::from(spans), top);

    let (s, e) = active_span;
    let width = area.width as usize;
    let mut row = String::new();
    row.push(if s == 0 { '│' } else { '╭' });
    row.push_str(&"─".repeat(s.saturating_sub(1)));
    if s > 0 {
        row.push('╯');
    }
    row.push_str(&" ".repeat(e.saturating_sub(s + 2)));
    row.push('╰');
    let used = row.chars().count();
    row.push_str(&"─".repeat(width.saturating_sub(used + 1)));
    row.push('╮');
    f.render_widget(Line::styled(row, border), line);
    panel
}

/// The panel under `draw_tab_header`: its top edge is the header's second row.
fn panel_block(border: Style) -> Block<'static> {
    Block::bordered()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded)
        .border_style(border)
}

pub fn status_color(category: &str) -> Color {
    match category {
        "new" => Color::Blue,
        "indeterminate" => Color::Yellow,
        "done" => Color::Green,
        _ => Color::Gray,
    }
}

fn priority_color(p: &str) -> Color {
    match p.to_lowercase().as_str() {
        "highest" | "blocker" | "critical" => Color::LightRed,
        "high" | "major" => Color::Red,
        "medium" => Color::Yellow,
        "low" | "minor" => Color::Green,
        "lowest" | "trivial" => Color::DarkGray,
        _ => Color::Gray,
    }
}

fn parse_time(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.f%z")
        .or_else(|_| DateTime::parse_from_rfc3339(ts))
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Compact age such as `5m`, `3h`, `2d`.
pub fn ago(t: DateTime<Utc>) -> String {
    let s = (Utc::now() - t).num_seconds().max(0);
    match s {
        0..60 => "now".into(),
        60..3600 => format!("{}m", s / 60),
        3600..86400 => format!("{}h", s / 3600),
        86400..2_592_000 => format!("{}d", s / 86400),
        2_592_000..31_536_000 => format!("{}mo", s / 2_592_000),
        _ => format!("{}y", s / 31_536_000),
    }
}

fn ago_str(ts: &str) -> String {
    parse_time(ts).map(ago).unwrap_or_default()
}

fn local_time(ts: &str) -> String {
    parse_time(ts)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn border_style(focused: bool) -> Style {
    if focused {
        Style::new().fg(ACCENT)
    } else {
        DIM
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}
