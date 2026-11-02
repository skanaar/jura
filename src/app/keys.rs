use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use super::*;

impl App {
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Normal => self.on_key_normal(key),
            Mode::Help => {}
            Mode::Search => self.on_key_search(key),
            Mode::Transition { key: issue, list } => self.on_key_transition(key, issue, list),
            Mode::Comment { key: issue, text } => self.on_key_comment(key, issue, text),
            Mode::Create { summary, list } => self.on_key_create(key, summary, list),
        }
    }

    pub(super) fn on_key_normal(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let in_detail = self.focus == Focus::Detail;
        let page = 10;
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Esc => {
                if in_detail {
                    self.focus = Focus::List;
                } else if !self.search.is_empty() {
                    self.search.clear();
                    self.sync_selection();
                }
            }
            KeyCode::Left | KeyCode::Char('h') if in_detail => self.switch_tab(-1),
            KeyCode::Right | KeyCode::Char('l') if in_detail => self.switch_tab(1),
            KeyCode::Down | KeyCode::Char('j') if in_detail => self.scroll(1),
            KeyCode::Up | KeyCode::Char('k') if in_detail => self.scroll(-1),
            KeyCode::PageDown | KeyCode::Char('d') if in_detail && (ctrl || key.code == KeyCode::PageDown) => {
                self.scroll(page)
            }
            KeyCode::PageUp | KeyCode::Char('u') if in_detail && (ctrl || key.code == KeyCode::PageUp) => {
                self.scroll(-page)
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(page as isize),
            KeyCode::PageUp => self.move_selection(-(page as isize)),
            KeyCode::Char('d') if ctrl => self.move_selection(page as isize),
            KeyCode::Char('u') if ctrl => self.move_selection(-(page as isize)),
            KeyCode::Home | KeyCode::Char('g') => self.move_selection(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => self.move_selection(isize::MAX / 2),
            KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => {
                self.switch_filter((self.active + 1) % self.filters.len())
            }
            KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left => {
                self.switch_filter((self.active + self.filters.len() - 1) % self.filters.len())
            }
            KeyCode::Char(c @ '1'..='9') => self.switch_filter(c as usize - '1' as usize),
            KeyCode::Enter if self.selected_issue().is_some() => {
                self.focus = if in_detail { Focus::List } else { Focus::Detail };
            }
            KeyCode::Char('p') => self.show_preview = !self.show_preview,
            KeyCode::Char('/') => {
                self.focus = Focus::List;
                self.mode = Mode::Search;
            }
            KeyCode::Char('r') => {
                self.refresh_filter(self.active);
                if let Some(k) = self.selected_key() {
                    self.ensure_detail(&k, true);
                }
            }
            KeyCode::Char('R') => self.refresh_all(),
            KeyCode::Char('o') => {
                if let Some(k) = self.selected_key()
                    && let Err(e) = open::that_detached(self.client.browse_url(&k))
                {
                    self.set_flash(format!("Could not open browser: {e}"), true);
                }
            }
            KeyCode::Char('t') => {
                if let Some(k) = self.selected_key() {
                    // Show cached transitions immediately, revalidate behind them.
                    self.fetch_transitions(&k);
                    let mut list = ListState::default();
                    list.select(Some(0));
                    self.mode = Mode::Transition { key: k, list };
                }
            }
            KeyCode::Char('c') => {
                if let Some(k) = self.selected_key() {
                    self.ensure_detail(&k, false);
                    self.mode = Mode::Comment { key: k, text: String::new() };
                }
            }
            KeyCode::Char('n') => {
                self.fetch_issue_types();
                self.mode = Mode::Create { summary: String::new(), list: ListState::default() };
            }
            KeyCode::Char('a') => {
                if let Some(k) = self.selected_key() {
                    self.assign(k, true);
                }
            }
            KeyCode::Char('A') => {
                if let Some(k) = self.selected_key() {
                    self.assign(k, false);
                }
            }
            _ => {}
        }
    }

    pub(super) fn switch_tab(&mut self, delta: isize) {
        let tab = self.detail_tab.shifted(delta);
        if tab != self.detail_tab {
            self.detail_tab = tab;
            self.detail_scroll = 0;
        }
    }

    pub(super) fn scroll(&mut self, delta: i32) {
        self.detail_scroll = (self.detail_scroll as i32 + delta).clamp(0, u16::MAX as i32) as u16;
    }

    pub(super) fn on_key_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.search.clear(),
            KeyCode::Enter => {}
            KeyCode::Backspace => {
                self.search.pop();
                self.mode = Mode::Search;
            }
            KeyCode::Char(c) => {
                self.search.push(c);
                self.mode = Mode::Search;
            }
            _ => self.mode = Mode::Search,
        }
        self.sync_selection();
    }

    pub(super) fn on_key_transition(&mut self, key: KeyEvent, issue: String, mut list: ListState) {
        let options = self.transitions.get(&issue).cloned().unwrap_or_default();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return,
            KeyCode::Down | KeyCode::Char('j') => list.select_next(),
            KeyCode::Up | KeyCode::Char('k') => list.select_previous(),
            KeyCode::Enter => {
                if let Some(t) = list.selected().and_then(|i| options.get(i)) {
                    self.apply_transition(issue, t.clone());
                }
                return;
            }
            _ => {}
        }
        self.mode = Mode::Transition { key: issue, list };
    }

    pub(super) fn on_key_comment(&mut self, key: KeyEvent, issue: String, mut text: String) {
        match key.code {
            KeyCode::Esc => return,
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => text.push('\n'),
            KeyCode::Enter => {
                let body = text.trim().to_string();
                if !body.is_empty() {
                    self.add_comment(issue, body);
                    self.focus = Focus::Detail;
                    return;
                }
            }
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Char(c) => text.push(c),
            _ => {}
        }
        self.mode = Mode::Comment { key: issue, text };
    }

    pub(super) fn on_key_create(&mut self, key: KeyEvent, mut summary: String, mut list: ListState) {
        match key.code {
            KeyCode::Esc => return,
            KeyCode::Down | KeyCode::Tab => list.select_next(),
            KeyCode::Up | KeyCode::BackTab => list.select_previous(),
            KeyCode::Enter => {
                let issue_type = list.selected().and_then(|i| self.issue_types.as_ref()?.get(i)).cloned();
                if let Some(t) = issue_type
                    && !summary.trim().is_empty()
                {
                    self.create_issue(summary.trim().to_string(), t);
                    return;
                }
            }
            KeyCode::Backspace => {
                summary.pop();
            }
            KeyCode::Char(c) => summary.push(c),
            _ => {}
        }
        self.mode = Mode::Create { summary, list };
    }
}
