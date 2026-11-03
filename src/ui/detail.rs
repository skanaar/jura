use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};

use super::*;
use crate::app::{App, DetailTab, Focus};

pub(super) fn draw(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Detail;
    let border = border_style(focused);
    let issue = app.selected_issue();
    let detail = issue.and_then(|i| app.details.get(&i.key));

    let tabs: Vec<(String, Style)> = DetailTab::ALL
        .iter()
        .map(|t| {
            let mut name = match t {
                DetailTab::Description => "Description".to_string(),
                DetailTab::Info => "Info".to_string(),
                DetailTab::Comments => "Comments".to_string(),
            };
            if *t == DetailTab::Comments
                && let Some(d) = detail
            {
                name = format!("{name} ({})", d.data.comments.len());
            }
            let style = if *t == app.detail_tab {
                Style::new().fg(ACCENT).bold()
            } else {
                Style::new().fg(Color::Gray)
            };
            (name, style)
        })
        .collect();
    let active = DetailTab::ALL.iter().position(|t| *t == app.detail_tab).unwrap_or(0);
    let area = draw_tab_header(f, area, &tabs, active, border);

    let block = panel_block(border);
    let Some(issue) = issue else {
        f.render_widget(block, area);
        return;
    };
    let key = issue.key.as_str();

    let key_line = Line::from(Span::styled(key.to_string(), Style::new().fg(ACCENT).bold()));
    let status_line = if app.details_loading.contains(key) {
        Line::from(format!("{} refreshing", spinner(app))).fg(Color::Yellow)
    } else if let Some(e) = app.detail_errors.get(key) {
        Line::from(format!("✗ {}", truncate(e, 50))).fg(Color::Red)
    } else if let Some(d) = detail {
        Line::from(format!("updated {} ago", ago(d.fetched_at))).style(DIM)
    } else {
        Line::default()
    };

    let label = |s: &str| Span::styled(format!("{s:<10}"), DIM);
    let mut lines: Vec<Line> = vec![Line::from(issue.summary.clone()).bold(), Line::default()];

    match app.detail_tab {
        DetailTab::Info => {
            lines.push(Line::from(vec![
                label("Status"),
                Span::styled(
                    app.cfg.status_label(&issue.status).to_string(),
                    Style::new().fg(status_color(&issue.status_category)).bold(),
                ),
            ]));
            lines.push(Line::from(vec![label("Type"), Span::raw(issue.issue_type.clone())]));
            if let Some(p) = &issue.priority {
                lines.push(Line::from(vec![
                    label("Priority"),
                    Span::styled(p.clone(), Style::new().fg(priority_color(p))),
                ]));
            }
            lines.push(Line::from(vec![
                label("Assignee"),
                Span::raw(
                    issue
                        .assignee
                        .clone()
                        .unwrap_or_else(|| "Unassigned".into()),
                ),
            ]));
            if let Some(r) = &issue.reporter {
                lines.push(Line::from(vec![label("Reporter"), Span::raw(r.clone())]));
            }
            if !issue.labels.is_empty() {
                lines.push(Line::from(vec![
                    label("Labels"),
                    Span::styled(issue.labels.join(", "), Style::new().fg(Color::Magenta)),
                ]));
            }
            lines.push(Line::from(vec![
                label("Created"),
                Span::raw(local_time(&issue.created)),
            ]));
            lines.push(Line::from(vec![
                label("Updated"),
                Span::raw(local_time(&issue.updated)),
            ]));
        }
        tab => match detail {
            Some(d) if tab == DetailTab::Description => {
                if d.data.description.trim().is_empty() {
                    lines.push(Line::from("No description.").style(DIM));
                } else {
                    lines.extend(
                        d.data
                            .description
                            .lines()
                            .map(|l| Line::from(l.to_string())),
                    );
                }
            }
            Some(d) => {
                if d.data.comments.is_empty() {
                    lines.push(Line::from("No comments.").style(DIM));
                }
                for c in &d.data.comments {
                    let when = if c.pending {
                        "sending…".to_string()
                    } else {
                        format!("{} ago", ago_str(&c.created))
                    };
                    lines.push(Line::from(vec![
                        Span::styled(c.author.clone(), Style::new().fg(Color::LightBlue).bold()),
                        Span::styled(format!(" · {when}"), DIM),
                    ]));
                    let style = if c.pending { DIM } else { Style::new() };
                    lines.extend(
                        c.body
                            .lines()
                            .map(|l| Line::from(format!("  {l}")).style(style)),
                    );
                    lines.push(Line::default());
                }
            }
            None if app.detail_errors.contains_key(key) => {
                lines.push(Line::from("Details unavailable. Press r to retry.").style(DIM));
            }
            // First ever view of this issue: the list data is already shown on the Info tab.
            None => lines.push(Line::from(format!("{} Loading…", spinner(app))).style(DIM)),
        },
    }

    let inner = block.inner(area);
    f.render_widget(block, area);
    let [inner, footer] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    draw_footer(f, footer, key_line, status_line);
    let body = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };

    let max_scroll = lines.len().saturating_sub(1) as u16;
    let para = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll.min(max_scroll), 0));
    f.render_widget(para, body);
}

