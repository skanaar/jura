use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, Paragraph, Wrap};

use super::*;
use crate::app::{App, Mode};

fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(4));
    let h = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Blanks the popup plus a 1-cell margin around it so it doesn't touch what is behind.
fn clear_with_margin(f: &mut Frame, area: Rect) {
    f.render_widget(Clear, area.outer(ratatui::layout::Margin::new(1, 1)).intersection(f.area()));
}

/// Dialog footer: dim hints separated by a dash in the border colour.
fn hints(items: &[&str]) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" - ", Style::new().fg(ACCENT)));
        }
        spans.push(Span::styled(item.to_string(), DIM));
    }
    spans.push(Span::raw(" "));
    Line::from(spans).right_aligned()
}

pub(super) fn transitions(f: &mut Frame, app: &mut App) {
    let Mode::Transition { key, list } = &mut app.mode else {
        return;
    };
    let options = app.transitions.get(key.as_str());
    let loading = app.transitions_loading.contains(key.as_str());

    let height = options.map_or(3, |o| o.len().max(1) as u16 + 2);
    let area = popup(f.area(), 46, height);
    let mut block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT))
        .title(format!(" Move {key} "));
    if loading {
        block = block.title_bottom(
            Line::from(format!(" {} ", SPINNER[app.spinner % SPINNER.len()])).right_aligned(),
        );
    }
    clear_with_margin(f, area);

    match options {
        Some(opts) if !opts.is_empty() => {
            let items: Vec<ListItem> = opts
                .iter()
                .map(|t| {
                    let to = Span::styled(
                        app.cfg.status_label(&t.to).to_string(),
                        Style::new().fg(Color::White),
                    );
                    // Transitions are often named after their target status; show that once.
                    let spans = if t.name == t.to {
                        vec![to]
                    } else {
                        vec![Span::raw(t.name.clone()), Span::styled(" → ", DIM), to]
                    };
                    ListItem::new(Line::from(spans))
                })
                .collect();
            let widget = List::new(items)
                .block(block)
                .highlight_style(Style::new().bg(Color::Rgb(40, 60, 80)).bold())
                .highlight_symbol("▌");
            f.render_stateful_widget(widget, area, list);
        }
        Some(_) if !loading => f.render_widget(
            Paragraph::new("No transitions available")
                .style(DIM)
                .block(block),
            area,
        ),
        _ => f.render_widget(
            Paragraph::new("Loading transitions…")
                .style(DIM)
                .block(block),
            area,
        ),
    }
}

pub(super) fn comment(f: &mut Frame, app: &App) {
    let Mode::Comment { key, text } = &app.mode else {
        return;
    };
    let area = popup(f.area(), 80, 12);
    clear_with_margin(f, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT))
        .title(format!(" Comment on {key} "))
        .title_bottom(
            hints(&["Enter send", "Alt+Enter newline", "Esc cancel"]),
        );
    let mut lines: Vec<Line> = text
        .split('\n')
        .map(|l| Line::from(l.to_string()))
        .collect();
    if let Some(last) = lines.last_mut() {
        last.push_span(Span::styled("█", Style::new().fg(ACCENT)));
    }
    let inner_h = area.height.saturating_sub(2);
    let scroll = (lines.len() as u16).saturating_sub(inner_h);
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

pub(super) fn create(f: &mut Frame, app: &mut App) {
    let Mode::Create { summary, list } = &mut app.mode else {
        return;
    };
    let types = app.issue_types.as_deref().unwrap_or_default();
    // Land on the configured default the first time the types are on screen.
    if list.selected().is_none() && !types.is_empty() {
        let default = types.iter().position(|t| t.eq_ignore_ascii_case(&app.cfg.default_issue_type));
        list.select(Some(default.unwrap_or(0)));
    }

    let height = types.len().clamp(1, 8) as u16 + 5;
    let area = popup(f.area(), 60, height);
    clear_with_margin(f, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT))
        .title(format!(" New issue in {} ", app.cfg.project))
        .title_bottom(
            hints(&["Enter create", "↑/↓ type", "Esc cancel"]),
        );
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [summary_row, label_row, list_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner);

    f.render_widget(
        Line::from(vec![
            Span::styled("Summary ", DIM),
            Span::raw(summary.clone()),
            Span::styled("█", Style::new().fg(ACCENT)),
        ]),
        summary_row,
    );
    f.render_widget(Line::from("Type").style(DIM), label_row);

    if types.is_empty() {
        let msg = if app.issue_types_loading { "Loading issue types…" } else { "No issue types available" };
        f.render_widget(Paragraph::new(msg).style(DIM), list_area);
        return;
    }
    let items: Vec<ListItem> = types.iter().map(|t| ListItem::new(t.as_str())).collect();
    let widget = List::new(items)
        .highlight_style(Style::new().bg(Color::Rgb(40, 60, 80)).bold())
        .highlight_symbol("▌");
    f.render_stateful_widget(widget, list_area, list);
}

pub(super) fn help(f: &mut Frame) {
    let rows = [
        ("j / k, ↓ / ↑", "Move selection (or scroll details)"),
        ("g / G", "First / last issue"),
        ("PgUp / PgDn, Ctrl+u / d", "Page up / down"),
        ("Tab / Shift+Tab, h / l, 1-9", "Switch filter"),
        ("Enter", "Focus details / back to list"),
        ("← / → (in details)", "Switch Description / Info / Comments"),
        ("/", "Search within the current filter"),
        ("n", "New issue"),
        ("t", "Transition issue"),
        ("c", "Add comment"),
        ("a / A", "Assign to me / unassign"),
        ("o", "Open issue in browser"),
        ("p", "Toggle preview pane"),
        ("r / R", "Refresh this filter / all filters"),
        ("Esc", "Back / clear search"),
        ("q, Ctrl+c", "Quit"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("{k:>28}  "), Style::new().fg(ACCENT)),
                Span::raw(*v),
            ])
        })
        .collect();
    let area = popup(f.area(), 74, lines.len() as u16 + 4);
    clear_with_margin(f, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT))
        .title(" Keys ")
        .title_bottom(Line::from(" any key to close ").style(DIM).right_aligned());
    f.render_widget(Paragraph::new(lines).block(block), area);
}

