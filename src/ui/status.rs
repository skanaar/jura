use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};

use super::*;
use crate::app::{App, Focus, Mode};

pub(super) fn draw(f: &mut Frame, app: &App, area: Rect) {
    if let Mode::Search = app.mode {
        let line = Line::from(vec![
            Span::styled(" / ", Style::new().fg(Color::Black).bg(Color::Yellow)),
            Span::raw(format!(" {}", app.search)),
            Span::styled("█", Style::new().fg(Color::Yellow)),
            Span::styled("   Enter keep · Esc clear", DIM),
        ]);
        f.render_widget(line, area);
        return;
    }

    let hints: &[(&str, &str)] = match app.focus {
        Focus::List => &[
            ("/", "search"),
            ("⏎", "open"),
            ("n", "new"),
            ("t", "transition"),
            ("c", "comment"),
            ("a", "assign me"),
            ("o", "browser"),
            ("r", "refresh"),
            ("?", "help"),
            ("q", "quit"),
        ],
        Focus::Detail => &[
            ("j/k", "scroll"),
            ("←/→", "tab"),
            ("Esc", "back"),
            ("n", "new"),
            ("t", "transition"),
            ("c", "comment"),
            ("a", "assign me"),
            ("o", "browser"),
            ("q", "quit"),
        ],
    };
    let mut spans: Vec<Span> = Vec::new();
    for (k, v) in hints {
        spans.push(Span::styled(format!(" {k}"), Style::new().fg(ACCENT)));
        spans.push(Span::styled(format!(" {v} "), DIM));
    }
    let left = Line::from(spans);

    let right = match app.flash() {
        Some(fl) => {
            Line::from(format!("{} ", fl.text)).fg(if fl.error { Color::Red } else { Color::Green })
        }
        None if app.is_busy() => Line::from(format!("{} syncing ", spinner(app))).fg(Color::Yellow),
        None => Line::default(),
    };
    let right_w = (right.width() as u16).min(area.width / 2);
    let [l, r] = Layout::horizontal([Constraint::Min(0), Constraint::Length(right_w)]).areas(area);
    f.render_widget(left, l);
    f.render_widget(right.right_aligned(), r);
}

