use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use super::*;
use crate::app::{App, Focus};
use crate::jira::Issue;

pub(super) fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::List;
    let border = border_style(focused);
    let tabs: Vec<(String, Style)> = app
        .filters
        .iter()
        .enumerate()
        .map(|(i, flt)| {
            let name = flt.name.clone();
            let style = if i == app.active {
                if focused { Style::new().fg(ACCENT).bold() } else { Style::new().fg(Color::Gray).bold() }
            } else if flt.error.is_some() {
                Style::new().fg(Color::Red)
            } else {
                Style::new().fg(Color::Gray)
            };
            (name, style)
        })
        .collect();
    let area = draw_tab_header(f, area, &tabs, app.active, border);

    let filter = app.active_filter();
    let visible = app.visible();

    let mut title = vec![Span::raw(format!("{}/{}", visible.len(), filter.issues.len()))];
    if !app.search.is_empty() {
        title.push(Span::styled(
            format!("  /{}", app.search),
            Style::new().fg(Color::Yellow),
        ));
    }

    let freshness = if filter.loading {
        Line::from(format!("{} refreshing", spinner(app))).fg(Color::Yellow)
    } else if let Some(e) = &filter.error {
        Line::from(format!("✗ {}", truncate(e, 60))).fg(Color::Red)
    } else if let Some(t) = filter.fetched_at {
        Line::from(format!("updated {} ago", ago(t))).style(DIM)
    } else {
        Line::default()
    };

    let block = panel_block(border);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [area, footer] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    draw_footer(f, footer, Line::from(title), freshness);
    let area = Rect { width: area.width.saturating_sub(1), ..area };

    if visible.is_empty() {
        let msg = if filter.fetched_at.is_none() && filter.loading {
            format!("{} Fetching issues…", spinner(app))
        } else if filter.fetched_at.is_none() && filter.error.is_some() {
            "Could not load issues. Press r to retry.".into()
        } else if !app.search.is_empty() {
            "No issues match the search. Esc clears it.".into()
        } else {
            "No issues match this filter.".into()
        };
        f.render_widget(Paragraph::new(msg).style(DIM).centered(), area);
        return;
    }

    let wide = area.width >= 110;
    let key_w = visible.iter().map(|i| i.key.len()).max().unwrap_or(6) as u16;
    // Fit the column to the (possibly shortened) status labels, within 6..=14.
    let status_w = visible
        .iter()
        .map(|i| app.cfg.status_label(&i.status).chars().count())
        .max()
        .unwrap_or(6);
    let status_w = status_w.clamp(6, 14);
    let mut widths = vec![
        Constraint::Length(key_w),
        Constraint::Length(status_w as u16),
    ];
    if wide {
        widths.push(Constraint::Length(8));
    }
    widths.extend([Constraint::Length(3), Constraint::Fill(1)]);

    let rows: Vec<Row> = visible
        .iter()
        .map(|i| issue_row(i, app.cfg.status_label(&i.status), status_w, wide))
        .collect();
    let mut header = vec!["Key", "Status"];
    if wide {
        header.push("Priority");
    }
    header.extend(["As", "Summary"]);

    let table = Table::new(rows, widths)
        .header(Row::new(header).style(Style::new().fg(Color::Gray).bold()))
        .column_spacing(1)
        .row_highlight_style(if focused {
            Style::new().bg(Color::Rgb(40, 60, 80)).bold()
        } else {
            Style::new().bg(Color::Rgb(45, 45, 45))
        })
        .highlight_symbol("▌");

    let pos = app.selected_pos();
    let state = &mut app.active_filter_mut().table;
    state.select(pos);
    f.render_stateful_widget(table, area, state);
}

fn issue_row(i: &Issue, status: &str, status_w: usize, wide: bool) -> Row<'static> {
    let mut cells = vec![
        Cell::from(i.key.clone()).style(Style::new().fg(ACCENT)),
        Cell::from(truncate(status, status_w))
            .style(Style::new().fg(Color::White)),
    ];
    if wide {
        let p = i.priority.clone().unwrap_or_default();
        cells.push(Cell::from(p.clone()).style(Style::new().fg(priority_color(&p))));
    }
    cells.push(match &i.assignee {
        Some(name) => Cell::from(initials(name)).style(Style::new().fg(Color::LightBlue)),
        None => Cell::from("-").style(DIM),
    });
    cells.push(Cell::from(i.summary.clone()));
    Row::new(cells)
}

/// First letters of the first and last name: "Daniel Kallin" → "DK".
fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let first = words.first().and_then(|w| w.chars().next());
    let last = words
        .get(1..)
        .and_then(|rest| rest.last())
        .and_then(|w| w.chars().next());
    first
        .into_iter()
        .chain(last)
        .flat_map(char::to_uppercase)
        .collect()
}
